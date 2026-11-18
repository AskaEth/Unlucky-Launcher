//! 版本 JSON 的 `arguments` 模型：规则（rules）求值与参数展开。
//!
//! 依据：`DevDocs/analyse/03-version-download.md` §4.4（`arguments` 的合并语义）、
//! §4.6（`minecraftArguments` 单串形式与新版 `arguments` 并存）。
//!
//! 规则求值语义照抄 Mojang/HMCL（`CompatibilityRule.appliesToCurrentEnvironment`）：
//! 规则列表为空 → 生效；否则**从 DISALLOW 开始**逐条覆盖，最后是 ALLOW 才算生效。
//! 这是"白名单式"语义，与"最后一条匹配者胜"不同，抄错会漏掉/多带启动参数。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::platform::os_info::OsInfo;

/// 规则动作。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    /// 允许（命中后该参数/库生效）。
    #[default]
    Allow,
    /// 禁止（命中后该参数/库失效）。
    Disallow,
}

/// 一条规则。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    /// 动作；缺省按 `allow` 处理（Mojang 老写法里 rules 只写 os 的情况）。
    #[serde(default)]
    pub action: RuleAction,
    /// 操作系统限制。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<OsRestriction>,
    /// 特性开关限制（`has_custom_resolution`、`is_demo_user`…）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<BTreeMap<String, bool>>,
}

/// 规则的 OS 限制。`version`/`arch` 在官方 JSON 里是**正则**，见 [`matches_simple_pattern`]。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OsRestriction {
    /// `windows` / `osx` / `linux`；也接受模组包里的 `win-x86` 这种合并写法。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// OS 版本模式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// 架构模式（`x86`、`x86_64`、`arm64`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
}

/// 求值时的运行环境（OS / 架构 / 特性开关）。
///
/// 现由 [`Environment::host`] 从 `core::platform::os_info` 的探测快照构造（同 crate 依赖），
/// 因此 `os_version` 是真实系统版本而不是空串；单测可以注入假快照做到不碰真机。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Environment {
    /// Mojang 口径的 OS 名：`windows` / `osx` / `linux`。
    pub os: String,
    /// OS 版本字符串（Windows 上是 `10.0.19045` 之类；探测失败时为空串）。
    pub os_version: String,
    /// Mojang 口径的架构：`x86` / `x86_64` / `arm64`。
    pub arch: String,
    /// 特性开关，缺省全部视为 `false`。
    pub features: BTreeMap<String, bool>,
}

impl Default for Environment {
    fn default() -> Self {
        Self::host()
    }
}

impl Environment {
    /// 当前主机环境。
    ///
    /// `os` / `arch` / `os_version` 全部取自 [`crate::platform::os_info::current`]
    /// （进程内缓存，探测只做一次）：`os_version` 不再是空串，因此带 `os.version` 限制的
    /// 规则（官方 JSON 里用正则写系统版本）能按真实值求值。
    pub fn host() -> Self {
        Self::from_os_info(crate::platform::os_info::current())
    }

    /// 从一份系统信息快照构造环境（单测注入假快照，不碰真机；也让 app/CLI 能复用已探测结果）。
    pub fn from_os_info(info: &OsInfo) -> Self {
        Self {
            os: info.family.manifest_name().to_owned(),
            os_version: info.os_version().to_owned(),
            arch: info.arch.clone(),
            features: BTreeMap::new(),
        }
    }

    /// 打开/关闭一个特性开关（构建器风格，便于 launch 侧串起来）。
    pub fn with_feature(mut self, name: impl Into<String>, value: bool) -> Self {
        self.features.insert(name.into(), value);
        self
    }

    /// 读一个特性开关；未声明视为 `false`。
    pub fn feature(&self, name: &str) -> bool {
        self.features.get(name).copied().unwrap_or(false)
    }
}

impl OsRestriction {
    /// 该限制是否匹配当前环境。
    pub fn allows(&self, env: &Environment) -> bool {
        if let Some(name) = &self.name {
            // 模组包常见 `{ "name": "win-x86" }`：OS 名与架构合写。
            let (os_name, arch) = match name.split_once('-') {
                Some((os_part, arch_part)) if is_known_os(os_part) && is_known_arch(arch_part) => {
                    (os_part, Some(arch_part))
                }
                _ => (name.as_str(), None),
            };
            if is_known_os(os_name) && os_name != env.os {
                return false;
            }
            if let Some(arch) = arch {
                if arch != env.arch {
                    return false;
                }
            }
        }
        if let Some(version) = &self.version {
            if !matches_simple_pattern(version, &env.os_version) {
                return false;
            }
        }
        if let Some(arch) = &self.arch {
            if !matches_simple_pattern(arch, &env.arch) {
                return false;
            }
        }
        true
    }
}

fn is_known_os(name: &str) -> bool {
    matches!(name, "windows" | "osx" | "linux")
}

fn is_known_arch(name: &str) -> bool {
    matches!(name, "x86" | "x86_64" | "arm64")
}

/// 极简正则匹配（官方 JSON 里 `os.version`/`os.arch` 用的是正则）。
///
/// 只覆盖实际出现过的形态：`^字面量`（前缀）、`字面量`（全等）、`字面量$`、
/// 以及里面的 `\.`/`.*` 展开。**遇到其它元字符一律放宽为匹配**——
/// 宁可多带一个参数（例如 Windows 专用 `-Dos.name`），也不要因为不会解析正则而漏掉参数。
/// 阶段 B 若真有 JSON 用到复杂模式，再引入 `regex` 依赖替换本函数。
pub fn matches_simple_pattern(pattern: &str, actual: &str) -> bool {
    let anchored_start = pattern.starts_with('^');
    let body = pattern.strip_prefix('^').unwrap_or(pattern);
    let anchored_end = body.ends_with('$');
    let body = body.strip_suffix('$').unwrap_or(body);

    let mut literal = String::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        match c {
            // `.*` 视作通配：丢掉两个字符即可（前后锚点仍然生效）。
            '.' if chars.as_str().starts_with('*') => {
                chars.next();
            }
            // `\.` 展开成字面点号。
            '\\' => match chars.next() {
                Some('.') => literal.push('.'),
                _ => return true,
            },
            '.' => literal.push('.'),
            '|' | '[' | ']' | '(' | ')' | '+' | '?' | '{' | '}' | '^' | '$' => return true,
            other => literal.push(other),
        }
    }

    match (anchored_start, anchored_end) {
        (true, true) => actual == literal,
        (true, false) => actual.starts_with(&literal),
        (false, true) => actual.ends_with(&literal),
        (false, false) => actual == literal || actual.starts_with(&literal),
    }
}

impl Rule {
    /// 该规则在当前环境下的动作；不匹配环境时返回 `None`。
    pub fn action_in(&self, env: &Environment) -> Option<RuleAction> {
        if let Some(os) = &self.os {
            if !os.allows(env) {
                return None;
            }
        }
        if let Some(features) = &self.features {
            for (name, expected) in features {
                if env.feature(name) != *expected {
                    return None;
                }
            }
        }
        Some(self.action)
    }
}

/// 一组规则的求值（Mojang 白名单语义）。
pub fn rules_allow(rules: &[Rule], env: &Environment) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut action = RuleAction::Disallow;
    for rule in rules {
        if let Some(this) = rule.action_in(env) {
            action = this;
        }
    }
    action == RuleAction::Allow
}

/// `arguments` 里的一项：纯字符串参数，或带规则的条件参数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    /// 无条件参数。
    Plain(String),
    /// 带规则的条件参数。
    Conditional(ConditionalArgument),
}

/// 带规则的条件参数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConditionalArgument {
    /// 规则列表（白名单语义，见 [`rules_allow`]）。
    #[serde(default)]
    pub rules: Vec<Rule>,
    /// 命中后的参数值（单个或多个）。
    pub value: ArgumentValue,
}

/// 参数值：单个字符串或多个字符串。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgumentValue {
    /// 单个参数。
    One(String),
    /// 多个参数。
    Many(Vec<String>),
}

impl ArgumentValue {
    /// 逐个参数借用（`One` 给一个，`Many` 给若干个）。
    pub fn strings(&self) -> Vec<&str> {
        match self {
            Self::One(value) => vec![value.as_str()],
            Self::Many(values) => values.iter().map(String::as_str).collect(),
        }
    }
}

impl Argument {
    /// 在当前环境下展开为具体参数（占位符替换归 launch 负责）。
    pub fn expand_into(&self, env: &Environment, out: &mut Vec<String>) {
        match self {
            Self::Plain(value) => out.push(value.clone()),
            Self::Conditional(cond) => {
                if rules_allow(&cond.rules, env) {
                    match &cond.value {
                        ArgumentValue::One(value) => out.push(value.clone()),
                        ArgumentValue::Many(values) => out.extend(values.iter().cloned()),
                    }
                }
            }
        }
    }
}

/// 版本 JSON 的 `arguments` 对象。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Arguments {
    /// 游戏参数（`mainClass` 之后）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub game: Vec<Argument>,
    /// JVM 参数（`-Xmx` 之类之外的那部分）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jvm: Vec<Argument>,
}

impl Arguments {
    /// 合并两层参数：**父在前、子在后**（analyse/03 §4.4）。
    pub fn merge(parent: Option<&Arguments>, child: Option<&Arguments>) -> Option<Arguments> {
        match (parent, child) {
            (None, None) => None,
            (Some(parent), None) => Some(parent.clone()),
            (None, Some(child)) => Some(child.clone()),
            (Some(parent), Some(child)) => Some(Arguments {
                game: parent
                    .game
                    .iter()
                    .chain(child.game.iter())
                    .cloned()
                    .collect(),
                jvm: parent.jvm.iter().chain(child.jvm.iter()).cloned().collect(),
            }),
        }
    }

    /// 按环境展开成扁平参数表（占位符 `--username ${auth_player_name}` 不在这里替换）。
    pub fn expand(&self, env: &Environment) -> ExpandedArguments {
        let mut game = Vec::new();
        let mut jvm = Vec::new();
        for argument in &self.game {
            argument.expand_into(env, &mut game);
        }
        for argument in &self.jvm {
            argument.expand_into(env, &mut jvm);
        }
        ExpandedArguments { game, jvm }
    }
}

/// [`Arguments::expand`] 的结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExpandedArguments {
    /// 展开后的游戏参数。
    pub game: Vec<String>,
    /// 展开后的 JVM 参数。
    pub jvm: Vec<String>,
}
