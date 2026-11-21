//! 清单参数模型：`arguments.jvm` / `arguments.game` 的 `{rules, value}` 求值。
//!
//! 平移自 `analyse/02` §4.7：`Argument` 是 `StringArgument | RuledArgument`，
//! `CompatibilityRule` 默认 `DISALLOW`、按顺序取最后一个命中的 action；
//! `os` 不匹配的规则整体“无效”（不是“不满足”），`features` 必须逐项相等。

use crate::launch::request::FeatureSet;

/// 规则动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleAction {
    /// 允许。
    Allow,
    /// 禁止。
    Disallow,
}

/// `rules.os` 子句。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OsRule {
    /// `windows` / `osx` / `linux`。
    pub name: Option<String>,
    /// `x86` / `x86_64` / `arm64`。
    pub arch: Option<String>,
    /// 系统版本模式（Mojang 用正则，这里只做前缀/子串匹配）。
    pub version: Option<String>,
}

/// 单条规则。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// 命中后的动作。
    pub action: RuleAction,
    /// 系统条件。
    pub os: Option<OsRule>,
    /// 特性条件（逐项相等才算命中）。
    pub features: Vec<(String, bool)>,
}

impl Rule {
    /// 允许规则。
    pub fn allow() -> Self {
        Self {
            action: RuleAction::Allow,
            os: None,
            features: Vec::new(),
        }
    }

    /// 禁止规则。
    pub fn disallow() -> Self {
        Self {
            action: RuleAction::Disallow,
            os: None,
            features: Vec::new(),
        }
    }

    /// 加系统条件。
    pub fn with_os_name(mut self, name: impl Into<String>) -> Self {
        let mut os = self.os.take().unwrap_or_default();
        os.name = Some(name.into());
        self.os = Some(os);
        self
    }

    /// 加特性条件。
    pub fn with_feature(mut self, name: impl Into<String>, expected: bool) -> Self {
        self.features.push((name.into(), expected));
        self
    }

    /// 求值：`Some(true)` 命中且允许，`Some(false)` 命中且禁止，`None` 不适用（规则无效）。
    pub fn evaluate(&self, ctx: &RuleContext<'_>) -> Option<bool> {
        if let Some(os) = &self.os {
            if let Some(name) = &os.name {
                if name != ctx.os_name {
                    return None;
                }
            }
            if let Some(arch) = &os.arch {
                if !arch_matches(arch, ctx.os_arch) {
                    return None;
                }
            }
            if let Some(pattern) = &os.version {
                if !version_matches(pattern, ctx.os_version) {
                    return None;
                }
            }
        }
        for (name, expected) in &self.features {
            if ctx.features.get(name) != Some(*expected) {
                return None;
            }
        }
        Some(self.action == RuleAction::Allow)
    }
}

/// 一个参数项：一组规则 + 一组取值（取值里可含 `${...}` 占位符）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArgumentEntry {
    /// 规则（空表示无条件适用）。
    pub rules: Vec<Rule>,
    /// 取值（每个元素是一个独立命令行 token）。
    pub values: Vec<String>,
}

impl ArgumentEntry {
    /// 无条件参数项。
    pub fn plain(values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            rules: Vec::new(),
            values: values.into_iter().map(Into::into).collect(),
        }
    }

    /// 带规则参数项。
    pub fn ruled(rules: Vec<Rule>, values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            rules,
            values: values.into_iter().map(Into::into).collect(),
        }
    }

    /// 是否在当前环境下适用。
    pub fn applies(&self, ctx: &RuleContext<'_>) -> bool {
        if self.rules.is_empty() {
            return true;
        }
        let mut allowed = false;
        for rule in &self.rules {
            if let Some(action) = rule.evaluate(ctx) {
                allowed = action;
            }
        }
        allowed
    }
}

/// rules 求值上下文。
#[derive(Debug, Clone, Copy)]
pub struct RuleContext<'a> {
    /// `rules.os.name` 取值。
    pub os_name: &'a str,
    /// `rules.os.arch` 取值。
    pub os_arch: &'a str,
    /// `rules.os.version` 取值。
    pub os_version: &'a str,
    /// 特性开关。
    pub features: &'a FeatureSet,
}

impl<'a> RuleContext<'a> {
    /// 构造。
    pub fn new(
        os_name: &'a str,
        os_arch: &'a str,
        os_version: &'a str,
        features: &'a FeatureSet,
    ) -> Self {
        Self {
            os_name,
            os_arch,
            os_version,
            features,
        }
    }
}

fn arch_matches(rule_arch: &str, actual: &str) -> bool {
    if rule_arch == actual {
        return true;
    }
    // Mojang 用 "x86" 表示 32 位，我们的实际值可能是 "i686" 之类。
    matches!(
        (rule_arch, actual),
        ("x86", "i686") | ("x86_64", "amd64") | ("arm64", "aarch64")
    )
}

/// 只做保守匹配：支持 `^10\.` / `^10\.15` 这类前缀模式与纯子串，无法识别的模式按“不匹配”处理
/// （宁可少加一个参数，也不要给错误平台加参数）。
///
/// 处理：去掉前置 `^` 与结尾 `$`、`\.` 反转义成 `.`、去掉结尾的点（`^10\.` 的语义就是
/// “以 `10.` 开头”，点本身不参与前缀比较）。含 `[ ( { | * + ?` 等真需要正则的模式一律不匹配。
fn version_matches(pattern: &str, actual: &str) -> bool {
    if actual.is_empty() {
        return false;
    }
    let trimmed = pattern.trim();
    let stripped = trimmed.strip_prefix('^').unwrap_or(trimmed);
    if stripped.contains(['[', '(', '{', '|', '*', '+', '?']) {
        return false;
    }
    let mut literal = stripped.replace("\\.", ".");
    while literal.ends_with(['$', '.']) {
        literal.pop();
    }
    if literal.is_empty() {
        return false;
    }
    actual.starts_with(&literal) || actual.contains(&literal)
}
