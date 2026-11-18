//! 带进度发射器的下载引擎（M197）：一次 JDK 安装一个实例 + `download.*` 的真消费者。
//!
//! 从 `downloads.rs` 拆出来（行数 + 口径独立：这里只说「为什么不用共享引擎」）。
//!
//! ## 与 `core::download::shared::build` 的关系（别读成「一样」）
//!
//! 一个数据根上**有两处** `DownloadEngine::new`：共享登记处（`shared::build`，GUI 主路径与 CLI
//! 都走它）和这里（JDK 下载页，因为 `with_progress` 要**拿走**引擎）。差别是实打实的：
//!
//! | | `shared::build` | 这里 |
//! |---|---|---|
//! | 缓存 / 健康度 | 该数据根 | 同一份（盘上共用） |
//! | 设置调优（并发 / 重试 / 三段超时） | `wanted(cache_root)`（M193/M292/M289） | **`EngineConfig::default()`** —— 未接，已知差 |
//! | 源选择（`region` / `mirror_preference`） | `apply_source_selection`（M294） | 同一入口（M294 r1） |
//! | gh-proxy 三态 | `DownloadTuning::apply_gh_proxy` | 本文件的 [`GhProxy`]（M197） |
//! | 进度事件 | `ProgressEmitter::silent()` | 这次作业的 emitter |
//!
//! 第一行「调优」是**没接**的差：JDK 页不理会 `download.concurrency` / 超时键。要接就把它并进
//! `DownloadTuning` 一起用 `wanted(cache_root)`——那是另一个决定面，不在 M294 里顺手做。
//! 「源选择」那一行本轮接上了（r1 复审 p1：设置页改「地区/镜像偏好」在这条路径上没生效），
//! 并且走的是**与共享引擎同一个翻译层**（`DownloadTuning::with_source_selection`），宿主里没有
//! 第二份字符串匹配。
//!
//! ## gh-proxy 的取值链（任务书那句「（默认值可改）」的落点）
//!
//! | `download.gh_proxy` | [`GhProxy`] | 效果 |
//! |---|---|---|
//! | 空（出厂） | [`GhProxy::Builtin`] | 跟随内置默认：`region.json` 判成镜像优先时用 `mirrors.json` 里的 `gh-proxy.com` |
//! | `off` / `none` / `direct` / `disable` | [`GhProxy::Off`] | `RegionProfile::with_gh_proxy(None)` —— 一个 gh-proxy 候选都不生成 |
//! | 其它非空值 | [`GhProxy::Prefix`] | 用这个前缀覆盖（仍然只在镜像优先/国内判定时启用） |
//!
//! **为什么只改 `RegionProfile`**：真正决定「要不要生成 gh-proxy 候选」的是
//! `MirrorTable::gh_proxy_candidates`，它读的是 `region.gh_proxy` 与 `region.mode`；表里的
//! `gh_proxies` 只是「已知可用前缀」的留档（**无生产读者**）。M193 已经把 `set_gh_proxy` /
//! `default_gh_proxy` 从表上删掉了，原因正是它们改的是那张没人读的表。所以本文件只动档案，
//! 并在 `downloads/tests.rs` 里用**候选链出现该前缀**来证明它真的生效。
//!
//! 内置默认值本身没有动（`mirrors.json` 仍是 `https://gh-proxy.com/`）：空串这一档等于
//! 「老文件没有这个键」的行为。

use std::path::Path;
use std::sync::Arc;

use unlucky_core::download::cache::now_ms;
use unlucky_core::download::config::{
    DownloadTuning, KEY_GH_PROXY, KEY_MIRROR_PREFERENCE, KEY_REGION,
};
use unlucky_core::download::region::{decide_from, load_cached, RegionProfile};
use unlucky_core::download::region_system::detect_signals;
use unlucky_core::download::{
    ContentCache, DownloadEngine, EngineConfig, HealthRegistry, MirrorTable, ProgressEmitter,
};
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;

/// `download.gh_proxy` 的三种取值。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GhProxy {
    /// 跟随内置默认（`region.json` 的判定 + `mirrors.json` 的前缀）
    #[default]
    Builtin,
    /// 明确关掉（`with_gh_proxy(None)`）
    Off,
    /// 用这个前缀覆盖
    Prefix(String),
}

/// 设置原文 → 取值（纯函数：`off` 与 URL 的边界在这里定死，不散落在别处）。
///
/// `off` / `none` / `direct` / `disable` 都认（大小写不敏感、前后空白忽略）：用户拿哪个词表达
/// 「关掉」都合理，专门要求某一个反而会造出「填了没生效」的坑。
#[must_use]
pub fn gh_proxy_from(raw: &str) -> GhProxy {
    let text = raw.trim();
    if text.is_empty() {
        return GhProxy::Builtin;
    }
    let lowered = text.to_ascii_lowercase();
    if matches!(
        lowered.as_str(),
        "off" | "none" | "direct" | "disable" | "disabled" | "no"
    ) {
        return GhProxy::Off;
    }
    GhProxy::Prefix(text.to_string())
}

/// 从 `launcher.json` 读 `download.gh_proxy`（读不到 / 没有文件 = 跟随内置默认）。
///
/// 键名走 core 的 [`KEY_GH_PROXY`]，不写字面量：同一件事的第二个字符串就是下一次「改名只改一处」的种子
/// （M207：重试次数、UA 之后又一处副本）。绑定关系由 core 的
/// `setting::keys::proxy::the_gh_proxy_key_is_bound_to_the_schema_registration` 钉住。
#[must_use]
pub fn gh_proxy_of_config(config: &Path) -> GhProxy {
    let store = SettingStore::load(config, launcher_schema());
    gh_proxy_from(store.doc().effective_str(KEY_GH_PROXY).unwrap_or_default())
}

/// 一次 JDK 下载要用的两个「源选择」键的**原文**（`download.region` / `download.mirror_preference`）。
///
/// **显式传值**：由按钮回调（`app/update/settings/java/downloads.rs`）从同一份 `launcher.json` 读出、
/// 挂进 [`Request`](crate::pages::settings::java::downloads::Request) 随作业传到这里，「设置从哪来」
/// 在调用处一眼可读。不从缓存根反推配置路径——反推失败就静默按 `auto` 收场，正是这条 mission 要
/// 消灭的那种「设置被无声忽略」。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceSelection {
    /// `download.region` 原文（`None` = 键不存在 / 不是字符串）。
    pub region: Option<String>,
    /// `download.mirror_preference` 原文（同上）。
    pub mirror_preference: Option<String>,
}

impl SourceSelection {
    /// 从 `launcher.json` 把两个键**读出来**（文件不存在 / 键没写 ⇒ `effective_str` 给 schema 默认
    /// 值 `"auto"`；非字符串值才是 `None`。两者在 core 那里同义，都落成 `Auto`）。
    ///
    /// **这里不做默认值回落、也不做取值校验**：`auto` 的语义、大小写与空白的归一、未知值怎么退，
    /// 全部由 [`DownloadTuning::with_source_selection`] 负责（全仓唯一一处）。在这个函数里再加一层
    /// 判断，就是「两份口径」的种子——GUI 认一套、CLI 认另一套。
    #[must_use]
    pub fn of_config(config: &Path) -> Self {
        let store = SettingStore::load(config, launcher_schema());
        let doc = store.doc();
        Self {
            region: doc.effective_str(KEY_REGION).map(str::to_string),
            mirror_preference: doc.effective_str(KEY_MIRROR_PREFERENCE).map(str::to_string),
        }
    }

    /// 叠到地区档案上：显式设置压过探测结论（`gh_proxy` 留 `Builtin`，前缀仍由调用点那三态决定）。
    fn apply(&self, profile: RegionProfile) -> RegionProfile {
        DownloadTuning::default()
            .with_source_selection(self.region.as_deref(), self.mirror_preference.as_deref())
            .apply_source_selection(profile)
    }
}

/// 带进度发射器的引擎（装配口径见模块头的对照表，**不等于** `shared::build`）。
///
/// 为什么不再走 `shared::engine_for`：那个登记处交出的是 `Arc<DownloadEngine>`，而挂进度要
/// 拿走引擎（`with_progress(self)`）。要让共享引擎带「按作业」的观察者，得改引擎内部的
/// `progress` 字段为观察者名单——那是引擎侧（M193 的镜像/并发那摊）的范围。
/// 代价：一次 JDK 安装有自己的连接池；**盘上的** `health.json` / `index.json` / `region.json`
/// 是同一个，所以换源经验仍然跨作业累积。
pub(super) fn engine_with(
    cache_root: &Path,
    progress: ProgressEmitter,
    gh_proxy: &GhProxy,
    selection: &SourceSelection,
) -> DownloadEngine {
    let now = now_ms();
    let profile = load_cached(&cache_root.join("region.json"), now).unwrap_or_else(|| {
        let profile = decide_from(&detect_signals(), now);
        let _ = std::fs::create_dir_all(cache_root);
        profile
    });
    // 显式地区 / 镜像偏好先叠（M294）：`DownloadTuning::default()` 的 gh_proxy 是 `Builtin`
    // （恒等），所以这一步只动「谁在前」；前缀仍由下面那三态决定——那是 JDK 页 `off` 档的唯一
    // 真相源，不在这里再翻译一次。
    let profile = selection.apply(profile);
    let profile = match gh_proxy {
        GhProxy::Builtin => profile,
        GhProxy::Off => profile.with_gh_proxy(None),
        GhProxy::Prefix(prefix) => profile.with_gh_proxy(Some(prefix)),
    };
    DownloadEngine::new(
        EngineConfig::default(),
        Arc::new(MirrorTable::builtin()),
        Arc::new(profile),
        Arc::new(HealthRegistry::open_in(cache_root, now)),
        Arc::new(ContentCache::open(cache_root)),
    )
    .with_progress(progress)
}

#[cfg(test)]
mod tests {
    //! JDK 下载页这个**独立装配点**的守门（M294 r1 复审 P1-1）。
    //!
    //! 不进 `downloads/tests.rs`：那一族守的是作业/快照，而这里守的是装配点本身，测试就在被守的
    //! 函数旁边（`engine_with` 是 `pub(super)`，同文件内可见）。

    use super::*;
    use std::path::PathBuf;
    use unlucky_core::download::region::{store_cached, RegionProfile};
    use unlucky_core::download::types::{DownloadRequest, ResourceKind};

    const LIBRARIES_URL: &str =
        "https://libraries.minecraft.net/com/google/guava/guava/32.0.0/guava-32.0.0.jar";

    /// 一次 JDK 下载的临时数据根（`<dir>/cache` + `<dir>/config`，与 `prefs_store` 的布局一致）。
    fn root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ull-jdk-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("cache")).expect("cache");
        std::fs::create_dir_all(dir.join("config")).expect("config");
        dir
    }

    /// 这个数据根的 `launcher.json`（与 `prefs_store` 的布局一致：`<home>/config/launcher.json`）。
    fn config_of(dir: &Path) -> PathBuf {
        dir.join("config").join("launcher.json")
    }

    fn write_config(dir: &Path, key: &str, value: &str) {
        let mut store = SettingStore::load(config_of(dir), launcher_schema());
        store
            .set(key, value.to_string().into())
            .unwrap_or_else(|err| panic!("schema 认识 {key}：{err}"));
        store.save().expect("写 launcher.json");
    }

    fn clear_config(dir: &Path) {
        std::fs::write(config_of(dir), b"{}").expect("清空 launcher.json");
    }

    fn plan_sources(engine: &DownloadEngine, dest: &Path) -> Vec<String> {
        let request = DownloadRequest::new(
            ResourceKind::Library,
            LIBRARIES_URL.to_string(),
            dest.to_path_buf(),
        );
        engine
            .plan(&request, 0)
            .into_iter()
            .map(|candidate| candidate.source)
            .collect()
    }

    /// **P1-1 判据**：`download.region` / `download.mirror_preference` 改得动 JDK 下载页的候选顺序。
    ///
    /// 两段都让设置与 `region.json` 的探测结论**相反**，所以「装配点没读设置」时断言必红
    /// （未接线时两段的第一条都还是探测值 `bmclapi` / `official`）。
    ///
    /// 变异证明：删掉 `engine_with` 里那句 `apply_source_selection`（换回裸档案）⇒ 两段顺序各自红。
    /// 抓不住：真机上下载 JDK 时实际命中哪个镜像（离线，没有网络证据）。
    #[test]
    fn the_source_selection_settings_reach_the_jdk_download_engine() {
        let dir = root("source-selection");
        let cache = dir.join("cache");
        let now = now_ms();

        // 探测=镜像优先，设置 official ⇒ 官方第一条。
        store_cached(
            &cache.join("region.json"),
            &RegionProfile::mirror_first(now),
        )
        .expect("档案");
        write_config(&dir, KEY_MIRROR_PREFERENCE, "official");
        let selection = SourceSelection::of_config(&config_of(&dir));
        let engine = engine_with(
            &cache,
            ProgressEmitter::silent(),
            &GhProxy::Builtin,
            &selection,
        );
        assert_eq!(
            plan_sources(&engine, &dir.join("a.jar")),
            ["official", "bmclapi"],
            "JDK 下载页也必须认 mirror_preference"
        );

        // 探测=直连，设置 cn ⇒ 镜像第一条（先清掉上一段的 official：它优先级更高）。
        store_cached(&cache.join("region.json"), &RegionProfile::direct(now)).expect("档案");
        clear_config(&dir);
        write_config(&dir, KEY_REGION, "cn");
        let selection = SourceSelection::of_config(&config_of(&dir));
        let engine = engine_with(
            &cache,
            ProgressEmitter::silent(),
            &GhProxy::Builtin,
            &selection,
        );
        assert_eq!(
            plan_sources(&engine, &dir.join("a.jar")),
            ["bmclapi", "official"],
            "JDK 下载页也必须认 region"
        );

        // 不设 ⇒ 回到探测结论（这一条与 M294 之前逐值相同）。清空后 `effective_str` 给的是
        // schema 默认值 `auto`（不是 `None`）——两者在 core 那里同义，都落成 `Auto`。
        clear_config(&dir);
        let selection = SourceSelection::of_config(&config_of(&dir));
        assert_eq!(selection.region.as_deref(), Some("auto"), "{selection:?}");
        let engine = engine_with(
            &cache,
            ProgressEmitter::silent(),
            &GhProxy::Builtin,
            &selection,
        );
        assert_eq!(
            plan_sources(&engine, &dir.join("a.jar")),
            ["official", "bmclapi"],
            "不设 = 探测结论（直连档案）"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
