//! 整合包导入/导出 DTO（M113 `PACKUI`）：把 M111/M112 的管线结果搬过 IPC 边界。
//!
//! ## 为什么这里几乎是 core 报告的逐字段镜像
//!
//! `crate::dto` 的规矩（见 `dto/mod.rs`）：**不得出现 `unlucky_core::*` 类型**，跨边界类型必须
//! `Serialize + Deserialize`。所以这里一面不引 core，一面又不许「按 UI 方便重新发明一套语义」——
//! 三态、阶段、分类、原因全部**逐条对应** `core::addon::import::report` 与
//! `core::addon::export::report`，只把 `&'static str` 的稳定码换成人读串、把 `PathBuf` 落成
//! 页面能显示的形态。翻译（core 类型 → 本模块）发生在 `host::addon`，页面侧不认识 core。
//!
//! 三条不许在翻译时丢的东西：
//!
//! 1. **三态**（[`ImportStatusView`]）：`Partial` 不是 `Success`。把「缺了 3 个文件」说成成功，
//!    用户会在启动后才发现缺依赖；
//! 2. **容忍 / 致命的分野**：[`MissingFileView`] 是「搞不到某个文件」（实例可用），
//!    [`ImportErrorView`] 是致命失败。UI 必须让这两类**视觉可区分**——并且致命那一类还要按
//!    [`ImportReportView::failure_stage`] 再分三档说（预检没过 / 已回滚 / 回滚没做完，M161）；
//! 3. **导出的三分类**（[`ExportClassView`]）：`Inline` 是「证明不了可被远程替代，原字节进
//!    `overrides/`」，`Skipped` 是「压根没进包」。`skipped > 0` 意味着包**不完整**。
//!
//! ## 文件分工（守 400 行）
//!
//! | 文件 | 职责 |
//! |---|---|
//! | [`format`] | [`PackFormatView`]（三个格式 + slug 对齐） |
//! | [`import`] | 请求 / 进度 / 三态 / 逐项原因 / 探测诊断 |
//! | [`conflict`] | 冲突成因、处置、三方哈希 |
//! | [`export`] | 三分类 / 逐条原因 / 预检 |
//!
//! ## 冲突：预检清单 → 用户点头 → 事后账（M160）
//!
//! 三样东西在三处，别混：
//!
//! | 类型 | 时机 | 语义 |
//! |---|---|---|
//! | [`ImportScanView`] / [`ImportConflictView`] | 导入**前**（[`PackEvent::Scan`]） | 待用户点头的冲突清单 |
//! | [`ConflictDecisionsView`] | 页面 → 宿主（[`ImportRequestView::decisions`]） | 全部保留 / 全部覆盖 / 逐项 |
//! | [`ConflictFileView`] | 导入**后**（[`PackEvent::Report`]） | 每条实际怎么处置的 |
//!
//! 两处缺省都是**保留**（[`ConflictDecisionsView::default`] = `KeepAll`，core 的
//! `ConflictChoice::default()` = `KeepUser`）：**绝不默认覆盖用户已有文件**。

mod conflict;
mod event;
mod export;
mod format;
mod import;

pub use conflict::{
    ConflictChoiceView, ConflictDecisionView, ConflictDecisionsView, ConflictFileView,
    ConflictReasonView, ConflictSourceView, ImportConflictView, ImportScanView,
};
pub use event::{PackCommand, PackEvent, PackState};
pub use export::{
    ExportClassView, ExportDoneView, ExportFileView, ExportPreviewView, ExportReasonView,
    ExportRequestView, ExportState,
};
pub use format::PackFormatView;
pub use import::{
    FileActionView, ImportCancelView, ImportErrorView, ImportFailureStageView, ImportProgressView,
    ImportReportView, ImportRequestView, ImportStageView, ImportState, ImportStatusView,
    ImportedFileView, MissingFileView, MissingReasonView, ProbeView,
};
