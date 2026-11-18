//! 整合包导入/导出区的**自有**消息（M113 `PACKUI`）。
//!
//! 与内容库页同一个范式（`pages/content/message.rs`）：这一支挂在
//! [`super::Message::Pack`] 下面，`pages::Message` 只留 `Content(..)` 一个投递口，
//! 于是「加一条导入/导出消息」不碰共享枚举的正文，也不碰 `app/update/page_messages.rs`
//! 那条**不许 `_ =>`** 的 match。
//!
//! **一条纪律**（与内容库页同一句）：这里的每个变体都是**意图**，不是状态变更。
//! 拖入文件 ≠ 已经导入（证据是 `ImportProgress` / `ImportReport` 事件回流）；
//! 点「导出」≠ 已经写了盘（证据是预检与 `ExportDone` 事件）。

use std::path::PathBuf;

use unlucky_presenter::dto::{InstanceId, PackFormatView};

/// 导入/导出区的消息。
#[derive(Debug, Clone)]
pub enum Message {
    /// 拖入了一个文件或目录（由 `app/update.rs` 的窗口事件转发）
    ///
    /// 只**落成草稿并立刻探测**，不直接开跑：用户要能看到「这个包看起来像 X」再决定。
    Dropped(PathBuf),
    /// 拖拽悬停状态变了（`true` = 有文件在窗口上方）
    Hover(bool),
    /// 路径输入框变了（**只改草稿**，不派指令）
    PathChanged(String),
    /// 提交导入（用当前草稿路径）
    Import,
    /// 取消正在跑的导入（阶段边界生效）
    Cancel,
    /// 关掉结果面板（报告 / 致命错误都走这一条）
    Dismiss,
    /// 以包为准重新导入（**只在用户显式点过之后**；载荷是整批覆盖档）
    Overwrite,
    /// 展开/收起某一条冲突的三方明细
    ToggleConflict(String),
    /// 冲突清单翻页（M160；页码会夹在 `[0, pages)` 里）
    PickPage(usize),
    /// 逐项切换某一条的处置（保留 ⇄ 覆盖）
    TogglePick(String),
    /// 整批档：全部保留（逐项里"覆盖"的那些一起清掉）
    PickKeepAll,
    /// 整批档：全部覆盖
    PickOverwriteAll,
    /// 按当前选择开始导入
    ConfirmImport,
    /// 取消这次导入（不写盘，回到入口态）
    CancelImport,
    /// 换目标实例（`None` = 按包名新建）
    SetInstance(Option<InstanceId>),
    /// 换新实例名（`None` 语义由 `SetInstance` 承担，这里只改名字）
    SetInstanceName(String),
    /// 换导出格式
    SetFormat(PackFormatView),
    /// 换导出包名
    SetExportName(String),
    /// 换导出包版本
    SetExportVersion(String),
    /// 换落盘路径（留空 = 让 host 给缺省）
    SetOutput(String),
    /// 跑预检（**不写盘**）
    Precheck,
    /// 确认写盘（预检已经有结果才可达）
    Confirm,
    /// 放弃这次导出（关掉预检面板，不写盘）
    AbortExport,
    /// 打开导出面板
    OpenExport,
}
