//! 启动期取证：后端选项的换算 + 首帧耗时那一行日志。
//!
//! 从 `app/state.rs` 拆出（rebase 到 5adac23 之后 `state.rs` 顶到 401 行）。这两件事都属于
//! 「进程启动那一刻记了什么」，与 `App` 的状态机无关；`state.rs` 只留 `pub(super) use` 再导出，
//! 于是 `app/boot.rs`（`use super::state::STARTED`）与 `app/view.rs`（`log_first_frame`）的既有路径一字未变。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

use crate::cli::Options;
use unlucky_presenter::backend::AppOptions;

/// 进程内「run() 开始」的时刻（首帧耗时基线）。
pub(crate) static STARTED: OnceLock<Instant> = OnceLock::new();
/// 首帧日志只打一次。
static FIRST_FRAME_LOGGED: AtomicBool = AtomicBool::new(false);

/// 启动参数 → presenter 的后端选项。
pub(super) fn app_options(options: &Options) -> AppOptions {
    AppOptions {
        // `--data-dir`：M90 起**真的消费**它（此前是「接受并忽略」的翻牌兼容参数）。
        // 截图通道强制 mock，所以 CI 冒烟那条 `--data-dir <tmp>` 不会往那个目录写东西。
        data_root: options.data_dir.clone(),
        mock: options.screenshot.is_some(),
    }
}

/// 首帧耗时日志（只在第一次 `view()` 打一行）。
pub(crate) fn log_first_frame() {
    if FIRST_FRAME_LOGGED.swap(true, Ordering::Relaxed) {
        return;
    }
    if let Some(started) = STARTED.get() {
        eprintln!(
            "[boot] first frame in {:.0} ms（run() 开始 → 首次 view，含后端探测与字体扫描）",
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
}
