//! 诊断挂钩：launch 与 M15（`core::diagnose`）之间的唯一接缝。
//!
//! 分工（`DevDocs/SubAgentsTalk/launch-接口约定.md` §3）：**launch 报事实、M15 下结论**。
//! launch 在进程结束后把「退出判定 + 日志环形缓冲 + 崩溃现场文件 + 环境快照」打包成
//! [`DiagnosisRequest`] 交给挂钩；挂钩返回结构化 payload（`candidates[]` 等），由 launch
//! 脱敏后写进同一个 JSONL（单一真相源）。
//!
//! 这样 M15 不需要 import launch 的类型，launch 也不需要 import diagnose 的类型——
//! 阶段 B 的 diagnose 尚未合并，P0 先用 [`NoopDiagnosis`] 占位。

use std::path::PathBuf;

use serde_json::Value;

use crate::launch::crash_scan::CrashArtifact;
use crate::launch::exit_analysis::ExitAnalysis;
use crate::launch::pump::LogLine;

/// 诊断挂钩。
pub trait DiagnosisHook: Send + Sync {
    /// 需要诊断时调用；返回 `None` 表示“这次没有结论”（事件流里就不会有 `diagnosis` 事件）。
    fn analyze(&self, request: &DiagnosisRequest) -> Option<Value>;
}

/// 空挂钩：不诊断（占位 / 用户关闭 AI）。
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopDiagnosis;

impl DiagnosisHook for NoopDiagnosis {
    fn analyze(&self, _request: &DiagnosisRequest) -> Option<Value> {
        None
    }
}

/// 交给诊断挂钩的完整事实包。
#[derive(Debug, Clone)]
pub struct DiagnosisRequest {
    /// 会话 id（M15 可据此回读 jsonl）。
    pub session_id: String,
    /// 实例 id。
    pub instance_id: String,
    /// 实例名。
    pub instance_name: String,
    /// 游戏版本。
    pub game_version: String,
    /// 退出判定。
    pub exit: ExitAnalysis,
    /// 存活毫秒数。
    pub uptime_ms: u64,
    /// 环形缓冲里的日志行（按时间顺序，已脱敏）。
    pub log_lines: Vec<LogLine>,
    /// 崩溃现场文件（crash-report / hs_err / latest.log）。
    pub artifacts: Vec<CrashArtifact>,
    /// 脱敏后的完整 argv。
    pub argv: Vec<String>,
    /// 最大堆（MiB）。
    pub memory_mb: Option<u32>,
    /// Java 主版本。
    pub java_major: u32,
    /// Java 可执行文件。
    pub java_binary: PathBuf,
    /// 运行目录。
    pub game_dir: PathBuf,
}

impl DiagnosisRequest {
    /// 错误级日志行（M15 规则匹配的主输入）。
    pub fn error_lines(&self) -> Vec<String> {
        self.log_lines
            .iter()
            .filter(|line| line.is_error())
            .map(|line| line.text.clone())
            .collect()
    }

    /// 全部日志拼成文本（M15 自己的 `LogBuffer` 也能直接用 `log_lines`）。
    pub fn log_text(&self) -> String {
        let mut text = String::new();
        for line in &self.log_lines {
            text.push_str(&line.text);
            text.push('\n');
        }
        text
    }

    /// 一句话摘要（日志/UI 展示）。
    pub fn summary(&self) -> String {
        format!(
            "会话 {} 退出：[{}]；错误行 {}；现场文件 {}；需诊断={}",
            self.session_id,
            self.exit.summary(),
            self.error_lines().len(),
            self.artifacts.len(),
            self.exit.needs_diagnosis
        )
    }
}
