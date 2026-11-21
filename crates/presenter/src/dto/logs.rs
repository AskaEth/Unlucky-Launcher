//! 日志页 DTO：会话列表、日志行与 D8 诊断结论。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// 一次启动会话（日志页左栏单位）标识。
pub type SessionId = String;

/// 一次运行的结局（G3 左栏图标：✓ 正常 / ✗ 崩溃 / ⚠ 静默失败）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogOutcome {
    /// 正常退出
    Ok,
    /// 崩溃（非零退出码或 hs_err_pid）
    Crashed,
    /// exit0 但游戏没起来（D8 新增检测）
    SilentFail,
}

/// 会话摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogSession {
    /// 会话 id
    pub id: SessionId,
    /// 实例名（会话属于哪个实例）
    pub instance: String,
    /// 展示用时间（`10-29 19:22`）
    pub when: String,
    /// 结局
    pub outcome: LogOutcome,
    /// 时长（秒）
    pub duration_secs: u64,
}

/// 日志行等级（错误行高亮）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    /// debug
    Debug,
    /// info
    Info,
    /// warn
    Warn,
    /// error
    Error,
}

/// 一行日志。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogLine {
    /// 原始文本（含时间戳前缀）
    pub text: String,
    /// 等级
    pub level: LogLevel,
}

/// 一条结论的严重度（core `diagnose::rules::Severity` 的三值，**结构化出口**）。
///
/// 为什么是枚举不是字符串：严重度是有限的三值，页面要用它做分支（配色 / 排序 / 是否弹窗）；
/// 字符串会让「拼错一个 `crtical`」在运行时静默变成第四种值。序列化仍走核心的三个 slug
/// （`info` / `warning` / `critical`），所以会话 JSONL 与 core 的 `Candidate.severity` 同形。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosisSeverity {
    /// 提示性：能跑但不最佳。
    Info,
    /// 警告：可疑但结论不确定。
    Warning,
    /// 严重：基本可以确定崩溃原因。
    Critical,
}

impl DiagnosisSeverity {
    /// 稳定 slug（与 core 的 `Severity::as_str` 同值；会话 JSONL 的读写都靠它）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }

    /// slug → 枚举（认不出就是 `None`：老会话 / 未来新增档位都不会被猜成 `Warning`）。
    #[must_use]
    pub fn parse(slug: &str) -> Option<Self> {
        match slug {
            "info" => Some(Self::Info),
            "warning" => Some(Self::Warning),
            "critical" => Some(Self::Critical),
            _ => None,
        }
    }
}

/// D8 规则引擎结论（日志页崩溃诊断卡片）。
///
/// ## 这里**没有**标题与提示的正文（M392）
///
/// 曾经有 `title` / `hint` 两个 `String`，装的是 core 规则表 `rules.d/*.yaml` 里
/// `cause.summary` / `cause.detail` 的**中文原文**。界面直接画 ⇒ 英文界面里冒出整句中文
/// （GAPS §2.6 的 N3，i18n 到 100% 的最后一公里）。
///
/// 现在只给**键与参数**，文案由界面按语言从 `unlucky_i18n` 目录取：
///
/// | 字段 | 用处 |
/// |---|---|
/// | [`cause_key`](Self::cause_key) | `diagnose.cause.<rule id>`，界面按它查 `diagnose.title.*` / `diagnose.hint.*` |
/// | [`groups`](Self::groups) | 模板里的 `{sourcemod}` 这类占位符要代入的值 |
/// | [`severity`](Self::severity) | core 的真严重度（此前在半路丢了，页面只能拿置信度硬凑） |
///
/// `title` / `hint` 不是「换了个名字」，而是**故意删掉**：留着它们，下一个人就还会画中文。
///
/// ## [`system_text`](Self::system_text)：core 中文原话的**备份口**（M395 重新定位）
///
/// M392 把它当作「目录里没有条目的内置结论」的直出口：中文界面画它（零倒退），英文界面画
/// `CoreTextUntranslated`（诚实承认没译）。M395 `BUILTIN10` 之后，那 10 条（`signal.*` ×6 +
/// `hs_err.*` ×4）也进了目录——动态片段改走 [`groups`](Self::groups) 的具名参数——于是
/// **目录里 74 条结论都有中英文案**，`system_text` 不再是任何一条结论的渲染来源。
///
/// 它剩下两处用途，都只是「排障时能看见 core 到底说了什么」：
///
/// | 用途 | 在哪 |
/// |---|---|
/// | 会话 JSONL 里 core 的原话备份 | `presenter::host::logs::diagnosis`（读回 `Candidate.message`） |
/// | 模板参数**缺了**才兜底 | 界面 `pages/logs/diagnostic.rs::texts_of`：M395 之前写的会话没有 `groups`，那时中文界面画它，英文界面画 `CoreTextUntranslated` |
///
/// ⚠ **不许把它变回「懒得翻译」的出口**。它装的是中文，画到英文界面上就是本条要修的病。
/// 新增一处「界面直接画 `system_text`」= 新增一条永远不会被翻译的结论 ⇒ **先报**（塔批之前别合）。
/// 真要翻译一条结论，正路是：在 core 给它一个稳定的 `cause_key`，把动态片段放进
/// [`groups`](Self::groups)，再往 `unlucky_i18n` 的目录加 `diagnose.title.<rule id>` 的中英两份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnosis {
    /// 命中规则 id（`graphics.opengl_not_supported`；`signal.*` / `hs_err.*` 是内置结论）
    pub rule: String,
    /// 结论文案的目录键（`diagnose.cause.<rule id>`）——界面用它取标题与提示
    pub cause_key: String,
    /// 严重度（core 规则表的 `severity`；内置结论同字段）
    pub severity: DiagnosisSeverity,
    /// 0~100 置信度
    pub confidence: u8,
    /// 模板参数：目录条目里的 `{name}` 要代入的值（没有占位符时为空）。
    ///
    /// 两个来源共用这一个通道（**没有第三套参数机制**）：
    ///
    /// | 来源 | 例子 |
    /// |---|---|
    /// | 规则捕获组（M392，64 条规则里 16 条有） | `sourcemod` / `destmod` |
    /// | 引擎内置结论的具名参数（M395） | `exit_code` / `exit_type` / `uptime_ms` / `error_lines` / `module_name` / `failed_frame` / `physical_ram` / `physical_free` / `swap` / `swap_free` |
    ///
    /// 名字是 core 定的**稳定语义名**（不是 `arg1`），值是**语言中立**的片段（数字、
    /// `application_error` 这类 slug、`16.0 GB`、`?`）——中文子句不能进这里，否则英文模板会夹中文。
    /// `module_name` / `failed_frame` 为空串 = core 不知道，界面按语言补「未知模块 / 未知」。
    #[serde(default)]
    pub groups: BTreeMap<String, String>,
    /// core 拼好的结论原文（中文）。用途与红线见上面的「备份口」一节——**默认不画**。
    #[serde(default)]
    pub system_text: Option<String>,
    /// 可一键执行的修复动作（无则只给建议）
    pub fix: Option<String>,
}

/// 诊断结论 + 归属会话（UI 用它决定卡片显隐）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosisReport {
    /// 归属会话
    pub session: SessionId,
    /// 诊断内容
    pub diagnosis: Diagnosis,
}
