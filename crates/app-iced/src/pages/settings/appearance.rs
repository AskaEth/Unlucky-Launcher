//! 设置页「Appearance」分组：演示行 + **真控件的接线**（M93 / APPEARWIRE）。
//!
//! 这一组是设置页唯一接了真控件的：`body = Some(body)`，`body` 直接调
//! `crate::pages::appearance::section`。`settings/mod.rs` 只按 `group.body` 分发，**不认识
//! 「Appearance」这个名字**；分组在左列的下标仍由 `crate::pages::appearance::APPEARANCE_GROUP`
//! 声明，`settings/tests/mod.rs` 拿它和本文件比对（顺序被调换会红）。
//!
//! **这一组归 language+appearance 线**：真控件的实现全在 `pages/appearance/section.rs`（本文件只负责
//! 把它挂进注册表），所以改控件时不用碰设置页目录。
//!
//! M101 r2：本组的渲染器签名改成收 [`States`]（组状态 + 外观状态），于是「这一组要读哪些状态」
//! 由本文件决定——`settings/mod.rs` 不再认识 `appearance::State`。

use iced::Element;
use unlucky_design::icons::IconKind;

use crate::theme::Ui;

use super::{Group, Message, States};

/// Appearance 分组。`body` 指着 [`body`]（它再转给 `pages/appearance/section.rs` 的真控件）；
/// 没有外观运行期状态时（harness / `pages_demo`）返回 `None`，由 `form` 退回下面的演示行。
pub(super) const GROUP: Group = Group {
    name: "Appearance",
    icon: IconKind::Sidebar,
    rows: &[
        ("Density", "Comfortable"),
        ("Corner style", "Follow system"),
        ("Motion", "Full"),
        ("Text scale", "100%"),
    ],
    body: Some(body),
};

/// 本组的真控件渲染器：有外观状态就画真控件（M93），没有（harness）返回 `None`。
fn body<'a>(ui: &'a Ui, states: States<'a>) -> Option<Element<'a, Message>> {
    states
        .appearance
        .map(|state| crate::pages::appearance::section(ui, state))
}
