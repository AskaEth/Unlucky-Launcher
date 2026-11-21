//! 设置原文 → [`EngineConfig`] 的翻译层（M292 从 `config.rs` 拆出：那边加完三个超时键就 423 行、
//! 踩了全仓 400 行硬线）。
//!
//! 搬移范围：`DownloadTuning` / `RawTuning` / `secs_in_range` 与 `impl DownloadTuning` 的全部方法，
//! 正文一字未改；`config.rs` 用 `pub use tuning::{DownloadTuning, RawTuning};` 把它们留在原来的路径上
//! （`crate::download::config::DownloadTuning`），两个宿主与 `shared.rs` 都不用跟着改。
//!
//! 「改这三个超时键要付什么代价」**不在这里**：写在 `core::download::stall` 的模块头，全仓只有那
//! 一处。本文件只负责默认值、区间与回落。

use std::time::Duration;

use super::super::preference::{self, MirrorPreference, RegionPreference};
use super::super::proxy::ProxySetting;
use super::super::region::RegionProfile;
use super::super::stall;
use super::{
    gh_proxy_override, EngineConfig, GhProxyOverride, DEFAULT_CHUNK_STALL,
    DEFAULT_MAX_ATTEMPTS_PER_SOURCE, DEFAULT_MAX_CONCURRENCY, DEFAULT_RESPONSE_TIMEOUT,
    MAX_ATTEMPTS_PER_SOURCE, MAX_CHUNK_STALL_SECS, MAX_CONCURRENCY, MAX_RESPONSE_TIMEOUT_SECS,
    MAX_THROUGHPUT_FLOOR_KIB, MIN_CHUNK_STALL_SECS, MIN_RESPONSE_TIMEOUT_SECS,
    MIN_THROUGHPUT_FLOOR_KIB,
};

/// 设置侧灌进来的下载调优（M193；M292 加三个超时/阈值）。
///
/// 这是「`launcher.json` 的 `download.*` 键」到「引擎装配参数」之间**唯一**的翻译层：键名由宿主读
/// （core 不认识设置键名），默认值、合法区间、回落策略都在这里——否则「UI 说 8、引擎跑 64」会从
/// 缝里长回来。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadTuning {
    /// 全局并发许可数（已夹在 `1..=MAX_CONCURRENCY`）。
    pub max_concurrency: usize,
    /// 每个源的尝试次数上限（已夹在 `1..=MAX_ATTEMPTS_PER_SOURCE`）。
    pub max_attempts_per_source: u32,
    /// gh-proxy 覆盖。
    pub gh_proxy: GhProxyOverride,
    /// 单次请求等响应头的上界（已夹在
    /// `MIN_RESPONSE_TIMEOUT_SECS..=MAX_RESPONSE_TIMEOUT_SECS`）。
    pub response_timeout: Duration,
    /// 分块作业的无进展窗口（同上，`MIN_CHUNK_STALL_SECS..=MAX_CHUNK_STALL_SECS`）。
    pub chunk_stall_timeout: Duration,
    /// 分块作业的平均吞吐下限（**字节/秒**，已夹在 `MIN_THROUGHPUT_FLOOR_KIB..=MAX_THROUGHPUT_FLOOR_KIB`
    /// 换算后的区间）。
    pub min_avg_throughput: u64,
    /// 显式的「地区」覆盖（`download.region`；`Auto` = 用自动探测/缓存，即 M294 之前的行为）。
    pub region: RegionPreference,
    /// 显式的「镜像偏好」覆盖（`download.mirror_preference`；`Auto` = 跟着 `region` 走）。
    pub mirror_preference: MirrorPreference,
    /// 代理三态（`download.proxy_mode` + `download.proxy_url`）。出厂 [`ProxySetting::Direct`]：
    /// 与 schema 的 `none` 同值（三档语义与回落见 `core::download::proxy` 模块头）。
    pub proxy: ProxySetting,
}

impl Default for DownloadTuning {
    fn default() -> Self {
        Self {
            max_concurrency: DEFAULT_MAX_CONCURRENCY,
            max_attempts_per_source: DEFAULT_MAX_ATTEMPTS_PER_SOURCE,
            gh_proxy: GhProxyOverride::Builtin,
            response_timeout: DEFAULT_RESPONSE_TIMEOUT,
            chunk_stall_timeout: DEFAULT_CHUNK_STALL,
            min_avg_throughput: stall::MIN_AVG_THROUGHPUT,
            region: RegionPreference::Auto,
            mirror_preference: MirrorPreference::Auto,
            proxy: ProxySetting::Direct,
        }
    }
}

/// 宿主读出来的**裸设置值**（`None` = 键不存在 / 类型不对，两种都当「没填」）。
///
/// 为什么要有这个结构体（M292）：默认值、区间、回落都在 [`DownloadTuning::from_raw`] **一处**，
/// 宿主只负责「按 [`KEY_THROUGHPUT_FLOOR`](super::KEY_THROUGHPUT_FLOOR) 这些常量取 `i64`」。新增键时只改这个结构体与
/// `from_raw`——两个宿主各读一半、各回落一半，就是「界面说有、引擎没读」重新长出来的样子。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RawTuning<'a> {
    /// [`KEY_CONCURRENCY`](super::KEY_CONCURRENCY)。
    pub concurrency: Option<i64>,
    /// [`KEY_RETRY_LIMIT`](super::KEY_RETRY_LIMIT)。
    pub retry_limit: Option<i64>,
    /// [`KEY_GH_PROXY`](super::KEY_GH_PROXY)（三态：空串 ≠ 关）。
    pub gh_proxy: Option<&'a str>,
    /// [`KEY_THROUGHPUT_FLOOR`](super::KEY_THROUGHPUT_FLOOR)（KiB/s）。
    pub throughput_floor_kib: Option<i64>,
    /// [`KEY_CHUNK_STALL`](super::KEY_CHUNK_STALL)（秒）。
    pub stall_timeout_secs: Option<i64>,
    /// [`KEY_RESPONSE_TIMEOUT`](super::KEY_RESPONSE_TIMEOUT)（秒）。
    pub response_timeout_secs: Option<i64>,
    /// [`KEY_PROXY_MODE`](super::KEY_PROXY_MODE)（`none` / `system` / `manual`）。
    ///
    /// 代理两键**进这个结构体**（不像 `region` / `mirror_preference` 那样另开一个 `with_*` 入口）：
    /// 它们是「字符串原文 + 一个客户端开关」，与 `gh_proxy` 同形；而 `region` 那一对当初另开入口
    /// 是因为给本结构体加字段会波及当时不在 scope 的 CLI（M294 的权宜），不是为了风格。
    pub proxy_mode: Option<&'a str>,
    /// [`KEY_PROXY_URL`](super::KEY_PROXY_URL)（**只在 `manual` 档被读**，其它档忽略）。
    pub proxy_url: Option<&'a str>,
}

/// `min..=max` 之间的秒数 → `Duration`（负数 / `0` / 越界一律 `None`，由调用方回落出厂值）。
fn secs_in_range(raw: Option<i64>, min: u32, max: u32) -> Option<Duration> {
    let secs = u64::try_from(raw?).ok()?;
    (u64::from(min)..=u64::from(max))
        .contains(&secs)
        .then(|| Duration::from_secs(secs))
}

impl DownloadTuning {
    /// 设置原文 → 调优（**未填 / 越界 / 非法值一律回落出厂默认**，绝不报错阻断下载）。
    ///
    /// 口径与 schema 的 `(min, max)` 同源但独立实现（用户可能手改文件绕过 `store.set`）：键不存在
    /// → 出厂默认；落在区间内 → 原值；负数 / `0` / 超上限 → 出厂默认（**不夹到边界**：那是
    /// 「想 9999 给 64」，界面仍会说谎）。`retry_limit` 的 `0` 是唯一例外，见实现里的注释。
    ///
    /// 三个超时键的 `0` 也走「回落默认」而不是「夹到 1」：`0` 秒的窗口等于「立刻判失速」、
    /// `0` 字节/秒的吞吐下限是除零——两种都不是用户想要的东西，静默回落到出厂值是唯一安全的读法。
    ///
    /// 代理两键在这里翻译（[`ProxySetting::from_raw`]）：未知模式 ⇒ `none`、`manual` + 空地址 ⇒
    /// `system`、地址**不在这里校验**（解析失败由 `build_client` 报有界错误，见
    /// `core::download::proxy` 模块头）。
    #[must_use]
    pub fn from_raw(raw: RawTuning<'_>) -> Self {
        let default = Self::default();
        Self {
            max_concurrency: raw
                .concurrency
                .and_then(|value| usize::try_from(value).ok())
                .filter(|value| (1..=MAX_CONCURRENCY).contains(value))
                .unwrap_or(default.max_concurrency),
            // `0` 是**合法值**（P2-1 复审：旧配置里真的有 0，且旧 GUI 把它当档位）：语义是
            // 「只试一次 = 不重试」，所以 0 与 1 都落成 1；负数 / 超上限才回落出厂默认。
            max_attempts_per_source: raw
                .retry_limit
                .and_then(|value| u32::try_from(value).ok())
                .filter(|value| *value <= MAX_ATTEMPTS_PER_SOURCE)
                .map_or(default.max_attempts_per_source, |value| value.max(1)),
            gh_proxy: raw
                .gh_proxy
                .map_or(GhProxyOverride::Builtin, gh_proxy_override),
            response_timeout: secs_in_range(
                raw.response_timeout_secs,
                MIN_RESPONSE_TIMEOUT_SECS,
                MAX_RESPONSE_TIMEOUT_SECS,
            )
            .unwrap_or(default.response_timeout),
            chunk_stall_timeout: secs_in_range(
                raw.stall_timeout_secs,
                MIN_CHUNK_STALL_SECS,
                MAX_CHUNK_STALL_SECS,
            )
            .unwrap_or(default.chunk_stall_timeout),
            min_avg_throughput: raw
                .throughput_floor_kib
                .and_then(|value| u32::try_from(value).ok())
                .filter(|kib| (MIN_THROUGHPUT_FLOOR_KIB..=MAX_THROUGHPUT_FLOOR_KIB).contains(kib))
                .map_or(default.min_avg_throughput, |kib| u64::from(kib) * 1024),
            // M294 的两个「源选择」键**不走这里**，见 [`Self::with_source_selection`]：
            // `from_raw` 恒给 `Auto`，即「不发言」。
            region: RegionPreference::Auto,
            mirror_preference: MirrorPreference::Auto,
            proxy: ProxySetting::from_raw(raw.proxy_mode, raw.proxy_url),
        }
    }

    /// 只读三个旧键的历史入口（M193）。**新代码用 [`Self::from_raw`]**：这里三个新键恒取出厂值，
    /// 宿主若把它当主入口，「设置了不生效」会静默回来。留下它是为了 core 内部（`shared` 的用例）
    /// 与宿主以外的老调用点不必为了一个三参包装改签名。
    #[must_use]
    pub fn from_settings(
        concurrency: Option<i64>,
        attempts: Option<i64>,
        gh_proxy: Option<&str>,
    ) -> Self {
        Self::from_raw(RawTuning {
            concurrency,
            retry_limit: attempts,
            gh_proxy,
            ..RawTuning::default()
        })
    }

    /// 把调优盖到 `EngineConfig` 上（其余字段保持传入值）。
    #[must_use]
    pub fn apply_to(&self, mut cfg: EngineConfig) -> EngineConfig {
        cfg.max_concurrency = self.max_concurrency;
        cfg.max_attempts_per_source = self.max_attempts_per_source;
        cfg.response_timeout = self.response_timeout;
        cfg.chunk_stall_timeout = self.chunk_stall_timeout;
        cfg.min_avg_throughput = self.min_avg_throughput;
        // 代理必须在这里盖：`EngineConfig::default()` 是 `System`（「没读设置」的档），而
        // `DownloadTuning::default()` 是 `Direct`（settings 的 `none`）——不盖就是「设置页说 none、
        // 引擎偷偷跟随环境变量」。
        cfg.proxy = self.proxy.clone();
        cfg
    }

    /// 一行「引擎实际拿到了什么」（CLI 日志 / 诊断用）。
    ///
    /// 存在的理由就是 P21 的验收判据：用户改完设置之后，得在 `logs/launcher.log` 里**看见**生效值，
    /// 而不是只有「我以为我改了」。字段顺序与 `EngineConfig` 一致，单位写在名字里。
    ///
    /// M294 r1：两个「源选择」档位（`地区` / `镜像偏好`）也在这里——CLI 的
    /// `download::tuning` 已经读了它们，这一行若不含就等于「打了一行说生效值、却少说两项」。两个值都用
    /// [`RegionPreference::as_str`] / [`MirrorPreference::as_str`]，与 schema 的取值逐字相同。
    ///
    /// M295：`代理` 这一档用 [`ProxySetting::describe`]（不是 `as_str`）：`none` 与 `system` 行为
    /// 相反，只印一个词分不出「跟随环境变量」与「某个具体地址」。
    ///
    /// （`crates/cli/src/download.rs::tuning` 是唯一读者。）
    #[must_use]
    pub fn summary(&self) -> String {
        let gh_proxy = match &self.gh_proxy {
            GhProxyOverride::Builtin => "跟随内置默认".to_string(),
            GhProxyOverride::Off => "off".to_string(),
            GhProxyOverride::Prefix(prefix) => prefix.clone(),
        };
        format!(
            "并发={} 单源尝试={} 吞吐下限={}KiB/s 无进展窗口={}s 单次请求上界={}s 地区={} 镜像偏好={} 代理={} gh_proxy={gh_proxy}",
            self.max_concurrency,
            self.max_attempts_per_source,
            self.min_avg_throughput / 1024,
            self.chunk_stall_timeout.as_secs(),
            self.response_timeout.as_secs(),
            self.region.as_str(),
            self.mirror_preference.as_str(),
            self.proxy.describe(),
        )
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

    /// 叠上「源选择」两个显式键的**原文**（`download.region` / `download.mirror_preference`）。
    ///
    /// 为什么不进 [`RawTuning`]：CLI 的 `tuning()` 用**完整字段字面量**构造 `RawTuning`，给它加字段
    /// 会让 `crates/cli` 编译不过——而本轮不改那个宿主。于是这两个键走**同一个翻译层、另一个入口**：
    /// 默认 [`RegionPreference::Auto`] / [`MirrorPreference::Auto`]（= 与今天逐值相同），
    /// 谁读设置谁调这里。未知值/空串的回落规则见 `preference` 模块头。
    #[must_use]
    pub fn with_source_selection(
        mut self,
        region: Option<&str>,
        mirror_preference: Option<&str>,
    ) -> Self {
        self.region = region.map_or(RegionPreference::Auto, RegionPreference::from_raw);
        self.mirror_preference =
            mirror_preference.map_or(MirrorPreference::Auto, MirrorPreference::from_raw);
        self
    }

    /// 把「源选择」这一族设置盖到地区档案上（装配引擎时的唯一入口）。
    ///
    /// 顺序即优先级：先 `region` / `mirror_preference` 的显式覆盖（细则见 `preference` 模块头），
    /// 再叠 gh-proxy 三态——它只管**前缀**、不管顺序，所以放在最后。
    #[must_use]
    pub fn apply_source_selection(&self, profile: RegionProfile) -> RegionProfile {
        self.apply_gh_proxy(preference::apply(
            profile,
            self.region,
            self.mirror_preference,
        ))
    }
}
