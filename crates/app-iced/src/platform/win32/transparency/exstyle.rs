//! 取证用的**扩展样式探针**（M100 r1 P2-2）：这个窗口是不是「点击穿透」。
//!
//! ## 本文件零 `unsafe`、零 `extern`
//!
//! 全仓 `unsafe` 只允许出现在 [`super`]（`transparency.rs`）一个文件里，判据是
//! 「**带放行属性**的文件集合恰好等于它」。所以探针需要的 FFI 一律由 `transparency.rs` 提供
//! **安全包装**（[`super::window_exists`] / [`super::raw_ex_style`]），本文件只做纯判断、
//! 三态映射与测试。
//!
//! ⚠️ **别把 FFI 写回来**（M100 r1 复审实测抓过一次）：lint 级别沿模块树继承，所以在这里写
//! 一个 `unsafe` 块 + `extern` 声明也**编得过**，`unsafe-scope` 门禁也照样绿（它只数「带放行
//! 属性的文件」）——但「全仓唯一允许 unsafe 的文件」这个性质就破了。
//! 属性的作用是可审计性，不是给 CI 看的。
//!
//! ## 判据来源
//!
//! winit 0.30 在 Windows 把 `set_cursor_hittest(false)` 落成
//! `WS_EX_TRANSPARENT | WS_EX_LAYERED`（`window_state.rs`）。所以读回这一位，就是
//! 「取证窗口真的把命中测试关掉了」的 OS 侧证据——比「出图 hash 不变」有判别力得多
//! （实测：同 argv 开/关穿透，hash 完全相同）。

// 唯一使用者是下面的 `click_through`（它自己按平台门禁），非 Windows 上这个导入没人用
// ⇒ `unused_imports` 在 `-D warnings` 下升成错（M147）。
#[cfg(target_os = "windows")]
use super::raw_ex_style;

/// `WS_EX_TRANSPARENT`：**点击穿透**位（`0x0000_0020`）。
pub const WS_EX_TRANSPARENT: isize = 0x0000_0020;

/// 纯判据：扩展样式里 [`WS_EX_TRANSPARENT`] 在不在。
///
/// 单独拎成 `const fn` 是为了让「位判断写反」有断言盯着——`& WS_EX_TRANSPARENT != 0` 写成
/// `== 0` 是这类 getter 最经典的错，而且只会在「开/关恰好反向」时暴露（两边都"有输出"）。
#[must_use]
pub const fn is_click_through_style(ex_style: isize) -> bool {
    ex_style & WS_EX_TRANSPARENT != 0
}

/// 取证用：窗口**现在**是否点击穿透（`WS_EX_TRANSPARENT` 在扩展样式里）。
///
/// ## 为什么是三态而不是 `bool`
///
/// `None` = **没读到**（非 Windows / 句柄无效 / 句柄还没拿到）。这时调用方必须记 `unknown`，
/// **不许记成 `off`**——「拿不到」和「没有」是两件事，混为一谈就是假绿（本项目在别处栽过：
/// 扫描读不全时报「没有」）。所以这里宁可 `None`，也不用「读失败的 0」冒充「没这一位」。
///
/// ## 为什么本文件零 `unsafe`、零 `extern`
///
/// `unsafe` 全仓只在 [`super`]（`transparency.rs`）那一个文件里。探针需要的 FFI
/// （`IsWindow` / `GetWindowLongPtrW`）都由 `super` 提供安全包装
/// （[`super::window_exists`] / [`super::raw_ex_style`]），本文件只做**纯判断**。
///
/// ## 用途
///
/// 出图时把它打进日志（`pages::harness` 的 `[caps] passthrough_ext=…`），于是「取证窗口到底
/// 有没有把指针输入关掉」每次运行都能自证，不依赖外部探针。
#[cfg(target_os = "windows")]
#[must_use]
pub fn click_through(hwnd: isize) -> Option<bool> {
    if hwnd == 0 || !super::window_exists(hwnd) {
        return None;
    }
    Some(is_click_through_style(raw_ex_style(hwnd)))
}

/// 非 Windows：**恒 `None`**。空实现不许冒充「没开穿透」（那就是把「不知道」写成「没有」）。
#[cfg(not(target_os = "windows"))]
#[must_use]
pub fn click_through(_hwnd: isize) -> Option<bool> {
    None
}

#[cfg(test)]
mod tests {
    use super::{click_through, is_click_through_style, WS_EX_TRANSPARENT};
    use crate::platform::win32::transparency::WS_EX_LAYERED;

    /// **抓得住**：`& WS_EX_TRANSPARENT != 0` 被写成 `== 0`（位判断反向）——这是这类 getter 最经典
    /// 的错，而且只会在「开/关恰好互换」时暴露：日志里永远有值，只是意思反了。
    /// **抓不住**：真实窗口上 winit 到底置没置这一位（那要出图日志里看 `passthrough_ext=on`）。
    #[test]
    fn the_click_through_bit_cannot_be_read_backwards() {
        assert_eq!(WS_EX_TRANSPARENT, 0x0000_0020, "位值照 Win32 头文件");
        assert!(is_click_through_style(WS_EX_TRANSPARENT));
        assert!(
            is_click_through_style(WS_EX_TRANSPARENT | WS_EX_LAYERED),
            "winit 的鼠标穿透是两位一起置的，这种组合必须是 true"
        );
        assert!(
            !is_click_through_style(WS_EX_LAYERED),
            "只有分层位不算穿透——LWA_ALPHA 也加这一位，反向了会把正常窗口误判成穿透"
        );
        assert!(!is_click_through_style(0));
    }

    /// **抓得住**：「拿不到」被记成「没有」。句柄 0 / 无效句柄 / 非 Windows 必须是 `None`
    /// （调用方据此写 `unknown`），不许用「读失败的 0」冒充「这一位没开」。
    /// **抓不住**：一个**有效**窗口上的读数（那要真窗口，见 harness 的出图日志）。
    #[test]
    fn click_through_reports_unknown_instead_of_off_when_it_cannot_read() {
        assert_eq!(click_through(0), None, "句柄 0 = 还没拿到窗口 ⇒ unknown");
        assert_eq!(click_through(1), None, "无效句柄 ⇒ unknown（不许掉成 off）");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(
            click_through(0x1234),
            None,
            "非 Windows 的空实现必须恒 None，不许冒充 off"
        );
    }
}
