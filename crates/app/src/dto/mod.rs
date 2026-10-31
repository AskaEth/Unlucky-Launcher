//! app ↔ core/agent 的 IPC 边界（01c §5 纪律）。
//!
//! 这里只允许出现「扁平 DTO / 事件 / 指令」：`crates/app` 任何模块都不得 `use unlucky_core::*`
//! 或 `use unlucky_agent::*` 的内部类型。core/agent 侧的 host 层负责把自身类型翻译成本模块的类型，
//! 反向的 [`AppCommand`] 也一样。P0 阶段由 [`crate::mock`] 的假后端实现 [`Backend`] 驱动骨架，
//! P3 起由 [`crate::host`] 的真实后端驱动全部页面。
//!
//! 结构：**按领域拆子模块**（instances / versions / logs / agent / accounts / java /
//! settings / install / notices / events），全量 `pub use` 出去，调用方依旧写
//! `crate::dto::InstanceSummary` 这类路径。
//! 契约说明（字段口径、事件顺序、错误语义）见 `DevDocs/SubAgentsTalk/app-DTO通道约定.md`。
//!
//! 全部跨界类型 `Serialize + Deserialize`（AGENTS §6）：事件与快照是 IPC 契约，
//! 序列化能力由 derive 钉住，`dto` 的单测做 round-trip 断言。

mod accounts;
mod agent;
mod events;
mod install;
mod instances;
mod java;
mod logs;
mod notices;
mod settings;
mod versions;

pub use accounts::{AccountKind, AccountView, DeviceLoginPhase, DeviceLoginView};
pub use agent::{AgentEvent, ChatMessage, ChatRole, ToolCallView};
pub use events::{AppCommand, AppEvent, Backend, CoreEvent, LaunchFinished, LiveLogBatch};
pub use install::{derive_speed, human_bytes, InstallProgressView, InstallStage};
pub use instances::{
    CreateRequest, InstanceId, InstanceRuntime, InstanceSummary, LaunchStage, LoaderInfo,
    LoaderKind, Progress,
};
pub use java::{JavaChainOutcome, JavaChainStep, JavaPageView, JavaRuntimeView, JavaVendorView};
pub use logs::{Diagnosis, DiagnosisReport, LogLevel, LogLine, LogOutcome, LogSession, SessionId};
pub use notices::{NoticeLevel, NoticeView};
pub use settings::{SettingControl, SettingGroupView, SettingRowView};
pub use versions::{AncientKind, VersionCategory, VersionKind, VersionSummary};
