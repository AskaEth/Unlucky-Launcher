//! 导入/导出的**指令、事件与面板状态**（M113）：`AppCommand::Pack` / `CoreEvent::Pack` 的载荷。
//!
//! ## 为什么整条流程只占一个枚举变体
//!
//! `AppCommand` / `CoreEvent` / `HostTask` 是这条装配链上最拥挤的三个枚举（M106/M107/M108/M109/M114
//! 都在往里加变体）。所以导入导出**不各占一个变体**，而是各自一个 [`PackCommand`] / [`PackEvent`]
//! 子枚举、外面只多一层包装：共享枚举里只多一行，rebase 时不用替别人定序。
//!
//! ## 取消是**界面级**的，不是管线级的（如实写）
//!
//! core 的 `import_pack` 没有取消令牌（`ImportRequest` 里没有 cancel 字段，`progress` 回调是
//! 纯通知、拿不到控制流）。所以「取消导入」在本轮的含义是：
//!
//! 1. 立刻把面板收回（[`PackEvent::Progress`] 传 `None`）；
//! 2. **丢弃**这一次运行之后回来的事件（代次比较，与 M105 的安装取消同一套做法）；
//! 3. 后台那次导入会继续跑到结束——**它不会因为点了取消而停手**。
//!
//! 这条必须在报告里如实登记：把它写成「真的停下来了」就是撒谎。
//!
//! ## 事件 → 状态只有一条路
//!
//! [`PackState::apply`] 是**唯一**的写入点（与 `AppState::apply` 同一条纪律）：页面读到的进度、
//! 报告、预检都只能从事件来，页面自己不做乐观回显。

use serde::{Deserialize, Serialize};

use crate::dto::pack::{
    ExportDoneView, ExportPreviewView, ExportRequestView, ExportState, ImportProgressView,
    ImportReportView, ImportRequestView, ImportState, ProbeView,
};

/// app → host 的导入/导出意图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PackCommand {
    /// 开始导入（目标实例已选好；`instance` 为 `None` = 按 `instance_name` 新建目录）
    Import(Box<ImportRequestView>),
    /// 跑**预检**（只扫描分类，**不写盘**）
    Export(Box<ExportRequestView>),
    /// 确认写盘。载荷是**预检时那一份请求**，不是页面当前字段——否则「预检 A、写 B」。
    Confirm(Box<ExportRequestView>),
    /// 关掉面板（导入结果 / 探测诊断 / 导出预检都收起来，回到入口态）
    Dismiss,
}

/// host → app 的导入/导出事件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PackEvent {
    /// 导入进度（`None` = 收回面板：取消，或流程已经给出结果）
    Progress(Option<Box<ImportProgressView>>),
    /// 导入结果（三态 + 逐项原因；致命失败也走它，`report.error` 里有原因）
    Report(Box<ImportReportView>),
    /// 探测失败：这个包看起来像什么、缺哪个标记文件
    Probe(Box<ProbeView>),
    /// 导出预检结果
    Preview(Box<ExportPreviewView>),
    /// 导出写盘完成
    Exported(Box<ExportDoneView>),
    /// 导出/预检在 host 侧失败（实例找不到、目录派生失败……一句人读原因）
    ExportFailed(String),
    /// 用户取消了导入：面板收回，但后台那次导入会跑完（core 没有取消令牌），
    /// 所以留一位给界面说清「还在跑、结果作废」。
    Cancelled,
    /// 关掉所有面板
    Cleared,
}

/// 导入/导出两个面板的状态（`AppState::pack` 的全部内容）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PackState {
    /// 导入侧（进度 / 结果 / 探测诊断）
    pub import: ImportState,
    /// 导出侧（预检 / 完成 / 失败）
    pub export: ExportState,
}

impl PackState {
    /// 事件 → 状态（**唯一**写入点）。
    pub fn apply(&mut self, event: PackEvent) {
        match event {
            PackEvent::Progress(progress) => {
                self.import.progress = progress.map(|progress| *progress);
            }
            PackEvent::Report(report) => {
                // 结果到了：进度面板让位，探测诊断作废（这一轮已经有结论了）
                self.import.progress = None;
                self.import.probe = None;
                self.import.report = Some(*report);
            }
            PackEvent::Probe(probe) => {
                self.import.progress = None;
                self.import.report = None;
                self.import.probe = Some(*probe);
            }
            PackEvent::Preview(preview) => {
                self.export.pending = false;
                self.export.failed = None;
                self.export.done = None;
                self.export.preview = Some(*preview);
            }
            PackEvent::Exported(done) => {
                self.export.pending = false;
                self.export.failed = None;
                // 写完之后预检就过期了：留着会让用户以为「还能再点一次确认」
                self.export.preview = None;
                self.export.done = Some(*done);
            }
            PackEvent::ExportFailed(reason) => {
                self.export.pending = false;
                self.export.failed = Some(reason);
            }
            PackEvent::Cancelled => {
                self.import.progress = None;
                self.import.cancelled = true;
            }
            PackEvent::Cleared => {
                self.import = ImportState::default();
                self.export = ExportState::default();
            }
        }
    }

    /// 记下「这次预检是对着哪份请求做的」（确认写盘时原样发回）。
    pub fn set_export_request(&mut self, request: ExportRequestView) {
        self.export.request = Some(request);
        self.export.pending = true;
        self.export.failed = None;
    }
}
