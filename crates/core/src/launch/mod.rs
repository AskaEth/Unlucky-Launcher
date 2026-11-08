//! 启动管线：10 阶段命令行组装、Java 选择、子进程管理、结构化 JSONL 事件流。
//!
//! 设计定稿：`DevDocs/PlanDoc/06-launch-and-diagnostics.md` §1（管线）与 §3（事件流），
//! Java 侧约束见 `DevDocs/PlanDoc/01d-language-choice.md` §3（JDK 17 基准 + 版本映射提示）。
//!
//! 事件流是 core → app 的唯一启动反馈通道（另一条是快照发布），
//! 不做 200ms 批量刷 UI 式的隐式耦合。
//!
//! # 公共面
//!
//! - [`Launcher`]：`dry_run`（D18，只组装命令行）、`spawn`/`launch`（完整管线）；
//! - [`LaunchRequest`] 是命令行组装的**纯数据输入**，由 [`adapter`] 从 game 快照映射而来；
//! - [`AssembledCommand`] 是组装结果（program + argv + classpath + 环境变量 + 告警）；
//! - 事件流（[`session`]）与索引（[`session_index`]）是 M15 诊断采集器与
//!   M16 `read_game_log`/`list_game_sessions` 的数据源，schema 见
//!   `DevDocs/SubAgentsTalk/launch-接口约定.md`；
//! - [`DiagnosisHook`] 是接 M15 的唯一接缝（launch 报事实、M15 下结论）。
//!
//! # 与 HMCL 的三处差异（`analyse/02` §8.5）
//!
//! 1. `exit 0` 静默失败单独立标（[`exit_analysis`]）；
//! 2. 日志泵 join 有预算（[`watchdog`]），进程树三段式回收（[`tree_kill`]，P1 换 Job Object）；
//! 3. 窗口出现检测带启发式名称/耗时/证据（[`window_detect`]）。
//!
//! 零 UI 依赖：本模块只产出结构化事件与纯数据，不 import 任何 UI 类型。

pub mod adapter;
pub mod args_table;
pub mod argument;
pub mod command_builder;
pub mod crash_scan;
pub mod defaults;
pub mod diagnosis_hook;
pub mod error;
pub mod exit_analysis;
pub mod game_args;
pub mod game_version;
pub mod jvm_args;
pub mod launch_session;
pub mod launcher;
pub mod log_level;
pub mod natives;
pub mod overrides;
pub mod placeholders;
pub mod platform;
pub mod process;
pub mod pump;
pub mod redact;
pub mod request;
pub mod session;
pub mod session_index;
pub mod session_meta;
pub mod session_payload;
pub mod session_sink;
pub mod sink;
pub mod time;
pub mod tree_kill;
pub mod watchdog;
pub mod window_detect;

pub use adapter::{
    build_request, AdapterOptions, AdapterOutput, AdapterWarning, LibraryInput, SnapshotInput,
};
pub use args_table::{assemble, AssembledCommand, AssemblyWarning};
pub use crash_scan::{CrashArtifact, CrashSource};
pub use diagnosis_hook::{DiagnosisHook, DiagnosisRequest, NoopDiagnosis};
pub use error::{LaunchError, MissingComponent, Result};
pub use exit_analysis::{ExitAnalysis, ExitType, SilentFailure};
pub use launch_session::{LaunchSession, SessionHandle};
pub use launcher::{DryRun, Launcher, LauncherConfig};
pub use natives::{NativeJar, NativesEra, NativesPlan, NativesReport};
pub use process::{ProcessConfig, RunningProcess};
pub use request::LaunchRequest;
pub use session::{EndReason, SessionMeta, SessionSink, SessionWriter};
pub use session_index::SessionIndexEntry;
pub use sink::{CompositeSink, CountingSink, FnSink};
