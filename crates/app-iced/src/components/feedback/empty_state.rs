//! 空态 EmptyState（`research/12b` §15）：图标块 + 标题 + 说明 + 可选动作。
//!
//! ## ⚠️ 这是**项目自定**组件，官方 Material 3 没有它
//!
//! `research/14`（官方组件清单）里**没有** EmptyState 这一项，Compose `material3` 也没有
//! 对应 token。所以**几何与排布是我们自定的**：下面表里标 `[估]` 的（96 块 / 三个间距 / 底色）
//! 没有任何官方出处，标 `[规格]` 的是 `11b R14` 的排版口径、标 `[源]` 的只有「CTA 就是标准按钮」
//! 这一条（按钮家族自己的 A1）。**不得整体对外声称「按官方规格实现」**：
//!
//! | # | 项 | 值 | 等级 |
//! |---|---|---|---|
//! | 15.1 | 图标块 | **96×96**，圆角 `Large(16)`，图标 **32** | `[估]`（圆角/图标档位是真令牌，96 是项目值） |
//! | 15.2 | 标题 | **HeadlineSmall 24/32**（空态是唯一允许 Headline 级的场景） | `[规格]` `11b R14` |
//! | 15.3 | 副文案 | **BodySmall 12/16** `OnSurfaceVariant`，最多 2 行 | `[规格]` |
//! | 15.4 | 垂直留白 | 内容在可用区居中（上下留白差 ≤8） | `[估]` |
//! | 15.5 | 快捷入口 | ≤ **3** 个 | `[估]`（本组件只给 1 个动作，见下） |
//! | 15.6 | CTA | 走标准 Button（40 高 / full 圆角），实心 `Primary` 占「每屏一个」名额 | `[源]`（按钮家族） |
//!
//! ## 为什么 CTA 直接用 `buttons::filled`
//!
//! 15.6 说 CTA 就是标准按钮。复用按钮家族有两个好处：空态的 CTA 与全站按钮同日升级；
//! 也顺带证明「组件库能跨家族组合」——本文件因此是唯一 import 兄弟家族的地方。

use iced::widget::{column, container, text};
use iced::{Alignment, Background, Border, Element, Length, Padding};

use unlucky_design::color::Role;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::icons::IconKind;
use unlucky_design::typography::TypeRole;

use crate::icons;
use crate::theme::Ui;

use super::super::buttons::{self, Button};
use super::super::layout;
use super::panel::EmptyStateGeometry;

/// 一个空态的装配描述（**不持有任何状态所有权**）。
#[derive(Debug, Clone)]
pub struct EmptyState<'a, M> {
    icon: IconKind,
    title: &'a str,
    body: &'a str,
    action: Option<(&'a str, M)>,
}

impl<'a, M> EmptyState<'a, M> {
    /// 图标 + 标题 + 说明，默认无动作。
    pub fn new(icon: IconKind, title: &'a str, body: &'a str) -> Self {
        Self {
            icon,
            title,
            body,
            action: None,
        }
    }

    /// 一个主操作（15.6：标准 Filled 按钮）。
    pub fn action(mut self, label: &'a str, on_press: M) -> Self {
        self.action = Some((label, on_press));
        self
    }
}

/// 画一个空态（内容整体在可用区里水平 + 垂直居中）。
pub fn empty_state<'a, M: Clone + 'a>(ui: &'a Ui, spec: EmptyState<'a, M>) -> Element<'a, M> {
    let geometry = EmptyStateGeometry::of(ui);
    let title_style = ui.style(TypeRole::HeadlineSmall);
    let body_style = ui.style(TypeRole::BodySmall);

    // Q9（两层）：①`view_at` 自己按 32 烘焙（stroke 也按 32 重算，视觉仍是 1.5px）；
    // ②外面再套 32 的固定盒，确保内容拿到的约束就是 32，不会被 96 块的 Fill 撑成 Contain。
    // 早先写的是 `view(..)`：它把 svg 钉成 24，32 的盒子只起居中作用，实渲 24（复审 C）。
    let glyph = layout::centered_fixed(
        geometry.icon,
        geometry.icon,
        icons::view_at::<M>(spec.icon, ui.color(Role::OnSurfaceVariant), geometry.icon),
    );
    let block_radius = iced::border::Radius::new(geometry.block_radius);
    // 块底色 `[估]`：`SurfaceContainerHigh`（材质层 `Overlay`）。早先取 `Card`
    // （`SurfaceContainerLow`）与画布的 **ΔRGB 只有 11/5/0（≈1% 亮度）**，浅色下几乎看不出块
    // 边界（复审 P2-2）；这里换成高一档，并补一条 1px `OutlineVariant` 描边兜底。
    let block_color = ui.layer(BLOCK_LAYER);
    let block_outline = ui.color(Role::OutlineVariant);
    let block = container(glyph)
        .width(Length::Fixed(geometry.block))
        .height(Length::Fixed(geometry.block))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(block_color)),
            border: Border {
                color: block_outline,
                width: crate::theme::STROKE_PX,
                radius: block_radius,
            },
            ..container::Style::default()
        });

    // **列间距走显式 gap，不走 `Column::spacing`**（复审 P2-1）：两者叠加时
    // block↔标题 会渲染成 8 + 16 + 8 = **32**、正文↔CTA 会渲染成 **40**，
    // 而登记的是 16 / 8 / 24（实测内容总高 264，按登记值应 232）。
    let mut content = column![]
        .align_x(Alignment::Center)
        .spacing(0.0)
        .push(block)
        .push(gap(geometry.gap_title))
        .push(
            text(spec.title)
                .size(title_style.size_px)
                .font(ui.font_of(title_style))
                .color(ui.color(Role::OnSurface)),
        )
        .push(gap(geometry.gap_body))
        .push(
            text(spec.body)
                .size(body_style.size_px)
                .font(ui.font_of(body_style))
                .color(ui.color(Role::OnSurfaceVariant)),
        );

    if let Some((label, message)) = spec.action {
        content = content
            .push(gap(geometry.gap_action))
            .push(buttons::filled(ui, Button::new(label, message)));
    }

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .padding(Padding::from(geometry.gap_action))
        .into()
}

/// 图标块的材质层（`[估]`：空态块是「抬起来的卡片」层级，不是画布也不是浮层）。
///
/// probe 与测试都从这里取，避免「组件一个色、断言另一个色」。
pub(super) const BLOCK_LAYER: MaterialLayer = MaterialLayer::Overlay;

/// 固定高度的透明留白（列间距的唯一来源：`Column::spacing` 必须保持 0，否则双算）。
fn gap<'a, M: 'a>(height: f32) -> Element<'a, M> {
    iced::widget::Space::new()
        .width(Length::Fill)
        .height(Length::Fixed(height))
        .into()
}
