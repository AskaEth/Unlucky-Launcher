//! 整窗 α 的**判据与执行**（D36）——从 `transparency.rs` 搬出来，那里要留行数给 DWM 的 FFI。
//!
//! ## 本文件零 `unsafe`、零 `extern`
//!
//! 全仓 `unsafe` 只允许出现在 [`super`]（`transparency.rs`）一个文件里，判据是「**带放行属性的
//! 文件集合恰好等于它**」。本文件要的 FFI 全部由 `super` 提供安全包装
//! （`raw_ex_style` / `raw_set_ex_style` / `has_layered` / `set_layered`），这里只做钳位、量化与三态映射。
//!
//! ⚠️ **别把 FFI 写回来**（`exstyle.rs` 的模块文档里有同一句，M100 r1 复审实测抓过一次）：
//! lint 级别沿模块树继承，在这里写 `unsafe` 块也**编得过**、门禁也照样绿，但「唯一落点」就破了。

use unlucky_presenter::prefs::WINDOW_ALPHA_MIN;

use super::{has_layered, raw_ex_style, raw_set_ex_style, set_layered};

/// Win32 的 `LWA_ALPHA` 标志位：只改整窗 α（`0x2`）。
///
/// 单独立成常量是为了让「写错成 `LWA_COLORKEY`（`0x1`）等于什么都没做」这件事有断言盯着。
pub const LWA_ALPHA: u32 = 0x0000_0002;

/// `WS_EX_LAYERED`：分层窗口扩展样式（`0x0008_0000`）。**整窗 α 的调用前提**。
pub const WS_EX_LAYERED: isize = 0x0008_0000;

/// `GWL_EXSTYLE`：扩展样式在 `Get/SetWindowLongPtrW` 里的槽位号（`-20`）。
pub const GWL_EXSTYLE: i32 = -20;

/// 一次整窗 α 应用的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlphaOutcome {
    /// α ≥ 1.0：完全不透明，**不调 FFI**（也就不需要分层窗口）。
    Opaque,
    /// 已下发，`byte` 是 Win32 收到的 `0..=255`。
    Applied {
        /// 量化后的字节值。
        byte: u8,
    },
    /// 拒绝执行。
    Rejected(AlphaRejection),
    /// 非 Windows 平台：空实现，什么都没做。
    Unsupported,
    /// FFI 自己失败（如句柄无效），`code` 来自 `GetLastError`。
    Failed {
        /// Win32 错误码（`1400` = `ERROR_INVALID_WINDOW_HANDLE`）。
        code: u32,
    },
}

/// [`AlphaOutcome::Rejected`] 的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlphaRejection {
    /// 没有可用的 α（`None` / 非正 / 非有限）——整窗穿透的拒绝在这里拦住。
    NoAlpha,
    /// 句柄为 0：还没拿到窗口，或窗口已销毁。
    NoWindow,
}

/// α → Win32 的 `bAlpha`（`0..=255`）。
///
/// 输入先夹到 [`WINDOW_ALPHA_MIN`]`..=1.0`（NaN 取下限）：调用方即使漏了
/// [`super::super::effective_window_alpha`] 的过滤，也不会把窗口写成穿透。
///
/// **向上取整**（`ceil`）是刻意的：`bAlpha/255` 才是实际下发的 α，用 `round` 会让
/// `0.08×255 = 20.4 → 20`，实际下发 `20/255 = 0.0784`，反而**低于** D36 的下限。
/// 因此下发值可能比请求值略高（最多 1/255 ≈ 0.004），但绝不会低于下限。
#[must_use]
pub fn alpha_byte(alpha: f64) -> u8 {
    let clamped = if alpha.is_nan() {
        WINDOW_ALPHA_MIN
    } else {
        alpha.clamp(WINDOW_ALPHA_MIN, 1.0)
    };
    (clamped * 255.0).ceil() as u8
}

/// 给窗口补上 `WS_EX_LAYERED`（幂等）。
///
/// ## 为什么这件事必须在这里（而不是「窗口创建侧」）
///
/// 加扩展样式只有 `GetWindowLongPtrW` / `SetWindowLongPtrW` 一条路，都是 `unsafe`；而
/// [`super::super`] 的 CI 门禁（`.github/workflows/guards.yml` 的 `unsafe-scope`）要求
/// 全仓显式放行 `unsafe_code` 的文件集合**恰好等于本文件**。所以窗口创建侧的
/// `app/boot.rs` 只负责「拿句柄 → 调本函数 → 调 [`apply_window_alpha`]」，
/// 不新增第二个放行文件。
///
/// ## 为什么创建侧无法预先带上这一位
///
/// iced 0.14 的 `window::Settings` 没有扩展样式入口；winit 0.30 只在
/// `IGNORE_CURSOR_EVENT`（鼠标穿透，`WS_EX_TRANSPARENT | WS_EX_LAYERED`）时置位，
/// `transparent: true` 走的是 DWM 那条路（`window.rs` 的 `ENABLE BLUR BEHIND WINDOW`），
/// 与 `LWA_ALPHA` 无关。所以只能拿到句柄后补。
///
/// ## 返回值
///
/// `true` = 这一位**已就位**（原本就有，或本次加上了）。判据是**再加读一次**验证，
/// 而不是信 `SetWindowLongPtrW` 的返回值：Win32 的它「失败返回 0 且成功也可能返回 0」
/// （前值就是 0 时），只能靠回读取证。句柄为 0 / 无效 → `false`（`GetWindowLongPtrW`
/// 也读不到这一位）。
pub fn ensure_layered(hwnd: isize) -> bool {
    if hwnd == 0 {
        return false;
    }
    if has_layered(hwnd) {
        return true;
    }
    raw_set_ex_style(hwnd, raw_ex_style(hwnd) | WS_EX_LAYERED);
    has_layered(hwnd)
}

/// 把整窗 α 下发到窗口。
///
/// `alpha` 一般直接来自 [`super::super::effective_window_alpha`]（`None` = 拒绝整窗穿透）。
/// 本函数是**执行层**：不再看渲染后端、不读环境变量，只认「有没有一个合法的值」+
/// 「有没有句柄」，然后（仅 Windows）调一次 `SetLayeredWindowAttributes`。
///
/// 调用前提是窗口已带 `WS_EX_LAYERED`（见 [`ensure_layered`]）：没有它 `SetLayeredWindowAttributes`
/// 一定失败（Win32 返回 0，本机实测 `GetLastError` 常见 87 / `ERROR_INVALID_PARAMETER`）。
pub fn apply_window_alpha(hwnd: isize, alpha: Option<f64>) -> AlphaOutcome {
    let Some(alpha) = alpha.filter(|value| value.is_finite() && *value > 0.0) else {
        return AlphaOutcome::Rejected(AlphaRejection::NoAlpha);
    };
    if alpha >= 1.0 {
        return AlphaOutcome::Opaque;
    }
    if hwnd == 0 {
        return AlphaOutcome::Rejected(AlphaRejection::NoWindow);
    }
    set_layered(hwnd, alpha_byte(alpha))
}
