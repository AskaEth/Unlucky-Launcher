//! 引擎配置：设置层（core::setting）直接映射到这里。
//!
//! # 出厂默认值与设置 schema 的唯一来源（M193；M292 加上三个超时/阈值常量）
//!
//! 这些 `DEFAULT_* / MAX_*` 常量是**唯一**写死数字的地方：schema 的默认值/范围、
//! `EngineConfig::default()`、`Semaphore` 容量、三个超时键的回落值全从它们取。此前 schema 写 8、
//! 引擎写 64 ⇒ 调并发完全无效（引擎侧零消费者）。收成常量后「改一处另一个不变」会直接变红
//! （守卫见 `shared.rs` 与本模块 `tests.rs` 的用例）。
//!
//! # 本文件与 `tuning` 的分工
//!
//! * 本文件：**引擎要什么**（[`EngineConfig`]）、数字的出厂值与上下界、gh-proxy 三态、
//!   按数据根的调优登记处（[`wanted`] / [`publish`]）。
//! * `tuning`：**设置原文怎么变成这些数**（`DownloadTuning` / `RawTuning` / 回落与夹取）。
//!   两者拆开只为守全仓 400 行硬线，类型路径没变（下面那行 `pub use`）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use super::backoff::BackoffPolicy;
use super::chunked::ChunkedConfig;
use super::proxy::ProxySetting;
use super::stall::{self, Deadlines};
use super::types::ResumePolicy;

mod tuning;

pub use tuning::{DownloadTuning, RawTuning};

/// 全局并发许可数的出厂默认（`download.concurrency` 的 schema 默认值同源）。
pub const DEFAULT_MAX_CONCURRENCY: usize = 8;

/// 全局并发许可的**硬上限**（`download.concurrency` 的 schema 上界同源）。schema 在写入口就拒绝
/// 越界值，这里再夹一次是纵深防御（手改文件、老 schema 允许的大数）——9999 会把机器拖死。
pub const MAX_CONCURRENCY: usize = 64;

/// 每个源尝试次数的出厂默认（`download.retry_limit` 的 schema 默认值同源）。
pub const DEFAULT_MAX_ATTEMPTS_PER_SOURCE: u32 = 3;

/// 每个源尝试次数的硬上限（`download.retry_limit` 的 schema 上界同源）。
pub const MAX_ATTEMPTS_PER_SOURCE: u32 = 10;

/// 分块作业平均吞吐下限的**出厂**值（KiB/s = 32，取自 [`stall::MIN_AVG_THROUGHPUT`]）。
///
/// `download.throughput_floor_kib` 的 schema 默认值、[`EngineConfig::default`] 与
/// [`DownloadTuning::default`] 同源；引擎里存的是**字节/秒**，KiB→字节的换算只在
/// [`DownloadTuning::from_raw`] 一处做。
pub const DEFAULT_THROUGHPUT_FLOOR_KIB: u32 = (stall::MIN_AVG_THROUGHPUT / 1024) as u32;

/// 吞吐下限的可设下界（KiB/s）。4 KiB/s 对 188MB 已经是 800 min 的预算——再低就不是「慢」，
/// 而是「这条链路下不完，别继续占着许可」。
pub const MIN_THROUGHPUT_FLOOR_KIB: u32 = 4;

/// 吞吐下限的可设上界（KiB/s = 4 MiB/s）。再高会让 188MB 的预算掉到 47s，等于把「慢但有效」
/// 一刀切死；想更快该换源/换镜像，不是压预算。
pub const MAX_THROUGHPUT_FLOOR_KIB: u32 = 4096;

/// 分块作业「已这么久没有任何新字节」的出厂窗口（秒）。
pub const DEFAULT_CHUNK_STALL: Duration = Duration::from_secs(120);

/// 无进展窗口的可设下界（秒）：比 30s 更短会把正常的调度抖动当成失速。
pub const MIN_CHUNK_STALL_SECS: u32 = 30;

/// 无进展窗口的可设上界（秒，10 min）。**这道上界就是「把挂住变成无界」的兜底**：它是
/// *单次尝试* 的界，最坏总等待还要乘「候选数 × 每源尝试次数」，所以再放宽一档就是几小时起步。
pub const MAX_CHUNK_STALL_SECS: u32 = 600;

/// 单次请求（`send()` → 响应头到手）上界的出厂值（秒）。
pub const DEFAULT_RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);

/// 等响应头的可设下界（秒）。
pub const MIN_RESPONSE_TIMEOUT_SECS: u32 = 10;

/// 等响应头的可设上界（秒，10 min）：回源冷启动再慢也不该超过这个量级。
pub const MAX_RESPONSE_TIMEOUT_SECS: u32 = 600;

/// 三个设置键名（层间契约）：登记在 `setting::keys`，读取在宿主。只写在这里，两侧都引用常量。
pub const KEY_CONCURRENCY: &str = "download.concurrency";
/// 见 [`KEY_CONCURRENCY`]。
pub const KEY_RETRY_LIMIT: &str = "download.retry_limit";
/// 分块作业平均吞吐下限（KiB/s）。语义与「改它要付什么代价」见 [`stall`] 模块头。
pub const KEY_THROUGHPUT_FLOOR: &str = "download.throughput_floor_kib";
/// 分块作业无进展窗口（秒）。见 [`KEY_THROUGHPUT_FLOOR`] 的那处文档。
pub const KEY_CHUNK_STALL: &str = "download.stall_timeout_secs";
/// 单次请求等响应头的上界（秒）。见 [`KEY_THROUGHPUT_FLOOR`] 的那处文档。
pub const KEY_RESPONSE_TIMEOUT: &str = "download.response_timeout_secs";
/// gh-proxy 前缀覆盖键（三态语义见 [`gh_proxy_override`]）。**已登记**：`setting::keys::proxy` 的
/// `specs()` 里有它（文本 / 出厂空串），presenter 与 CLI 的宿主都按这个常量读，读不到 ⇒
/// [`GhProxyOverride::Builtin`]。**绑定在哪（别当死键删）**：常量与 schema 登记的对应关系由
/// `setting::keys::proxy` 的 `the_gh_proxy_key_is_bound_to_the_schema_registration`（M205）钉住：写错就红；
/// 它抓不住两侧同时改名（公开改名）。行为退化由 `empty_gh_proxy_means_follow_builtin_not_off` 与 `shared` 的候选链拦。
pub const KEY_GH_PROXY: &str = "download.gh_proxy";
/// 「地区判定」键（`auto` / `cn` / `global`）。消费方与回落规则见 `download::preference` 模块头；
/// 读它的是宿主（presenter 的 `tuning_of`），翻译成 [`DownloadTuning::with_source_selection`]。
pub const KEY_REGION: &str = "download.region";
/// 「镜像偏好」键（`auto` / `official` / `mirror`）。读法与优先级同 [`KEY_REGION`]。
pub const KEY_MIRROR_PREFERENCE: &str = "download.mirror_preference";
/// 「代理模式」键（`none` / `system` / `manual`）。读它的是宿主，翻译成 [`ProxySetting`]
/// （三档语义、`proxy_url` 在非 manual 档的处置、非法地址怎么回落，**全写在** [`super::proxy`] 的
/// 模块头——别在宿主里再解释一次字符串）。
pub const KEY_PROXY_MODE: &str = "download.proxy_mode";
/// 「代理地址」键（**只在 `manual` 档生效**，其它档忽略，见 [`KEY_PROXY_MODE`]）。
pub const KEY_PROXY_URL: &str = "download.proxy_url";

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
    /// 等响应头的上界（**单次请求**：`send()` 到响应头为止，不含正文），默认
    /// [`DEFAULT_RESPONSE_TIMEOUT`]（60s），可被 `download.response_timeout_secs` 改。
    ///
    /// 取数与「为什么不是整作业总预算」见 [`super::stall`] 模块头那一处，别在调用点另写一套理由。
    pub response_timeout: Duration,
    /// 分块作业「已这么久没有任何新字节」的判定窗口，默认 [`DEFAULT_CHUNK_STALL`]（120s），
    /// 可被 `download.stall_timeout_secs` 改。
    ///
    /// 抓的是「读超时不响、但整个作业不动」的那类卡住（分块任务卡在写盘上）。
    pub chunk_stall_timeout: Duration,
    /// 分块作业总期限的下限，默认 10min（实际期限取它与「按平均吞吐下限反推的时长」的较大者，
    /// 见 [`super::stall::job_budget`]）。**没有**设置键：小作业的期限不该由用户填。
    pub chunk_job_floor: Duration,
    /// 分块作业的**平均吞吐下限**（字节/秒），默认 [`stall::MIN_AVG_THROUGHPUT`]（32 KiB/s），
    /// 可被 `download.throughput_floor_kib` 改（那是 KiB/s 口径）。
    ///
    /// 总期限 = `总字节 / 它`；调低它 ⇒ 慢链路能下完、但「真挂住」的作业占用更久。
    /// 代价与上界见 [`super::stall`] 模块头「改这几个数要付的代价」。
    pub min_avg_throughput: u64,
    /// 整文件重算摘要（校验阶段）的上界，默认 120s。
    ///
    /// 正常盘上 188MB 的 SHA-1 不到 1s；这条只抓「校验被卡住」，不参与慢链路判定。
    pub verify_timeout: Duration,
    /// 等并发许可的上界，默认 120min。
    ///
    /// 必须 ≥ 单个作业的总期限（最坏 ≈100 min 预算 + 校验），否则排队本身会被误杀；
    /// 它挡的是「许可被永久占住」。取数见 [`super::stall`] 模块头。
    pub permit_timeout: Duration,
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
    /// 代理三态（`download.proxy_mode` / `download.proxy_url` 的翻译结论，唯一翻译层在
    /// [`super::proxy`]）。
    ///
    /// **默认 [`ProxySetting::System`]，与 `download.proxy_mode` 的 schema 出厂值（`none`）故意不同**：
    /// 这一档是「这个子系统**没读**设置」时的行为（`auth` 的登录、发布版 GUI 的 JDK 下载页、地区探测
    /// 都直接 `EngineConfig::default()`），保持 M295 之前「跟随环境变量代理」才不会静默改掉它们的
    /// 网络路径。读了设置的装配点（`shared::build` / `engine_with`）由 `DownloadTuning` 显式盖上去，
    /// 那一路的出厂值是 `none`（真直连，与设置页画的那颗 chip 一致）。两处默认为什么必须不同，
    /// 见 [`super::proxy`] 模块头。
    pub proxy: ProxySetting,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_concurrency: DEFAULT_MAX_CONCURRENCY,
            max_attempts_per_source: DEFAULT_MAX_ATTEMPTS_PER_SOURCE,
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(30),
            response_timeout: DEFAULT_RESPONSE_TIMEOUT,
            chunk_stall_timeout: DEFAULT_CHUNK_STALL,
            chunk_job_floor: Duration::from_secs(600),
            min_avg_throughput: stall::MIN_AVG_THROUGHPUT,
            verify_timeout: Duration::from_secs(120),
            permit_timeout: Duration::from_secs(120 * 60),
            user_agent: super::http::USER_AGENT.to_string(),
            progress_throttle: Duration::from_millis(250),
            backoff: BackoffPolicy::default(),
            chunked: ChunkedConfig::default(),
            resume: ResumePolicy::Conservative,
            proxy: ProxySetting::System,
        }
    }
}

impl EngineConfig {
    /// 两段共存的界（等响应头 / 等下一块字节）：所有 HTTP 读路径都从这里取，别各自读字段。
    #[must_use]
    pub const fn deadlines(&self) -> Deadlines {
        Deadlines {
            response: self.response_timeout,
            read_idle: self.read_timeout,
        }
    }

    /// 分块作业的总期限（总长未知时只用下限）。
    #[must_use]
    pub fn chunk_job_budget(&self, expected: Option<u64>) -> Duration {
        expected.map_or(self.chunk_job_floor, |total| {
            stall::job_budget(total, self.chunk_job_floor, self.min_avg_throughput)
        })
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
#[path = "config/tests.rs"]
mod tests;
