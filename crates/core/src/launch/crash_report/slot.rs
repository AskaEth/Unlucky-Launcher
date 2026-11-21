//! 进程级**单槽**：最近一次失败启动的现场（M298 从 `crash_report.rs` 拆出，纯搬移）。
//!
//! ## 为什么是一格而不是一条队列
//!
//! 失败弹窗的消费方是「收尾事件到达的那一拍」——一个 `LaunchFinished` 只解释**一场**失败。
//! 多存几份没有读者，反而会让「这份现场是哪一场的」变成要靠时间猜的问题。
//!
//! 归属的正确性**不靠这个槽**：报告自己带 `session`，界面侧的并发守卫拿它与收尾事件比
//! （见 `crates/app-iced/src/app/dialog.rs`；M298 把 session 归属做成单一来源）。

use std::sync::{Mutex, OnceLock};

use super::CrashReport;

fn slot() -> &'static Mutex<Option<CrashReport>> {
    static SLOT: OnceLock<Mutex<Option<CrashReport>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// 登记一次失败启动的现场（启动线程收尾时调用）。
pub fn publish(report: CrashReport) {
    if let Ok(mut guard) = slot().lock() {
        *guard = Some(report);
    }
}

/// 读最近一次失败启动的现场。
#[must_use]
pub fn failing_launch() -> Option<CrashReport> {
    slot().lock().ok().and_then(|guard| guard.clone())
}

/// 清空槽位（测试用）。
pub fn clear() {
    if let Ok(mut guard) = slot().lock() {
        *guard = None;
    }
}
