//! 进度事件与它的**唯一出口**（M321 从 [`super`] 搬出来：那边贴着 400 行硬线，而本轮要在
//! 报告里加 processor 链的痕迹）。
//!
//! 纯搬移：类型与 `tell` 逐字未变，只是多了 [`tell_to`]（已经确认有观测者的那一侧用）。

use serde::Serialize;

use super::ImportStage;

/// 一条进度事件（同步回调，跨线程安全）。
#[derive(Debug, Clone, Serialize)]
pub struct ImportProgress {
    /// 当前阶段。
    pub stage: ImportStage,
    /// 阶段内已完成条目数。
    pub done: usize,
    /// 阶段内总条目数（未知时为 0）。
    pub total: usize,
    /// 当前条目的实例内路径（阶段刚开始时为 `None`）。
    pub current: Option<String>,
}

/// 进度回调。
pub type ImportProgressSink = std::sync::Arc<dyn Fn(&ImportProgress) + Send + Sync>;

/// 发一条进度事件（`sink` 为 `None` 时什么都不做）。
///
/// 放在这里是因为它是 [`ImportProgress`] 的唯一出口：编排、阶段实现、提交事务都走它，
/// 谁都不许自己拼一条进度事件。
pub(crate) fn tell(
    sink: &Option<ImportProgressSink>,
    stage: ImportStage,
    done: usize,
    total: usize,
    current: Option<String>,
) {
    if let Some(sink) = sink {
        tell_to(sink, stage, done, total, current);
    }
}

/// 同上，但调用方**已经确认**有观测者（闭包里反复调用的那一侧用这个，省掉每次包一层
/// `Option`）。两个入口写的是同一条 [`ImportProgress`]，谁都不许自己拼事件。
pub(crate) fn tell_to(
    sink: &ImportProgressSink,
    stage: ImportStage,
    done: usize,
    total: usize,
    current: Option<String>,
) {
    sink(&ImportProgress {
        stage,
        done,
        total,
        current,
    });
}
