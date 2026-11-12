//! 引擎配置：设置层（core::setting）直接映射到这里。
//!
//! # 出厂默认值与设置 schema 的唯一来源（M193）
//!
//! 四个常量是**唯一**写死数字的地方：schema 的默认值/范围、`EngineConfig::default()`、
//! `Semaphore` 容量全从它们取。此前 schema 写 8、引擎写 64 ⇒ 调并发完全无效（引擎侧零消费者）。
//! 收成常量后「改一处另一个不变」会直接变红（守卫见 `shared.rs` 与本文件的用例）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use super::backoff::BackoffPolicy;
use super::chunked::ChunkedConfig;
use super::region::RegionProfile;
use super::types::ResumePolicy;

/// 全局并发许可数的出厂默认（`download.concurrency` 的 schema 默认值同源）。
pub const DEFAULT_MAX_CONCURRENCY: usize = 8;

/// 全局并发许可的**硬上限**（`download.concurrency` 的 schema 上界同源）。schema 在写入口就拒绝
/// 越界值，这里再夹一次是纵深防御（手改文件、老 schema 允许的大数）——9999 会把机器拖死。
pub const MAX_CONCURRENCY: usize = 64;

/// 每个源尝试次数的出厂默认（`download.retry_limit` 的 schema 默认值同源）。
pub const DEFAULT_MAX_ATTEMPTS_PER_SOURCE: u32 = 3;

/// 每个源尝试次数的硬上限（`download.retry_limit` 的 schema 上界同源）。
pub const MAX_ATTEMPTS_PER_SOURCE: u32 = 10;

/// 三个设置键名（层间契约）：登记在 `setting::keys`，读取在宿主。只写在这里，两侧都引用常量。
pub const KEY_CONCURRENCY: &str = "download.concurrency";
/// 见 [`KEY_CONCURRENCY`]。
pub const KEY_RETRY_LIMIT: &str = "download.retry_limit";
/// gh-proxy 前缀覆盖键（三态语义见 [`gh_proxy_override`]）。**已登记**：`setting::keys::proxy` 的
/// `specs()` 里有它（文本 / 出厂空串），presenter 与 CLI 的宿主都按这个常量读，读不到 ⇒
/// [`GhProxyOverride::Builtin`]。**绑定在哪（别当死键删）**：常量与 schema 登记的对应关系由
/// `setting::keys::proxy` 的 `the_gh_proxy_key_is_bound_to_the_schema_registration`（M205）钉住：写错就红；
/// 它抓不住两侧同时改名（公开改名）。行为退化由 `empty_gh_proxy_means_follow_builtin_not_off` 与 `shared` 的候选链拦。
pub const KEY_GH_PROXY: &str = "download.gh_proxy";

/// 引擎配置。
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// 全局并发许可数（tokio `Semaphore`），默认 [`DEFAULT_MAX_CONCURRENCY`]。
    pub max_concurrency: usize,
    /// 每个源的尝试次数上限，默认 [`DEFAULT_MAX_ATTEMPTS_PER_SOURCE`]。
    pub max_attempts_per_source: u32,
    /// 连接超时，默认 10s。
    pub connect_timeout: Duration,
    /// 单次读超时（读空闲判定，不是整体超时），默认 30s。
    pub read_timeout: Duration,
    /// User-Agent（唯一定义在 `http::USER_AGENT`；两条 HTTP 路径发的都是这一份，别在这里另拼）。
    pub user_agent: String,
    /// 进度事件节流间隔，默认 250ms。
    pub progress_throttle: Duration,
    /// 退避策略。
    pub backoff: BackoffPolicy,
    /// 分块下载配置。
    pub chunked: ChunkedConfig,
    /// 默认续传策略（请求级 `Never` 优先级更高）。
    pub resume: ResumePolicy,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_concurrency: DEFAULT_MAX_CONCURRENCY,
            max_attempts_per_source: DEFAULT_MAX_ATTEMPTS_PER_SOURCE,
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(30),
            user_agent: super::http::USER_AGENT.to_string(),
            progress_throttle: Duration::from_millis(250),
            backoff: BackoffPolicy::default(),
            chunked: ChunkedConfig::default(),
            resume: ResumePolicy::Conservative,
        }
    }
}

/// `download.gh_proxy` 的三种取值（三态语义见 [`gh_proxy_override`]）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GhProxyOverride {
    /// 空串：**没有覆盖**，跟随内置默认（地区判定 + 镜像表里的前缀）。
    #[default]
    Builtin,
    /// 明确关掉：一个 gh-proxy 候选都不生成（`RegionProfile::with_gh_proxy(None)`）。
    Off,
    /// 用这个前缀覆盖（仍然只在镜像优先 / 国内判定时启用）。
    Prefix(String),
}

/// 设置原文 → gh-proxy 三态（纯函数：`off` 与 URL 的边界在这里定死，不散落到宿主）。
///
/// **空串不等于关**：「我不想用加速」和「我还没决定」是两件事。空串 = 跟随内置默认，
/// 要关必须写 `off`（`none` / `direct` / `disable` / `disabled` / `no` 这些口语词一并认，
/// 大小写不敏感、前后空白忽略——用户拿哪个词表达「关掉」都合理，专门要求某一个会造出
/// 「填了没生效」的坑）。
#[must_use]
pub fn gh_proxy_override(raw: &str) -> GhProxyOverride {
    let text = raw.trim();
    if text.is_empty() {
        return GhProxyOverride::Builtin;
    }
    let lowered = text.to_ascii_lowercase();
    if matches!(
        lowered.as_str(),
        "off" | "none" | "direct" | "disable" | "disabled" | "no"
    ) {
        return GhProxyOverride::Off;
    }
    GhProxyOverride::Prefix(text.to_string())
}

/// 设置侧灌进来的下载调优（M193）：并发 / 每个源的尝试次数 / gh-proxy 覆盖。
///
/// 这是「`launcher.json` 的三个键」到「引擎装配参数」之间**唯一**的翻译层：键名由宿主读（core
/// 不认识设置键名），默认值、合法区间、回落策略都在这里——否则「UI 说 8、引擎跑 64」会从缝里长回来。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadTuning {
    /// 全局并发许可数（已夹在 `1..=MAX_CONCURRENCY`）。
    pub max_concurrency: usize,
    /// 每个源的尝试次数上限（已夹在 `1..=MAX_ATTEMPTS_PER_SOURCE`）。
    pub max_attempts_per_source: u32,
    /// gh-proxy 覆盖。
    pub gh_proxy: GhProxyOverride,
}

impl Default for DownloadTuning {
    fn default() -> Self {
        Self {
            max_concurrency: DEFAULT_MAX_CONCURRENCY,
            max_attempts_per_source: DEFAULT_MAX_ATTEMPTS_PER_SOURCE,
            gh_proxy: GhProxyOverride::Builtin,
        }
    }
}

impl DownloadTuning {
    /// 设置原文 → 调优（**未填 / 越界 / 非法值一律回落出厂默认**，绝不报错阻断下载）。
    ///
    /// 口径与 schema 的 `(min, max)` 同源但独立实现（用户可能手改文件绕过 `store.set`）：键不存在
    /// → 出厂默认；落在区间内 → 原值；负数 / 超上限 → 出厂默认（**不夹到边界**：那是「想 9999 给
    /// 64」，界面仍会说谎）。`retry_limit` 的 `0` 在区间内，落成「只试一次」（见实现里的注释）。
    #[must_use]
    pub fn from_settings(
        concurrency: Option<i64>,
        attempts: Option<i64>,
        gh_proxy: Option<&str>,
    ) -> Self {
        let default = Self::default();
        Self {
            max_concurrency: concurrency
                .and_then(|raw| usize::try_from(raw).ok())
                .filter(|raw| (1..=MAX_CONCURRENCY).contains(raw))
                .unwrap_or(default.max_concurrency),
            // `0` 是**合法值**（P2-1 复审：旧配置里真的有 0，且旧 GUI 把它当档位）：语义是
            // 「只试一次 = 不重试」，所以 0 与 1 都落成 1；负数 / 超上限才回落出厂默认。
            max_attempts_per_source: attempts
                .and_then(|raw| u32::try_from(raw).ok())
                .filter(|raw| *raw <= MAX_ATTEMPTS_PER_SOURCE)
                .map_or(default.max_attempts_per_source, |raw| raw.max(1)),
            gh_proxy: gh_proxy.map_or(GhProxyOverride::Builtin, gh_proxy_override),
        }
    }

    /// 把调优盖到 `EngineConfig` 上（其余字段保持传入值）。
    #[must_use]
    pub fn apply_to(&self, mut cfg: EngineConfig) -> EngineConfig {
        cfg.max_concurrency = self.max_concurrency;
        cfg.max_attempts_per_source = self.max_attempts_per_source;
        cfg
    }

    /// 把 gh-proxy 覆盖盖到档案上（`Builtin` 原样返回 = 跟随地区判定与镜像表）。
    #[must_use]
    pub fn apply_gh_proxy(&self, profile: RegionProfile) -> RegionProfile {
        match &self.gh_proxy {
            GhProxyOverride::Builtin => profile,
            GhProxyOverride::Off => profile.with_gh_proxy(None),
            GhProxyOverride::Prefix(prefix) => profile.with_gh_proxy(Some(prefix)),
        }
    }
}

/// 每个缓存根一份「宿主想要什么」（[`wanted`] 用）。按数据根而不是进程级：登记处本就是「一个数据根
/// 一个引擎」；进程级那份在多数据根时会互相踩（A 根改 2、B 根冲回 8）。
fn wanted_slot() -> &'static Mutex<HashMap<PathBuf, DownloadTuning>> {
    static WANTED: OnceLock<Mutex<HashMap<PathBuf, DownloadTuning>>> = OnceLock::new();
    WANTED.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 该数据根当前想要的调优（没灌过 = 出厂默认）。
#[must_use]
pub fn wanted(cache_root: &Path) -> DownloadTuning {
    wanted_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(cache_root)
        .cloned()
        .unwrap_or_default()
}

/// 记下该数据根想要的调优，返回**是否真的变了**（没变时调用方不该重建引擎）。
pub fn publish(cache_root: &Path, next: DownloadTuning) -> bool {
    let mut wanted = wanted_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // 归一化比较：「没灌过」与「灌了出厂默认」是同一件事，不能因为 `None != Some(default)`
    // 就把每次取引擎都当成「参数变了」（那会每次重建引擎、把连接池与健康度记忆全丢掉）。
    let current = wanted.get(cache_root).cloned().unwrap_or_default();
    if current == next {
        return false;
    }
    if next == DownloadTuning::default() {
        // 默认值不占格子（也顺手回收测试造的临时根）。
        wanted.remove(cache_root);
    } else {
        wanted.insert(cache_root.to_path_buf(), next);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setting::keys::{launcher_schema, launcher_version};

    /// **守门③**：schema 的默认值 / 区间 / 键名与 `EngineConfig::default()` / 调优默认值取同一组常量。
    /// 变异证明（只改实现侧）：`EngineConfig::default()` 的并发写回 64、schema 默认值写成 16、
    /// `MAX_CONCURRENCY` 改了而 schema 仍写 64 ⇒ 各自红。
    #[test]
    fn the_schema_contract_is_single_sourced() {
        let schema = launcher_schema();
        assert_eq!(schema.version(), launcher_version());
        let cfg = EngineConfig::default();
        let tune = DownloadTuning::default();

        // 并发 1~64（0 张许可没意义）；尝试次数 **0**~10（P2-1：0 = 只试一次，旧配置里真有）。
        let contract: [(&str, i64, i64, usize); 2] = [
            (
                KEY_CONCURRENCY,
                i64::try_from(DEFAULT_MAX_CONCURRENCY).expect("装得下"),
                1,
                MAX_CONCURRENCY,
            ),
            (
                KEY_RETRY_LIMIT,
                i64::from(DEFAULT_MAX_ATTEMPTS_PER_SOURCE),
                0,
                usize::try_from(MAX_ATTEMPTS_PER_SOURCE).expect("装得下"),
            ),
        ];
        for (key, default, min, max) in contract {
            let spec = schema.spec(key).expect("键已登记");
            assert_eq!(spec.default, serde_json::json!(default), "{key}");
            assert_eq!(
                spec.kind,
                crate::setting::spec::ValueKind::Int {
                    min,
                    max: i64::try_from(max).expect("装得下"),
                },
                "{key}"
            );
        }
        assert_eq!(cfg.max_concurrency, DEFAULT_MAX_CONCURRENCY);
        assert_eq!(tune.max_concurrency, DEFAULT_MAX_CONCURRENCY);
        assert_eq!(cfg.max_attempts_per_source, DEFAULT_MAX_ATTEMPTS_PER_SOURCE);
        assert_eq!(
            tune.max_attempts_per_source,
            DEFAULT_MAX_ATTEMPTS_PER_SOURCE
        );
    }

    /// `SettingSpec.doc` 是 `&'static str`（`concat!` 插不进常量），描述里的数字只能手写——这条钉住它，
    /// 改常量不改文案就红（`catalog.rs` 会把这些 doc 当 JSON schema 的 description 发出去）。
    #[test]
    fn the_key_docs_quote_the_real_limits() {
        let schema = launcher_schema();
        let doc = |key: &str| schema.spec(key).expect("键已登记").doc;
        assert!(doc(KEY_CONCURRENCY).contains(&format!("1~{MAX_CONCURRENCY}")));
        assert!(doc(KEY_CONCURRENCY).contains(&format!("出厂 {DEFAULT_MAX_CONCURRENCY}")));
        assert!(doc(KEY_RETRY_LIMIT).contains(&format!("出厂 {DEFAULT_MAX_ATTEMPTS_PER_SOURCE}")));
    }

    /// 未填 / 越界回落默认（不阻断下载）；**`retry_limit: 0` 是合法值**，语义 = 只试一次。
    ///
    /// 变异证明：把并发的区间过滤删掉 ⇒ `0` / `9999` 两条红；把尝试次数的 `.max(1)` 删掉 ⇒
    /// `0 → 1` 那条红（引擎会拿到 0 次尝试）；把 `0` 判成越界 ⇒ 同样红。
    #[test]
    fn missing_or_illegal_values_fall_back_to_the_defaults() {
        assert_eq!(
            DownloadTuning::from_settings(None, None, None),
            DownloadTuning::default()
        );

        let just_over = i64::try_from(MAX_CONCURRENCY).expect("上限装得下") + 1;
        for bad in [0_i64, -1, 9_999, just_over] {
            let tune = DownloadTuning::from_settings(Some(bad), None, None);
            assert_eq!(tune.max_concurrency, DEFAULT_MAX_CONCURRENCY, "{bad}");
        }
        // 0 与 1 都 = 只试一次：旧配置里的 0 不许回落成出厂 3（那正是「界面 0、引擎 3」）。
        for zero_or_one in [0_i64, 1] {
            let tune = DownloadTuning::from_settings(None, Some(zero_or_one), None);
            assert_eq!(tune.max_attempts_per_source, 1, "{zero_or_one}");
        }
        for bad in [-1_i64, 9_999] {
            let tune = DownloadTuning::from_settings(None, Some(bad), None);
            assert_eq!(
                tune.max_attempts_per_source,
                DEFAULT_MAX_ATTEMPTS_PER_SOURCE
            );
        }

        let tune = DownloadTuning::from_settings(Some(2), Some(2), None);
        assert_eq!(tune.max_concurrency, 2);
        assert_eq!(tune.max_attempts_per_source, 2);
    }

    /// 三态：**空串 ≠ 关**（这条是「设置项说假话」的高发点，M197 的键语义同源）。
    #[test]
    fn empty_gh_proxy_means_follow_builtin_not_off() {
        assert_eq!(gh_proxy_override(""), GhProxyOverride::Builtin);
        assert_eq!(gh_proxy_override("   "), GhProxyOverride::Builtin);
        for off in [
            "off", "OFF", " none ", "direct", "disable", "disabled", "no",
        ] {
            assert_eq!(gh_proxy_override(off), GhProxyOverride::Off, "{off}");
        }
        assert_eq!(
            gh_proxy_override(" https://gh.example/ "),
            GhProxyOverride::Prefix("https://gh.example/".to_string())
        );
        // 未填（键不存在）也是 Builtin，不是 Off。
        assert_eq!(
            DownloadTuning::from_settings(None, None, None).gh_proxy,
            GhProxyOverride::Builtin
        );
    }

    /// `apply_gh_proxy` 的三档真的改到档案上、并且**改到候选链上**（`expand()` 读的就是档案里那个
    /// 前缀；自定义前缀那一档的链断言在 `shared.rs`）。变异证明：`Off` 分支改成原样返回 ⇒ `off` 红。
    #[test]
    fn gh_proxy_override_reaches_the_profile_and_the_chain() {
        use crate::download::mirrors::{MirrorTable, SOURCE_GH_PROXY};
        use crate::download::types::ResourceKind;

        let url = "https://github.com/o/r/releases/download/v1/a.jar";
        let table = MirrorTable::builtin();
        let sources = |profile: &RegionProfile| -> Vec<String> {
            table
                .expand(url, ResourceKind::Other, profile)
                .into_iter()
                .map(|candidate| candidate.source)
                .collect()
        };

        let mirror_first = RegionProfile::mirror_first(0);
        let kept = DownloadTuning::default().apply_gh_proxy(mirror_first.clone());
        assert_eq!(kept.gh_proxy, mirror_first.gh_proxy, "Builtin 不该动档案");
        assert!(
            sources(&kept).contains(&SOURCE_GH_PROXY.to_string()),
            "出厂套前缀"
        );

        let off = DownloadTuning::from_settings(None, None, Some("off"))
            .apply_gh_proxy(mirror_first.clone());
        assert!(
            !sources(&off).contains(&SOURCE_GH_PROXY.to_string()),
            "关了就没候选"
        );

        let prefix = DownloadTuning::from_settings(None, None, Some("https://gh.example/"))
            .apply_gh_proxy(mirror_first);
        assert_eq!(prefix.gh_proxy.as_deref(), Some("https://gh.example/"));
        assert!(prefix.gh_proxy_enabled(), "前缀生效");
    }

    /// 每个数据根的调优互不串味（一个根改成 2 不该改另一个根的引擎）。
    /// 变异证明：把 `wanted`/`publish` 的键改回单一进程级槽位 ⇒ 第二个根读到 2，红。
    #[test]
    fn tuning_is_per_cache_root_and_no_change_is_reported_as_such() {
        let a = PathBuf::from("/tmp/ull-tuning-a");
        let b = PathBuf::from("/tmp/ull-tuning-b");
        assert_eq!(wanted(&a), DownloadTuning::default());

        let changed = DownloadTuning::from_settings(Some(2), None, None);
        assert!(publish(&a, changed.clone()), "真变了要报变更");
        assert_eq!(wanted(&a), changed);
        assert!(!publish(&a, changed.clone()), "同值不该报变更");
        assert_eq!(wanted(&b), DownloadTuning::default(), "别的根不受影响");

        assert!(
            publish(&a, DownloadTuning::default()),
            "恢复默认也是一次变更"
        );
        assert_eq!(wanted(&a), DownloadTuning::default());
    }
}
