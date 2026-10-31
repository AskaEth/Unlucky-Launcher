//! 日志页 DTO：会话列表、日志行与 D8 诊断结论。

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

/// D8 规则引擎结论（日志页崩溃诊断卡片）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnosis {
    /// 命中规则 id（`gpu.driver_old`）
    pub rule: String,
    /// 结论标题
    pub title: String,
    /// 0~100 置信度
    pub confidence: u8,
    /// 建议文案
    pub hint: String,
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
