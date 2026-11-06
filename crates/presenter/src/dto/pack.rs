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
//!    [`ImportErrorView`] 是「已整包回滚」（实例零残留）。UI 必须让这两类**视觉可区分**；
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
//! ## 冲突：默认一侧永远是用户那一侧
//!
//! [`ImportRequestView::overwrite`] 缺省 `false` = `ConflictPolicy::KeepUser`。这是 M113 的硬线：
//! **绝不默认覆盖用户改过的文件**。

mod conflict;
mod event;
mod export;
mod format;
mod import;

pub use conflict::{ConflictChoiceView, ConflictFileView, ConflictReasonView};
pub use event::{PackCommand, PackEvent, PackState};
pub use export::{
    ExportClassView, ExportDoneView, ExportFileView, ExportPreviewView, ExportReasonView,
    ExportRequestView, ExportState,
};
pub use format::PackFormatView;
pub use import::{
    FileActionView, ImportErrorView, ImportProgressView, ImportReportView, ImportRequestView,
    ImportStageView, ImportState, ImportStatusView, ImportedFileView, MissingFileView,
    MissingReasonView, ProbeView,
};
