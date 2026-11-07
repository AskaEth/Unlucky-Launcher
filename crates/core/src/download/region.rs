//! 地区自动判定：时区 + 系统语言 + 网络探测 → 直连 / 镜像优先。
//!
//! 设计文档 05 §3.2（用户 Q19 定案）：
//!
//! * 信号①时区 ∈ 大中华区列表，或 信号②系统语言 zh-* → 「大中华区候选」；
//! * 信号③网络探测（google/baidu，各 3s 超时，结果缓存 24h，可关）；
//! * 判定：大中华区 && google 不可达 → **镜像优先**（并启用 gh-proxy）；
//!   google 可达（已有全局代理）→ **直连**（尊重用户代理）；非大中华区 → 直连；
//! * 探测被关闭时，中文环境默认镜像优先（用户拍板的默认值）。
//!
//! 判定结果可由用户在设置里钉死（`RegionTrigger::UserPinned`），此时不参与自动重判。

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::atomic::write_atomic;

/// 大中华区时区（IANA 名）。
pub const GREATER_CHINA_TIMEZONES: &[&str] = &[
    "Asia/Shanghai",
    "Asia/Urumqi",
    "Asia/Chongqing",
    "Asia/Harbin",
    "Asia/Hong_Kong",
    "Asia/Macau",
    "Asia/Taipei",
];

/// 中文语言前缀（大小写不敏感的前缀匹配）。
pub const GREATER_CHINA_LANGUAGE_PREFIXES: &[&str] = &["zh", "wuu", "yue", "lzh"];

/// 默认 gh-proxy 前缀（2026-10 实测可用；设置可改，`None` 表示不默认启用）。
pub const DEFAULT_GH_PROXY: &str = "https://gh-proxy.com/";

/// 探测超时（设计文档定死 3s）。
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// 自动判定的有效期（24h）。
pub const DECISION_TTL_MS: u64 = 24 * 60 * 60 * 1000;

/// google 可达性探测点（204 短响应）。
pub const GOOGLE_PROBE_URL: &str = "https://www.gstatic.com/generate_204";

/// 百度可达性探测点。
pub const BAIDU_PROBE_URL: &str = "https://www.baidu.com/robots.txt";

/// 源选择模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionMode {
    /// 直连：官方在前，镜像只做兜底。
    Direct,
    /// 镜像优先：镜像在前，官方兜底；GitHub 资源套 gh-proxy。
    MirrorFirst,
}

impl RegionMode {
    /// 稳定字符串 id。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::MirrorFirst => "mirror_first",
        }
    }
}

/// 判定来源：自动 or 用户钉死。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionTrigger {
    /// 自动判定（24h 后可重判）。
    Auto,
    /// 用户手动钉死（不参与自动重判）。
    UserPinned,
}

/// 地区档案：引擎唯一需要知道的「用哪套源顺序」。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionProfile {
    /// 直连 / 镜像优先。
    pub mode: RegionMode,
    /// 判定来源。
    pub trigger: RegionTrigger,
    /// 是否落在「大中华区候选」。
    pub greater_china: bool,
    /// gh-proxy 前缀（`None` = 不启用）。
    pub gh_proxy: Option<String>,
    /// 是否做过网络探测（设置里可关）。
    pub probes_enabled: bool,
    /// 判定时间（unix 毫秒）。
    pub decided_at_ms: u64,
}

impl Default for RegionProfile {
    fn default() -> Self {
        Self::direct(0)
    }
}

impl RegionProfile {
    /// 显式直连。
    #[must_use]
    pub fn direct(now_ms: u64) -> Self {
        Self {
            mode: RegionMode::Direct,
            trigger: RegionTrigger::Auto,
            greater_china: false,
            gh_proxy: None,
            probes_enabled: false,
            decided_at_ms: now_ms,
        }
    }

    /// 镜像优先（并启用默认 gh-proxy）。
    #[must_use]
    pub fn mirror_first(now_ms: u64) -> Self {
        Self {
            mode: RegionMode::MirrorFirst,
            trigger: RegionTrigger::Auto,
            greater_china: true,
            gh_proxy: Some(DEFAULT_GH_PROXY.to_string()),
            probes_enabled: false,
            decided_at_ms: now_ms,
        }
    }

    /// 用户钉死某个模式（设置页「直连 / 镜像优先」）。
    #[must_use]
    pub fn pinned(mode: RegionMode, now_ms: u64) -> Self {
        let mut profile = match mode {
            RegionMode::Direct => Self::direct(now_ms),
            RegionMode::MirrorFirst => Self::mirror_first(now_ms),
        };
        profile.trigger = RegionTrigger::UserPinned;
        profile
    }

    /// gh-proxy 是否真的会生效（镜像优先 + 配了前缀）。
    #[must_use]
    pub fn gh_proxy_enabled(&self) -> bool {
        self.mode == RegionMode::MirrorFirst
            && self
                .gh_proxy
                .as_deref()
                .is_some_and(|prefix| !prefix.is_empty())
    }

    /// 是否按「国内」口径选源（镜像优先，或判定落在大中华区）。
    ///
    /// 给 JDK 发行版默认优先级用（D23：国内 Temurin→Oracle→MS，国外 Temurin→MS→Oracle）：
    /// 用户把模式钉成直连但机器仍在国内时，仍然应该吃国内镜像更快的那个顺序。
    #[must_use]
    pub fn is_domestic(&self) -> bool {
        self.mode == RegionMode::MirrorFirst || self.greater_china
    }

    /// 覆盖 gh-proxy 前缀。
    #[must_use]
    pub fn with_gh_proxy(mut self, prefix: Option<&str>) -> Self {
        self.gh_proxy = prefix
            .map(str::trim)
            .filter(|prefix| !prefix.is_empty())
            .map(str::to_string);
        self
    }

    /// 标记是否允许网络探测。
    #[must_use]
    pub fn with_probes(mut self, enabled: bool) -> Self {
        self.probes_enabled = enabled;
        self
    }

    /// 自动判定是否已过期（用户钉死永不过期）。
    #[must_use]
    pub fn is_stale(&self, now_ms: u64) -> bool {
        self.trigger == RegionTrigger::Auto
            && now_ms.saturating_sub(self.decided_at_ms) > DECISION_TTL_MS
    }

    /// 设置页展示用的中文标签。
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self.mode {
            RegionMode::Direct => "直连",
            RegionMode::MirrorFirst => "镜像优先",
        }
    }
}

/// 判定输入信号。`None` 表示「未知/未探测」。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Signals {
    /// 时区名（IANA）。
    pub timezone: Option<String>,
    /// 系统语言列表（BCP-47 或 POSIX locale，取首个非空即可）。
    pub languages: Vec<String>,
    /// google 可达性（`None` = 未探测/探测被关）。
    pub google_reachable: Option<bool>,
    /// baidu 可达性（诊断信息，暂不参与判定）。
    pub baidu_reachable: Option<bool>,
}

impl Signals {
    /// 从环境变量读（尽力而为：Windows 上通常没有 `TZ`，此时由 platform 层补精确值）。
    #[must_use]
    pub fn from_env() -> Self {
        let timezone = std::env::var("TZ").ok().filter(|tz| !tz.trim().is_empty());
        let mut languages = Vec::new();
        for key in ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
            if let Ok(value) = std::env::var(key) {
                for part in value.split([':', ';']) {
                    let part = part.trim();
                    if !part.is_empty() && !languages.iter().any(|known: &String| known == part) {
                        languages.push(part.to_string());
                    }
                }
            }
        }
        Self {
            timezone,
            languages,
            google_reachable: None,
            baidu_reachable: None,
        }
    }

    /// 时区是否落在大中华区。
    #[must_use]
    pub fn timezone_in_greater_china(&self) -> bool {
        self.timezone.as_deref().is_some_and(|tz| {
            GREATER_CHINA_TIMEZONES
                .iter()
                .any(|known| known.eq_ignore_ascii_case(tz.trim()))
        })
    }

    /// 系统语言是否是中文（含 `zh_CN.UTF-8` 这类 POSIX 形态）。
    #[must_use]
    pub fn language_in_greater_china(&self) -> bool {
        self.languages.iter().any(|language| {
            let lower = language.to_ascii_lowercase().replace('_', "-");
            GREATER_CHINA_LANGUAGE_PREFIXES
                .iter()
                .any(|prefix| lower.starts_with(prefix))
        })
    }

    /// 是否是大中华区候选（信号① 或 信号②）。
    #[must_use]
    pub fn is_greater_china(&self) -> bool {
        self.timezone_in_greater_china() || self.language_in_greater_china()
    }
}

/// 纯判定函数（不发请求 → 好测）。
#[must_use]
pub fn decide_from(signals: &Signals, now_ms: u64) -> RegionProfile {
    let greater_china = signals.is_greater_china();
    let mode = match (greater_china, signals.google_reachable) {
        // 非大中华区：一律直连。
        (false, _) => RegionMode::Direct,
        // 大中华区但 google 可达：用户已有全局代理，尊重直连。
        (true, Some(true)) => RegionMode::Direct,
        // 大中华区且 google 不可达：镜像优先。
        (true, Some(false)) => RegionMode::MirrorFirst,
        // 探测被关：中文环境默认镜像优先（用户拍板）。
        (true, None) => RegionMode::MirrorFirst,
    };
    let mut profile = match mode {
        RegionMode::Direct => RegionProfile::direct(now_ms),
        RegionMode::MirrorFirst => RegionProfile::mirror_first(now_ms),
    };
    profile.greater_china = greater_china;
    profile.probes_enabled = signals.google_reachable.is_some();
    if mode == RegionMode::Direct {
        profile.gh_proxy = None;
    }
    profile
}

/// 单点可达性探测（3s 超时，任何错误都算不可达）。
pub async fn probe_reachable(client: &reqwest::Client, url: &str) -> bool {
    let request = client.get(url).header(reqwest::header::RANGE, "bytes=0-0");
    match tokio::time::timeout(PROBE_TIMEOUT, request.send()).await {
        Ok(Ok(response)) => {
            let status = response.status();
            status.is_success() || status.is_redirection() || status.as_u16() == 204
        }
        _ => false,
    }
}

/// 并发探测 google 与 baidu（两个都 3s 上限，总耗时约 3s）。
pub async fn probe_signals(client: &reqwest::Client) -> (Option<bool>, Option<bool>) {
    let (google, baidu) = tokio::join!(
        probe_reachable(client, GOOGLE_PROBE_URL),
        probe_reachable(client, BAIDU_PROBE_URL)
    );
    (Some(google), Some(baidu))
}

/// 完整判定：读信号 →（可选）探测 → 决定。
pub async fn detect(client: &reqwest::Client, probes_enabled: bool, now_ms: u64) -> RegionProfile {
    let mut signals = Signals::from_env();
    if probes_enabled {
        let (google, baidu) = probe_signals(client).await;
        signals.google_reachable = google;
        signals.baidu_reachable = baidu;
    }
    decide_from(&signals, now_ms)
}

/// 判定结果的落盘形态。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedProfile {
    version: u32,
    profile: RegionProfile,
}

/// 读 24h 缓存（过期或损坏返回 `None`）。
#[must_use]
pub fn load_cached(path: &Path, now_ms: u64) -> Option<RegionProfile> {
    let bytes = std::fs::read(path).ok()?;
    let persisted: PersistedProfile = serde_json::from_slice(&bytes).ok()?;
    if persisted.profile.is_stale(now_ms) {
        return None;
    }
    Some(persisted.profile)
}

/// 写 24h 缓存（原子写）。
pub fn store_cached(path: &Path, profile: &RegionProfile) -> std::io::Result<()> {
    let persisted = PersistedProfile {
        version: 1,
        profile: profile.clone(),
    };
    let json = serde_json::to_vec_pretty(&persisted).map_err(std::io::Error::other)?;
    write_atomic(path, &json)
}
