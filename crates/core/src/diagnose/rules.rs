//! 诊断规则模型：YAML schema + 编译后的规则。
//!
//! 设计定稿：`DevDocs/PlanDoc/06-launch-and-diagnostics.md` §2.2–§2.3；
//! 规则来源：HMCL `CrashReportAnalyzer.Rule` 59 条（`DevDocs/analyse/02-launch-pipeline.md` §7.3）。
//! 字段说明与给 M16/M18 的契约见同目录 `CONTRACT.md`。
//!
//! 相对 HMCL「枚举即签名库」的四处升级（analyse/02 §7.10）：
//! 1. 规则是**数据**不是代码：加规则只改 `rules.d/*.yaml`，不重编译；
//! 2. 每条规则带 `confidence`/`priority`/`suppress`，可做主因排序（HMCL 只能平铺）；
//! 3. 命名捕获组由正则自声明，不再有 `groupNames` 顺序契约；
//! 4. 每条规则绑定六维修复动作，直接成为 agent 工具的参数来源。

use std::collections::BTreeMap;
use std::fmt;

use regex::Regex;
use serde::Deserialize;

// 六维修复动作按体量拆两个文件（本目录文件上限 400 行），从本模块统一导出公开面。
#[path = "fix_action.rs"]
mod fix_action;
#[path = "fix_params.rs"]
mod fix_params;

pub use fix_action::{class_file_to_java_major, FixAction, FixActionKind, MaterializedFixAction};
pub use fix_params::{
    AdjustMemory, ChangeGraphics, ChangeJava, ComponentKind, GraphicsSetting, JavaVendor,
    ModAction, Reinstall, RemoveMod, SeekHelp,
};

/// 规则分类：HMCL 59 条规则的 10 个根因分组，与 `rules.d/<category>.yaml` 一一对应。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleCategory {
    /// JVM 实现 / Java 版本不兼容。
    Jvm,
    /// 堆内存不足、系统内存耗尽。
    Memory,
    /// 显卡驱动 / OpenGL / 分辨率 / 图形覆盖层。
    Graphics,
    /// 文件被改、安装不完整、原生库缺失。
    Integrity,
    /// Fabric 的 ModResolutionException 家族。
    Fabric,
    /// Forge 的依赖解析 / 重复安装 / mod 元数据。
    Forge,
    /// mod 加载期或运行期崩溃（含 mixin）。
    ModRuntime,
    /// 具体 mod 与 OptiFine 等成名冲突。
    ModConflict,
    /// 崩在某个实体 / 方块的 tick。
    World,
    /// 其它（手动触发调试崩溃等）。
    Other,
}

impl RuleCategory {
    /// 稳定字符串（也用于诊断输出的 `category` 字段）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Jvm => "jvm",
            Self::Memory => "memory",
            Self::Graphics => "graphics",
            Self::Integrity => "integrity",
            Self::Fabric => "fabric",
            Self::Forge => "forge",
            Self::ModRuntime => "mod_runtime",
            Self::ModConflict => "mod_conflict",
            Self::World => "world",
            Self::Other => "other",
        }
    }

    /// 全部分类（loader 校验规则文件归属、测试遍历用）。
    #[must_use]
    pub const fn all() -> [Self; 10] {
        [
            Self::Jvm,
            Self::Memory,
            Self::Graphics,
            Self::Integrity,
            Self::Fabric,
            Self::Forge,
            Self::ModRuntime,
            Self::ModConflict,
            Self::World,
            Self::Other,
        ]
    }
}

impl fmt::Display for RuleCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 结论严重度（UI 配色与「是否弹窗」的依据）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// 提示性：能跑但不最佳。
    Info,
    /// 警告：可疑但结论不确定。
    #[default]
    Warning,
    /// 严重：基本可以确定崩溃原因。
    Critical,
}

impl Severity {
    /// 稳定字符串（诊断输出用）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 匹配范围：决定引擎怎么切文本，也是性能开关（analyse/02 §8.3 第 1 条）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchScope {
    /// 逐行匹配（默认，绝大多数 Java 异常都是单行签名）。
    #[default]
    Line,
    /// 按空行切块匹配（一段异常 + 上下文）。
    Block,
    /// 整份文本匹配（仅跨行贪婪模式：mod 解析列表、实体 tick 上下文等）。
    File,
}

/// 结论文案（`summary` 支持 `{group}` 占位符，`key` 是稳定的 i18n/AI 引用 id）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cause {
    /// 稳定 id，如 `diagnose.cause.jvm.too_old_java`。
    pub key: String,
    /// 一句话结论（中文，可直接给用户看）。
    pub summary: String,
    /// 更细的解释 / 处理建议。
    #[serde(default)]
    pub detail: Option<String>,
}

impl Cause {
    /// 代入捕获组后的（结论, 细化说明）。
    #[must_use]
    pub fn render(&self, groups: &BTreeMap<String, String>) -> (String, Option<String>) {
        (
            substitute(&self.summary, groups),
            self.detail
                .as_ref()
                .map(|detail| substitute(detail, groups)),
        )
    }
}

/// 一条规则的 YAML schema。
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// 全局唯一 id（`命名空间.短名`）。
    pub id: String,
    /// 严重度。
    #[serde(default)]
    pub severity: Severity,
    /// 正则（Rust `regex` 语法，命名组用 `(?<name>…)`）。
    pub pattern: String,
    /// 匹配范围。
    #[serde(default)]
    pub scope: MatchScope,
    /// 命中可信度 0.0~1.0。
    #[serde(default = "default_confidence")]
    pub confidence: f32,
    /// 主因排序权重 0~100。
    #[serde(default = "default_priority")]
    pub priority: u8,
    /// 命中本规则时把列出的规则降级为次生结论。
    #[serde(default)]
    pub suppress: Vec<String>,
    /// 结论文案。
    pub cause: Cause,
    /// 六维修复动作。
    #[serde(default)]
    pub fix_action: Vec<FixAction>,
    /// 回归样本路径（相对 `rules.d/`）。
    #[serde(default)]
    pub sample: Option<String>,
    /// 出处（HMCL 枚举名 / issue 链接）。
    #[serde(default)]
    pub source: Option<String>,
}

fn default_confidence() -> f32 {
    0.5
}

fn default_priority() -> u8 {
    50
}

/// 编译后的规则（正则只编译一次）。
#[derive(Debug, Clone)]
pub struct CompiledRule {
    rule: Rule,
    category: RuleCategory,
    regex: Regex,
}

impl CompiledRule {
    /// 编译一条规则。
    pub fn compile(rule: Rule, category: RuleCategory) -> Result<Self, RuleError> {
        let regex = Regex::new(&rule.pattern).map_err(|err| RuleError::InvalidPattern {
            id: rule.id.clone(),
            message: err.to_string(),
        })?;
        Ok(Self {
            rule,
            category,
            regex,
        })
    }

    /// 原始规则。
    #[must_use]
    pub const fn rule(&self) -> &Rule {
        &self.rule
    }

    /// 规则 id。
    #[must_use]
    pub fn id(&self) -> &str {
        &self.rule.id
    }

    /// 所属分类。
    #[must_use]
    pub const fn category(&self) -> RuleCategory {
        self.category
    }

    /// 匹配范围。
    #[must_use]
    pub const fn scope(&self) -> MatchScope {
        self.rule.scope
    }

    /// 编译好的正则。
    #[must_use]
    pub const fn regex(&self) -> &Regex {
        &self.regex
    }

    /// 命名捕获组名（文档/测试用；顺序无关，取代 HMCL 的 `groupNames` 契约）。
    #[must_use]
    pub fn named_groups(&self) -> Vec<&str> {
        self.regex.capture_names().flatten().collect()
    }
}

/// 规则编译错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleError {
    /// 正则非法（规则 id + 正则库报错原文）。
    InvalidPattern {
        /// 出错的规则 id。
        id: String,
        /// 正则库报错。
        message: String,
    },
}

impl fmt::Display for RuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPattern { id, message } => {
                write!(f, "规则 {id} 的正则非法：{message}")
            }
        }
    }
}

impl std::error::Error for RuleError {}

/// 把 `{group}` 占位符替换成捕获组值；未知占位符原样保留（便于发现规则写错）。
#[must_use]
pub fn substitute(template: &str, groups: &BTreeMap<String, String>) -> String {
    if !template.contains('{') {
        return template.to_owned();
    }
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let Some(close) = rest[start..].find('}') else {
            break;
        };
        let end = start + close;
        let name = &rest[start + 1..end];
        out.push_str(&rest[..start]);
        match groups.get(name) {
            Some(value) => out.push_str(value),
            None => out.push_str(&rest[start..=end]),
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}
