//! 会话事件类型（从 `session.rs` 纯搬移，守单文件 400 行上限）。
//!
//! 对外路径不变：`unlucky_core::launch::session::EventKind`。

/// 事件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    /// 会话开始。
    SessionStart,
    /// Java 选择结果。
    JavaSelected,
    /// 命令行组装完成。
    CmdBuilt,
    /// natives 解压结果。
    NativesExtracted,
    /// 启动前失败（没有进程）。
    AssemblyFailed,
    /// 进程已启动。
    ProcStart,
    /// stdout 行。
    Stdout,
    /// stderr 行。
    Stderr,
    /// 检测到游戏窗口。
    WindowDetected,
    /// 进程退出。
    Exit,
    /// 发现崩溃报告 / hs_err。
    CrashFound,
    /// 诊断结论（M15 产出）。
    Diagnosis,
    /// 被主动结束。
    Killed,
    /// 会话结束。
    SessionEnd,
}

impl EventKind {
    /// JSONL 里的 `kind` 字符串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SessionStart => "session_start",
            Self::JavaSelected => "java_selected",
            Self::CmdBuilt => "cmd_built",
            Self::NativesExtracted => "natives_extracted",
            Self::AssemblyFailed => "assembly_failed",
            Self::ProcStart => "proc_start",
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
            Self::WindowDetected => "window_detected",
            Self::Exit => "exit",
            Self::CrashFound => "crash_found",
            Self::Diagnosis => "diagnosis",
            Self::Killed => "killed",
            Self::SessionEnd => "session_end",
        }
    }
}
