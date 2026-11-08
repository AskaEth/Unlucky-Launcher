//! presenter ↔ core/agent 的 IPC 边界（01c §5 纪律）。
//!
//! 这里只允许出现「扁平 DTO / 事件 / 指令」：本 crate 的 DTO 里不得出现 `unlucky_core::*`
//! 或 `use unlucky_agent::*` 的内部类型。core/agent 侧的 host 层负责把自身类型翻译成本模块的类型，
//! 反向的 [`AppCommand`] 也一样。P0 阶段由 [`crate::mock`] 的假后端实现 [`Backend`] 驱动骨架，
//! P3 起由 [`crate::host`] 的真实后端驱动全部页面。
//!
//! 结构：**按领域拆子模块**（instances / versions / logs / agent / accounts / java /
//! settings / install / notices / content / events），全量 `pub use` 出去，调用方依旧写
//! `crate::dto::InstanceSummary` 这类路径。
//! 契约说明（字段口径、事件顺序、错误语义）见 `DevDocs/SubAgentsTalk/app-DTO通道约定.md`；
//! UI 侧的唯一入口与 iced 接线用法见 crate 文档（`src/lib.rs`）。
//!
//! 全部跨界类型 `Serialize + Deserialize`（AGENTS §6）：事件与快照是 IPC 契约，
//! 序列化能力由 derive 钉住，`dto` 的单测做 round-trip 断言。

mod accounts;
mod agent;
mod content;
mod events;
mod install;
mod instances;
mod java;
// M157: 实例启动参数（启动参数区的读取视图 + 写回补丁）
mod launch_args;
mod launch_failure;
mod logs;
mod notices;
// M113: import/export —— 整合包导入/导出的 DTO（M111/M112 管线的 IPC 面）
mod pack;
mod settings;
// M159: 自更新（检查结论 / 候选 / 就位状态）
mod update;
mod versions;

pub use accounts::{
    looks_like_jwt, redact, valid_offline_name, AccountKind, AccountNotice, AccountNoticeCode,
    AccountView, AuthlibLoginView, AuthlibRequest, DeviceLoginPhase, DeviceLoginView, LoginFailure,
    SkinImage, REDACTED,
};
pub use agent::{AgentEvent, ChatMessage, ChatRole, ConfirmRequest, Sensitivity, ToolCallView};
pub use content::{
    loader_facet, ContentDetail, ContentDetailRequest, ContentFailure, ContentFailureKind,
    ContentHit, ContentInstallRequest, ContentKind, ContentQuery, ContentResults, ContentSort,
    ContentSource, ContentState, ContentVersionView,
};
pub use events::{AppCommand, AppEvent, Backend, CoreEvent, LaunchFinished, LiveLogBatch};
pub use install::{derive_speed, human_bytes, InstallProgressView, InstallStage};
pub use instances::{
    CreateRequest, InstanceId, InstanceRuntime, InstanceSummary, LaunchFailureCause,
    LaunchFailureStage, LaunchFailureView, LaunchStage, LoaderInfo, LoaderKind, Progress,
};
pub use java::{JavaChainOutcome, JavaChainStep, JavaPageView, JavaRuntimeView, JavaVendorView};
pub use launch_args::{LaunchArgsPatch, LaunchArgsView};
pub use logs::{Diagnosis, DiagnosisReport, LogLevel, LogLine, LogOutcome, LogSession, SessionId};
pub use notices::{NoticeLevel, NoticeView};
// M113: import/export
pub use pack::{
    ConflictChoiceView, ConflictDecisionView, ConflictDecisionsView, ConflictFileView,
    ConflictReasonView, ConflictSourceView, ExportClassView, ExportDoneView, ExportFileView,
    ExportPreviewView, ExportReasonView, ExportRequestView, ExportState, FileActionView,
    ImportConflictView, ImportErrorView, ImportFailureStageView, ImportProgressView,
    ImportReportView, ImportRequestView, ImportScanView, ImportStageView, ImportState,
    ImportStatusView, ImportedFileView, MissingFileView, MissingReasonView, PackCommand, PackEvent,
    PackFormatView, PackState, ProbeView,
};
pub use settings::{SettingControl, SettingGroupView, SettingRowView};
// M159: 自更新
pub use update::{
    ReadyUpdate, UpdateBanner, UpdateCandidateView, UpdateFailureView, UpdateOutcome, UpdateView,
};
pub use versions::{AncientKind, VersionCategory, VersionKind, VersionSummary};
