//! M116：窗口圆角的下发落点（`impl App::apply_window_corner`）。
//!
//! ## 为什么单独成文件
//!
//! 圆角与整窗 α 是**两条独立的管线**，只是都必须拿到 HWND、都在同一个时点下发：
//!
//! | 管线 | 判据 | 落点 | 键 |
//! |---|---|---|---|
//! | 整窗 α（D36） | `platform::effective_window_alpha` | `SetLayeredWindowAttributes` | `appearance.window_opacity` |
//! | 圆角（M116） | `platform::plan_corner` | `DwmSetWindowAttribute` | `appearance.window_corner` |
//!
//! 合在一个函数里会让「α 变了要不要重设圆角」这种问题变成实现细节；分开之后两个判据各自可测。
//!
//! ## 日志为什么是**一行、六栏**
//!
//! `corner=policy=… source=dwm|square os=… excluded_reason=… radius=system pref=… readback=… frame=… layered=…`
//!
//! 用户报的现象是「别的窗口都有圆角，只有我们的没有」。没有这一行，事后无法区分
//! 「没生效」「生效了但看不见」「这台系统根本没有这个能力」；`frame` / `layered` 两栏则是
//! 「圆角为什么不生效」的两个现场读数：`frame=`（窗口形状）与 `layered=`（整窗 α 引入的分层位）。
//!
//! ⚠️ **产品窗口不是 `WS_POPUP`**（M116 r4 复审订正）：winit 在 `decorations = false` 下给的是
//! `WS_CAPTION|WS_BORDER|WS_CLIPSIBLINGS|WS_SYSMENU`（可缩放时再加 `WS_THICKFRAME` 与 min/max 键），
//! `WS_POPUP` 只在**带 owner** 的弹出窗才加；无边框观感来自 `WM_NCCALCSIZE` 把客户区铺满。
//! 所以真进程日志里 `frame=` 应是 `overlapped`（关掉 min/max 则 `other`），**永远不会是 `popup`**。
//! 真正让系统忽略圆角请求的是**整窗 α 补的 `WS_EX_LAYERED`**（`layered=on`）。
//!
//! `radius=system` 是刻意的：系统不暴露 DWM 的圆角半径，我们也不自己定一个数字
//! （M116 取消了 8/12/16 固定档）。

use crate::app::diag::diag;
use crate::platform;
use crate::platform::win32::corner::{self, CornerApplied};
use crate::platform::win32::transparency::{apply_corner, style_summary, CornerOutcome};

use super::super::state::App;

impl App {
    /// 把当前圆角策略下发到窗口，并打一行可分辨的日志。
    ///
    /// ## 去重（M116 r1 复审 P2-4）
    ///
    /// 设置页的滑条（整窗 α / 面板 α / 暗化）每动一下都会走一次外观变更，而<b>圆角与那些键无关</b>。
    /// 判定完全没变时这里**直接返回**：不动窗口、不打日志。判据是
    /// [`corner::needs_redispatch`]（纯函数，有「同一策略 N 次只记 1 行」的测试），
    /// 指纹只含「策略 / 来源 / 原因码 / 有没有句柄」，不含只有真下发才拿得到的读数。
    ///
    /// 没有句柄时**也打日志**（`window=none`）：不能让「没生效」只体现为屏幕上没变化。
    /// 句柄从无到有会让指纹变化 ⇒ 首次拿到 HWND 时必然重下一次。
    pub(crate) fn apply_window_corner(&mut self, context: &str) {
        let policy = self.appearance.window_corner();
        let plan = platform::plan_corner(
            policy,
            platform::current_windows_version(),
            self.appearance.backend,
        );
        // 整窗 α < 1 ⇒ `window_alpha::dispatch` 会给窗口补 `WS_EX_LAYERED`；系统因此忽略圆角请求
        // （tower 裁定 A / D47）。这里只记**事实**：α 是否把我们自己的分层位打开了。
        let alpha_layered = self.appearance.window_opacity < 1.0;
        let suppressed_by = alpha_layered.then_some("window_alpha");
        let next = CornerApplied::new(policy, &plan, self.hwnd.is_some(), alpha_layered);
        if !corner::needs_redispatch(self.corner_applied.as_ref(), &next) {
            return;
        }
        self.corner_applied = Some(next);

        // 行的形状只有一处（`corner::format_line`），因为那里能在没有窗口的环境里被单测钉住。
        // 关键的三栏必须在同一行：`pref=`（请求发出去了）、`layered=`（分层位在不在，来自
        // 整窗 α）、`frame=`（窗口形状 = `overlapped`，不是 `popup`）——否则以后只能猜是哪一条。
        let (outcome, style) = match self.hwnd {
            Some(hwnd) => (apply_corner(hwnd, &plan).describe(), style_summary(hwnd)),
            None => (
                CornerOutcome::NoWindow.describe(),
                "frame=? layered=?".to_owned(),
            ),
        };
        diag!(
            "{}",
            corner::format_line(context, &plan, policy, &outcome, &style, suppressed_by)
        );
    }
}
