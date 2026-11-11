//! 镜像清单与 URL 前缀重写。
//!
//! 内置清单 `mirrors.json` 随源码嵌入（`include_str!`），发行后可由设置层追加自定义规则。
//! 重写语义承 analyse/03 §5.2 的「前缀替换而非全量重写」——保留子路径，因此 `/libraries`、
//! `/maven`、`/fabric-meta` 这类子路径天然兼容；`inject` 用于补上路径形状不同的场景
//! （资源对象：官方 `<xx>/<hash>`，BMCLAPI `/assets/<xx>/<hash>`）。
//!
//! gh-proxy 规则（用户定案）：**只有国内模式 + GitHub host** 才套前缀，其它资源不受影响。

use serde::{Deserialize, Serialize};

pub use super::adoptium::AdoptiumMirror;
use super::jdk_sources::{MsOpenJdkTable, OracleMirror, ZuluTable};
use super::region::{RegionMode, RegionProfile};
use super::types::{Candidate, ResourceKind, SOURCE_OFFICIAL};

/// gh-proxy 候选的源标识。
pub const SOURCE_GH_PROXY: &str = "gh_proxy";

const fn default_true() -> bool {
    true
}

/// 一条前缀重写规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefixRule {
    /// 规则 id（日志/诊断用）。
    pub id: String,
    /// 匹配的 URL 前缀（含 scheme）。
    pub from: String,
    /// 替换成的前缀。
    pub to: String,
    /// 插在 `to` 与剩余路径之间的内容（默认空）。
    #[serde(default)]
    pub inject: String,
    /// 「仅加速不替代」：镜像优先模式下仍排在官方之后。
    #[serde(default)]
    pub fallback: bool,
    /// 只对这些资源类型生效（空 = 全部）。
    #[serde(default)]
    pub kinds: Vec<ResourceKind>,
    /// 是否启用（设置层可关）。
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl PrefixRule {
    /// 构造一条最简规则。
    #[must_use]
    pub fn new(id: impl Into<String>, from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            from: from.into(),
            to: to.into(),
            inject: String::new(),
            fallback: false,
            kinds: Vec::new(),
            enabled: true,
        }
    }

    /// 设一个「仅加速」标记。
    #[must_use]
    pub fn fallback(mut self) -> Self {
        self.fallback = true;
        self
    }

    /// 设置注入片段。
    #[must_use]
    pub fn with_inject(mut self, inject: impl Into<String>) -> Self {
        self.inject = inject.into();
        self
    }

    /// 是否命中该 URL（前缀匹配 + 资源类型过滤 + 启用位）。
    #[must_use]
    pub fn matches(&self, url: &str, kind: ResourceKind) -> bool {
        self.enabled
            && url.starts_with(&self.from)
            && (self.kinds.is_empty() || self.kinds.contains(&kind))
    }

    /// 生成镜像候选。
    #[must_use]
    pub fn apply(&self, url: &str) -> Option<Candidate> {
        if !url.starts_with(&self.from) {
            return None;
        }
        let rest = &url[self.from.len()..];
        let rewritten = format!("{}{}{}", self.to, self.inject, rest);
        Some(Candidate::new(source_id_for(&rewritten), rewritten))
    }
}

/// 一个 gh-proxy 类前缀。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GhProxy {
    /// 源标识。
    pub id: String,
    /// 展示名。
    pub name: String,
    /// 前缀（形如 `https://gh-proxy.com/`）。
    pub prefix: String,
    /// 是否启用。
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// 镜像表：内置规则 + 用户自定义规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorTable {
    /// 清单版本（便于诊断页显示「镜像表是哪一版」）。
    pub version: u32,
    /// 清单自带的说明（假阳性成因、不可推导项等，给运维看的原地注释）。
    #[serde(default)]
    pub notes: Vec<String>,
    /// 前缀重写规则（前面的优先）。
    #[serde(default)]
    pub rules: Vec<PrefixRule>,
    /// Adoptium 院校镜像。
    #[serde(default)]
    pub adoptium: Vec<AdoptiumMirror>,
    /// Oracle OpenJDK 镜像（v3 新增）。
    #[serde(default)]
    pub oracle_openjdk: Vec<OracleMirror>,
    /// Microsoft OpenJDK 分发模板（v3 新增）。
    #[serde(default)]
    pub microsoft_openjdk: Option<MsOpenJdkTable>,
    /// Zulu 元数据（v3 新增）。
    #[serde(default)]
    pub zulu: Option<ZuluTable>,
    /// gh-proxy 类前缀（**目前无生产读者**：生效前缀的唯一真相源是 `RegionProfile.gh_proxy`；
    /// 这一列只作为 `mirrors.json` 的解析留档，别再按它做「改这里能生效」的接口）。
    #[serde(default)]
    pub gh_proxies: Vec<GhProxy>,
}

impl Default for MirrorTable {
    fn default() -> Self {
        Self::builtin()
    }
}

impl MirrorTable {
    /// 内置清单（编译期嵌入）。
    #[must_use]
    pub fn builtin() -> Self {
        Self::parse(include_str!("mirrors.json")).unwrap_or_else(|err| {
            debug_note(&format!("内置 mirrors.json 解析失败（不应发生）：{err}"));
            Self {
                version: 0,
                notes: Vec::new(),
                rules: Vec::new(),
                adoptium: Vec::new(),
                oracle_openjdk: Vec::new(),
                microsoft_openjdk: None,
                zulu: None,
                gh_proxies: Vec::new(),
            }
        })
    }

    /// 从 JSON 解析。
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// 序列化成 JSON（诊断页/用户自定义文件回写用）。
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// 追加用户自定义规则（插到最前，优先于内置规则）。
    pub fn add_custom_rule(&mut self, rule: PrefixRule) {
        self.rules.insert(0, rule);
    }

    /// 找到第一条命中的前缀规则。
    #[must_use]
    pub fn prefix_rule_for(&self, url: &str, kind: ResourceKind) -> Option<&PrefixRule> {
        self.rules.iter().find(|rule| rule.matches(url, kind))
    }

    /// GitHub host 且允许时套 gh-proxy 前缀。
    ///
    /// **生效前缀的唯一真相源是 `region.gh_proxy`**（设置层/地区判定写入，`None` = 用户关闭）。
    /// 表格里的 `gh_proxies` 只是「已知可用前缀」的留档（无生产读者），**改它不会生效**——M193
    /// 删掉了原先那个 `MirrorTable::set_gh_proxy`，正是因为它改了这张表而 `expand()` 读的是档案，
    /// 属于「看起来能改、其实没人读」的入口。
    #[must_use]
    pub fn gh_proxy_candidates(&self, url: &str, region: &RegionProfile) -> Vec<Candidate> {
        if region.mode != RegionMode::MirrorFirst {
            return Vec::new();
        }
        let Some(prefix) = region
            .gh_proxy
            .as_deref()
            .map(str::trim)
            .filter(|prefix| !prefix.is_empty())
        else {
            return Vec::new();
        };
        let Some(host) = host_of(url) else {
            return Vec::new();
        };
        if !is_github_host(host) {
            return Vec::new();
        }
        vec![Candidate::new(SOURCE_GH_PROXY, format!("{prefix}{url}"))]
    }

    /// 展开候选序列：镜像优先模式下镜像在前、官方兜底；直连模式下官方在前、镜像兜底。
    #[must_use]
    pub fn expand(&self, url: &str, kind: ResourceKind, region: &RegionProfile) -> Vec<Candidate> {
        let official = Candidate::new(direct_source_id(url), url);
        let mut mirrors: Vec<Candidate> = Vec::new();
        let mut official_first = false;

        for candidate in self.adoptium_candidates(url) {
            push_unique(&mut mirrors, url, candidate);
        }
        if let Some(rule) = self.prefix_rule_for(url, kind) {
            if let Some(candidate) = rule.apply(url) {
                // 「仅加速不替代」的规则在镜像优先模式下也排官方之后。
                official_first = rule.fallback;
                push_unique(&mut mirrors, url, candidate);
            }
        }
        for candidate in self.gh_proxy_candidates(url, region) {
            push_unique(&mut mirrors, url, candidate);
        }

        if region.mode == RegionMode::MirrorFirst && !official_first {
            mirrors.push(official);
            mirrors
        } else {
            let mut out = Vec::with_capacity(mirrors.len() + 1);
            out.push(official);
            out.append(&mut mirrors);
            out
        }
    }
}

fn push_unique(list: &mut Vec<Candidate>, official_url: &str, candidate: Candidate) {
    if candidate.url == official_url || list.iter().any(|known| known.url == candidate.url) {
        return;
    }
    list.push(candidate);
}

/// 直连候选的源标识。
///
/// 绝大多数资源是 Mojang/CDN 的「官方源」，标 `official`；但 JDK 的四家发行版里，
/// Oracle（国内镜像）/ MS（VS CDN）/ Zulu（Azul CDN）的**直连 URL 本身就是某个厂商的 CDN**，
/// 标成 `official` 会让用户和健康度记忆都看错（「来源 official」其实来自华为云）。
/// 所以：认得的厂商/镜像 host 用它的稳定 id，其余一律 `official`。
#[must_use]
pub fn direct_source_id(url: &str) -> String {
    let id = source_id_for(url);
    if MIRROR_SOURCE_IDS.contains(&id.as_str()) {
        id
    } else {
        SOURCE_OFFICIAL.to_string()
    }
}

/// 会被 `direct_source_id` 认出来的厂商/镜像源 id（都是「非官方 CDN/镜像」那一类）。
pub const MIRROR_SOURCE_IDS: &[&str] = &[
    "bmclapi",
    "tuna",
    "ustc",
    "sjtu",
    "bfsu",
    "nju",
    "huawei",
    "azul",
    "microsoft",
    SOURCE_GH_PROXY,
];

/// 从 URL 里抠出 host（无 `url` 依赖）。
#[must_use]
pub fn host_of(url: &str) -> Option<&str> {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next()?;
    let host_port = authority.rsplit('@').next()?;
    let host = host_port.split(':').next()?;
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

/// 是否是 GitHub 系 host（gh-proxy 只对这些套前缀）。
#[must_use]
pub fn is_github_host(host: &str) -> bool {
    matches!(
        host,
        "github.com"
            | "raw.githubusercontent.com"
            | "objects.githubusercontent.com"
            | "codeload.github.com"
            | "gist.github.com"
            | "gist.githubusercontent.com"
    ) || host.ends_with(".githubusercontent.com")
}

/// URL 的源标识（健康度按此归并；不知名 host 直接用 host）。
#[must_use]
pub fn source_id_for(url: &str) -> String {
    let Some(host) = host_of(url) else {
        return SOURCE_OFFICIAL.to_string();
    };
    if host.ends_with("bangbang93.com") {
        return "bmclapi".to_string();
    }
    if host.contains("tuna.tsinghua.edu.cn") {
        return "tuna".to_string();
    }
    if host.contains("ustc.edu.cn") {
        return "ustc".to_string();
    }
    if host.contains("sjtu.edu.cn") {
        return "sjtu".to_string();
    }
    if host.contains("bfsu.edu.cn") {
        return "bfsu".to_string();
    }
    if host.contains("nju.edu.cn") {
        return "nju".to_string();
    }
    if host.contains("huaweicloud.com") {
        return "huawei".to_string();
    }
    if host.contains("azul.com") {
        return "azul".to_string();
    }
    if host.contains("visualstudio.microsoft.com") || host == "aka.ms" {
        return "microsoft".to_string();
    }
    if host.contains("gh-proxy") {
        return SOURCE_GH_PROXY.to_string();
    }
    host.to_string()
}

fn debug_note(message: &str) {
    if cfg!(debug_assertions) {
        eprintln!("[ull::download] {message}");
    } else {
        let _ = message;
    }
}
