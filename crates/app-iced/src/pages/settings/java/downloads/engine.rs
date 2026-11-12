//! 带进度发射器的下载引擎（M197）：一次 JDK 安装一个实例 + `download.gh_proxy` 的真消费者。
//!
//! 从 `downloads.rs` 拆出来（行数 + 口径独立：这里只说「为什么不用共享引擎」）。
//! 装配口径与 `core::download::shared::build` 一致，只多挂一个 `ProgressEmitter`。
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
use unlucky_core::download::config::KEY_GH_PROXY;
use unlucky_core::download::region::{decide_from, load_cached};
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

/// 带进度发射器的引擎（装配口径与 `download::shared::build` 一致，只多挂一个 emitter + gh-proxy 覆盖）。
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
) -> DownloadEngine {
    let now = now_ms();
    let profile = load_cached(&cache_root.join("region.json"), now).unwrap_or_else(|| {
        let profile = decide_from(&detect_signals(), now);
        let _ = std::fs::create_dir_all(cache_root);
        profile
    });
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
