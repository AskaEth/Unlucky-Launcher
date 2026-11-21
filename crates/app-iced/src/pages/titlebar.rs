//! harness 的**标题栏留白带**：`pages/mod.rs` 的 [`crate::pages::app`] 那条出图/单测路径画的那条带。
//!
//! 为什么单独一个文件（M355 r2，复审 P2）：`pages/mod.rs` 贴到 400 行**零余量**，而这块与页面内容
//! 无关——它画的正是「产品路径不画它」的那条留白（产品走 `shell::view_with_account` 的真标题栏）。
//! **纯搬移**：函数体一字未改，`pages/mod.rs` 只剩一行 `use`。

use iced::widget::{container, space};
use iced::{Element, Length};

use unlucky_design::elevation::MaterialLayer;

use super::frame::filled;
use super::Message;
use crate::theme::Ui;

/// 标题栏留白带（高 = `LayoutMetrics::titlebar_height`）——**只给 [`crate::pages::app`] 的
/// harness 路径用**。
///
/// 产品路径的标题栏是 `shell::view_with_account` 画的那个真的（三个 caption 键 + 拖动热区），
/// 见 `pages` 模块文档的偏离 #7（M90 r2 已修）。
pub(super) fn titlebar_placeholder<'a>(ui: &'a Ui) -> Element<'a, Message> {
    container(space::horizontal())
        .width(Length::Fill)
        .height(ui.metrics().titlebar_height)
        .style(move |_theme| filled(ui.layer(MaterialLayer::Canvas)))
        .into()
}
