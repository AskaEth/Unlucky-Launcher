//! tab 按钮的样式闭包（M299 r1：从 `tabs.rs` 拆出来）。
//!
//! ## 为什么拆文件
//!
//! 只有行数：`tabs.rs` 加了这个函数之后到 413 行（AGENTS.md §3-1 的单文件 400 行硬线）。拆法是
//! 「整段搬走 + 一条 `pub(super) use`」，行为零变化。
//!
//! ## 为什么返回**闭包**而不是普通函数
//!
//! 单测要拿到的必须是「交给 `button` 的那个闭包」，不是它里面的助手：M299 r1 复审把一个
//! `tab_fill(..)` 形状的助手留在闭包外先算、闭包写回 `|_theme, _status|`，那条只测助手的用例
//! **仍然全绿**——本 mission 在 tabs 上唯一的代码修复当时等于没有判据。照
//! `pages/settings/general/theme.rs` 的 `chip_style` / `appearance/section` 的 `choice_style`
//! 同一条写法，这里把闭包整个交出去，测试直接喂 `button::Status`。
//!
//! ## 修的是什么
//!
//! 此前 `tab_cell` 在闭包**外**把 `state` 算成 `forced.unwrap_or(Enabled)`、闭包又写成
//! `|_theme, _status|` ⇒ 真实指针的 hover / pressed **一个像素都不变**（只有 gallery 的
//! `Tab::state(..)` 强制态画得出来），全仓的 tab 一起中招。
//!
//! ## 解析顺序
//!
//! `forced`（取证 / 矩阵的 `.state(..)`）优先，否则 [`Interaction::from_status`]——与
//! `components/selection/chip.rs` 同一条。颜色经 [`interaction::layer_color`]（唯一入口，A8 起
//! 带过渡时间轴）：没有取样控件时**逐位**等于 `overlay_color`，取证图因此一字不变。

use iced::widget::button;
use iced::{Background, Border, Color, Shadow, Theme};

use unlucky_design::color::Role;
use unlucky_design::shape::ShapeScale;

use crate::app::components::interaction::{self, Base, Interaction};
use crate::theme::Ui;

/// tab 按钮的样式闭包（**M299**：状态层在闭包里跟 `status` 求值）。
pub(super) fn tab_style<'a>(
    ui: &'a Ui,
    base: Base,
    forced: Option<Interaction>,
    color: Color,
) -> impl Fn(&Theme, button::Status) -> button::Style + 'a {
    move |_theme, status| {
        let state = forced.unwrap_or_else(|| Interaction::from_status(status));
        button::Style {
            background: interaction::layer_color(ui, state, Role::OnSurface, base.role())
                .map(Background::Color),
            text_color: color,
            // ContainerShape = CornerNone（tab 本身是直角；圆角只在指示条上）
            border: Border {
                radius: ui.radius_dp(ShapeScale::None),
                ..Border::default()
            },
            shadow: Shadow::default(),
            snap: true,
        }
    }
}
