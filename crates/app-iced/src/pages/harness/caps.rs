//! 取证窗口的**OS 侧密闭指标**（M100 r1 P2-2）：`WS_EX_TRANSPARENT` 在不在。
//!
//! 为什么单独一个文件：① `harness.rs` 正卡在 400 行硬线上；② 「三态映射」这件事本身值得有断言
//! （把 `unknown` 掉成 `off` 是这类日志最典型的假绿）。
//!
//! 判据来自 winit 0.30 的 Windows 实现：`set_cursor_hittest(false)` ⇒
//! `WS_EX_TRANSPARENT | WS_EX_LAYERED`。读位的手在
//! [`crate::platform::win32::transparency::click_through`]（全仓唯一 unsafe 落点）。

use crate::platform::win32::transparency;

/// 窗口是不是点击穿透 → 日志里的三态字面量。
///
/// - `on` = 读到了，且有 `WS_EX_TRANSPARENT`（winit 已把命中测试关掉）；
/// - `off` = 读到了，没有这一位；
/// - `unknown` = **没读到**（非 Windows / 句柄无效 / 还没拿到句柄）。
///
/// `unknown` 绝不许写成 `off`：「拿不到」和「没有」是两件事（U1 同族）。
#[must_use]
pub fn passthrough_label(hwnd: Option<isize>) -> &'static str {
    match hwnd.and_then(transparency::click_through) {
        Some(true) => "on",
        Some(false) => "off",
        None => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::passthrough_label;

    /// **抓得住**：谁把「没读到」掉成 `off`（那样一次失败的读数就变成了「密闭没开」的假证据，
    /// 或者反过来——在一台读不到扩展样式的机器上永远看不出密闭是否生效）。
    /// **抓不住**：真实窗口上的位到底置没置（那要出图日志里看 `on`）。
    #[test]
    fn a_missing_handle_is_unknown_not_off() {
        assert_eq!(
            passthrough_label(None),
            "unknown",
            "没句柄 = 没读到 ⇒ unknown"
        );
        assert_eq!(
            passthrough_label(Some(0)),
            "unknown",
            "句柄 0 = 拿不到 ⇒ unknown（不是 off）"
        );
        assert_eq!(
            passthrough_label(Some(1)),
            "unknown",
            "无效句柄 ⇒ unknown（不是 off）"
        );
    }
}
