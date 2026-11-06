//! 整窗 α 的**执行侧**：句柄探测 → `WS_EX_LAYERED` → `SetLayeredWindowAttributes` → 一行日志。
//!
//! ## 为什么句柄要绕一圈 iced
//!
//! iced 0.14 只给 `window::Id`（它自己的编号），没有「给我 HWND」的公开 API；唯一的入口是
//! `window::run(id, callback)`——回调里拿到的 `&dyn Window` 实现了 `HasWindowHandle`
//! （raw-window-handle 0.6），于是能取到 `RawWindowHandle::Win32(hwnd)`。
//!
//! ## 三层责任（顺序不能换）
//!
//! 1. **探测句柄**：[`probe_hwnd`]，纯取；拿不到就整条链跳过（并如实打日志，不许假装成功）；
//! 2. **补分层样式**：`ensure_layered`（唯一 `unsafe` 落点，`platform/win32/transparency.rs`）；
//!    `SetLayeredWindowAttributes` 在没有 `WS_EX_LAYERED` 的窗口上**必然失败**（Win32 返回 0，
//!    本机 `GetLastError` 常见 87），所以这一步是硬前提；
//! 3. **下发 α**：`apply_window_alpha`。`α ≥ 1` 时它返回 `Opaque` 并且**不调 FFI**，那时也就不必
//!    改扩展样式（1、2 步都是白改），[`dispatch`] 里按这个条件短路。
//!
//! ## 为什么日志要记「下发字节」而不是「看起来多透明」
//!
//! 本机是 RDP + 4 个虚拟显示适配器：`LWA_ALPHA` 的**观感**在这里不可信。所以证据链是
//! `配置值 → 判据 → 字节 → Win32 返回值`，全部经 [`super::App::log_appearance`] 落成一行文本。

use iced::window;

use crate::platform::win32::transparency::{apply_window_alpha, ensure_layered, AlphaOutcome};

/// 一次「补样式 + 下发」的结果（日志与单测共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Dispatch {
    /// `WS_EX_LAYERED` 是否就位（`α ≥ 1` 或 `α = None` 时为 `false`：那次根本没动扩展样式）。
    pub layered: bool,
    /// 下发的结局。
    pub alpha: AlphaOutcome,
}

impl Dispatch {
    /// 日志尾巴：`layered=ok outcome=Applied { byte: 205 }`。
    ///
    /// `layered` 有三个值，刻意区分「没用到」与「失败」：
    /// - `n/a`：α ≥ 1 或被拒绝 —— 那次**根本不需要**分层窗口，不是错误；
    /// - `ok`：这一位在（原本就有或本条补上）；
    /// - `FAILED`：需要补但补不上（无效句柄等）—— 只有这一种是坏消息。
    pub(super) fn describe(&self) -> String {
        let layered = match self.alpha {
            AlphaOutcome::Opaque | AlphaOutcome::Rejected(_) => "n/a",
            AlphaOutcome::Applied { .. } | AlphaOutcome::Failed { .. } if !self.layered => "FAILED",
            _ => "ok",
        };
        format!("layered={layered} outcome={:?}", self.alpha)
    }
}

/// 把 iced 的窗口句柄翻成 Win32 的 `HWND`（`isize`）。非 Windows 平台返回 `None`。
pub(super) fn probe_hwnd(handle: &dyn window::Window) -> Option<isize> {
    use window::raw_window_handle::RawWindowHandle;

    let handle = handle.window_handle().ok()?;
    match handle.as_raw() {
        #[cfg(target_os = "windows")]
        RawWindowHandle::Win32(win32) => Some(win32.hwnd.get()),
        #[cfg(not(target_os = "windows"))]
        _ => None,
        #[cfg(target_os = "windows")]
        _ => None,
    }
}

/// 补样式 + 下发（**幂等**：`α ≥ 1` 时什么都不做，返回 `Opaque`）。
pub(super) fn dispatch(hwnd: isize, alpha: Option<f64>) -> Dispatch {
    // α 不透明 / 被拒绝时不需要分层窗口：`SetLayeredWindowAttributes` 那次调用也不会发生，
    // 改扩展样式纯属副作用（还会让 DWM 换一条合成路径），所以短路掉。
    let needs_layered = alpha.is_some_and(|alpha| alpha < 1.0);
    let layered = needs_layered && ensure_layered(hwnd);
    Dispatch {
        layered,
        alpha: apply_window_alpha(hwnd, alpha),
    }
}

#[cfg(test)]
mod tests {
    use super::dispatch;
    use crate::platform::win32::transparency::{AlphaOutcome, AlphaRejection};

    /// 抓得住：①`α = None` / `α ≥ 1` / `α = 0` 时**不去碰扩展样式**（短路条件写反会在这里红）；
    /// ②拒绝路径的返回值与 `transparency` 的判据一致。
    /// 抓不住：真实窗口（有效句柄）上 `WS_EX_LAYERED` 是否真的加上了——那要靠本机日志的 `layered=ok`。
    #[test]
    fn opaque_or_refused_alpha_never_touches_the_window_style() {
        for none in [None, Some(1.0), Some(1.5), Some(0.0), Some(-1.0)] {
            let result = dispatch(1234, none);
            assert!(!result.layered, "{none:?} 不该改扩展样式");
        }
        assert_eq!(
            dispatch(1234, None).alpha,
            AlphaOutcome::Rejected(AlphaRejection::NoAlpha)
        );
        assert_eq!(dispatch(1234, Some(1.0)).alpha, AlphaOutcome::Opaque);
        // 无效句柄 + 合法 α：样式补不上（layered=false），FFI 也不会成功——两层都如实报告。
        let invalid = dispatch(1, Some(0.5));
        assert!(!invalid.layered, "无效句柄补不上 WS_EX_LAYERED");
        assert!(
            matches!(invalid.alpha, AlphaOutcome::Failed { .. }),
            "{:?}",
            invalid.alpha
        );
    }

    /// 本机真窗口的取证由 `App` 启动日志给（见 `app::appearance::log_appearance`），
    /// 这里只钉日志格式：`Applied { byte: 205 }` 必须能一眼读出字节，且「没用到样式」不等于失败。
    #[test]
    fn the_log_tail_carries_the_byte_and_distinguishes_skipped_from_failed() {
        let applied = super::Dispatch {
            layered: true,
            alpha: AlphaOutcome::Applied { byte: 205 },
        };
        assert!(applied.describe().starts_with("layered=ok outcome=Applied"));
        assert!(applied.describe().contains("205"));

        // α = 1.0：没补样式是**正常**的，不许写成 FAILED（否则报告里会出现假红）。
        let opaque = super::Dispatch {
            layered: false,
            alpha: AlphaOutcome::Opaque,
        };
        assert!(opaque.describe().starts_with("layered=n/a"));

        // 真的要补却没补上：这才是 FAILED。
        let failed = super::Dispatch {
            layered: false,
            alpha: AlphaOutcome::Failed { code: 87 },
        };
        assert!(failed.describe().starts_with("layered=FAILED"));
    }
}
