//! app ↔ core/agent 的 IPC 边界（01c §5 纪律）。
//!
//! 这里只允许出现「扁平 DTO / 事件 / 指令」：`crates/app` 任何模块都不得 `use unlucky_core::*`
//! 或 `use unlucky_agent::*` 的内部类型。core/agent 侧的 host 层负责把自身类型翻译成本模块的类型，
//! 反向的 [`AppCommand`] 也一样。P0 阶段由 [`crate::mock`] 的假后端实现 [`Backend`] 驱动骨架。
//!
//! 结构：**按领域拆子模块**（instances / versions / logs / agent / events），
//! 全量 `pub use` 出去，调用方依旧写 `crate::dto::InstanceSummary` 这类路径。
//! 契约说明（字段口径、事件顺序、错误语义）见 `DevDocs/SubAgentsTalk/app-DTO通道约定.md`。

mod agent;
mod events;
mod instances;
mod logs;
mod versions;

pub use agent::{AgentEvent, ChatMessage, ChatRole, ToolCallView};
pub use events::{AppCommand, AppEvent, Backend, CoreEvent};
pub use instances::{
    InstanceId, InstanceRuntime, InstanceSummary, LaunchStage, LoaderInfo, LoaderKind, Progress,
};
pub use logs::{Diagnosis, DiagnosisReport, LogLevel, LogLine, LogOutcome, LogSession, SessionId};
pub use versions::{AncientKind, VersionCategory, VersionKind, VersionSummary};
