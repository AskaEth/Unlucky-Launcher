//! gallery 里的选择类小节：把状态矩阵摆进 harness（**断言不在这里**）。
//!
//! 矩阵本体的构造在 [`selection::matrix`]（12 行 × 5 列：checkbox 四态 / radio / switch /
//! chip 三类），消息用 `()`（矩阵只供视觉验收），这里一次 `.map(..)` 接成 `DemoMessage`。
//!
//! ## 渲染层几何断言去哪了
//!
//! 在 [`crate::app::components::selection::probe`]（`selection/probe.rs`）。挪出去的原因是
//! 400 行红线（复审 P2-5 同一类问题）：断言 + 像素扫描会把这个文件顶到 458 行。
//!
//! 跑法（worktree 根目录）：
//!
//! ```text
//! ULL_GALLERY_SECTION=selection ./target/debug/unlucky-launcher-iced --software-renderer \
//!   --screen=gallery --screenshot target/m76/selection-light.png --screenshot-theme=light
//! ULL_GALLERY_SHOT=target/m76/selection-light.png cargo test -p unlucky-app-iced selection::probe
//! ```

use iced::widget::column;
use iced::Element;

use super::super::selection;
use crate::theme::Ui;

use super::DemoMessage;

/// 选择类家族的小节：标题 + 矩阵 + 图例。
pub(super) fn section<'a>(ui: &'a Ui) -> Element<'a, DemoMessage> {
    let pressed = |()| DemoMessage::Pressed("selection");
    column![
        selection::matrix::heading_and_note(ui).map(pressed),
        selection::matrix::matrix(ui).map(pressed),
        selection::matrix::legend(ui).map(pressed),
    ]
    .spacing(ui.space(unlucky_design::spacing::Space::X2))
    .into()
}
