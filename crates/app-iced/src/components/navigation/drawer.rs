//! NavigationDrawer / NavigationRail（`research/15` §21 + 12b §5）：**同一个容器的两个宽度**（D33）。
//!
//! 本文件只装「容器」：宽度（88 ⇄ 240）、`CornerLargeEnd` 圆角、分组标题、组间分隔线。
//! 一项长什么样（`NavItem` / `NavGroup` / 颜色求值 / 药丸排布）在 `drawer/item.rs`。
//!
//! ## 参考值与 D33（详见 `geometry.rs` 的表）
//!
//! ```text
//! 折叠 = Rail   88  = LayoutMetrics::nav_rail_width    项高 52 = 药丸 32 + 4 + 标签 16
//! 展开 = Drawer 240 = LayoutMetrics::nav_drawer_width  项高 56 = A1 ActiveIndicatorHeight（G24）
//! 药丸：rail 88 − 2×16 = 56（M68 的 56×32）；drawer 240 − 2×12 = 216（= A1 的 360 − 2×12）
//! ```
//!
//! 两者共用 [`navigation`] 一个入口、一套颜色角色、一套 full 圆角药丸；切换时圆角语言不变，
//! 一屏导航容器数恒为 1（§21.9）。**不是两套组件**——D33 的可验收形式就是「同一函数两个宽度」。
//!
//! ## Q8 的现场
//!
//! M68 的药丸写 56 宽后用 `center_x(Length::Fill)` 居中，容器尺寸被 Fill 覆盖成 88 宽（图上横贯全轨）。
//! 这里药丸宽由 [`NavPaneGeometry::pill_width`] 给出，居中一律交给父级 `align_x`。
//!
//! ## 分组标题（只 drawer 侧，§21.8）
//!
//! `TitleSmall` + `OnSurfaceVariant`，**不可点**；组与组之间 1px `OutlineVariant`
//! （复用 [`crate::app::components::divider`]），不用背景色块分区。

use iced::widget::container;
use iced::{Background, Border, Element, Length, Padding};

use unlucky_design::color::Role;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::typography::TypeRole;

use crate::theme::Ui;

use super::super::divider;
use super::super::interaction::Base;
use super::geometry::{NavLayout, NavPaneGeometry};

mod item;
use item::{line, nav_row};
pub use item::{NavGroup, NavItem};

/// 导航容器的底：§21.3 标准（常驻）= `Surface` + L0 —— 所以项的禁用内容叠在画布色上。
///
/// 与 [`navigation`] 的 `ui.layer(MaterialLayer::Canvas)` 同源（`Canvas` 的 surface_role 就是
/// `Surface`），M234 之后「底」由这里给，项自己不猜。
pub const BASE: Base = Base::of(Role::Surface);

/// 导航容器（D33：`layout` 决定 88 还是 240，其余全同）。
#[derive(Debug, Clone)]
pub struct Navigation<'a, M> {
    layout: NavLayout,
    groups: Vec<NavGroup<'a, M>>,
}

impl<'a, M> Navigation<'a, M> {
    /// 折叠态：rail（88）。
    pub fn rail() -> Self {
        Self {
            layout: NavLayout::Rail,
            groups: Vec::new(),
        }
    }

    /// 展开态：标准常驻 drawer（240）。
    pub fn drawer() -> Self {
        Self {
            layout: NavLayout::Drawer,
            groups: Vec::new(),
        }
    }

    /// 追加分组（组间自动画 1px `OutlineVariant`）。
    pub fn group(mut self, group: NavGroup<'a, M>) -> Self {
        self.groups.push(group);
        self
    }
}

/// 导航容器（rail ⇄ drawer 的唯一入口）。
pub fn navigation<'a, M: Clone + 'a>(ui: &'a Ui, nav: Navigation<'a, M>) -> Element<'a, M> {
    let geometry = NavPaneGeometry::of(ui, nav.layout);
    let gap = match nav.layout {
        NavLayout::Rail => ui.metrics().nav_item_gap,
        // drawer 的项是紧贴的（56 已经含了内部留白，§21.4）
        NavLayout::Drawer => 0.0,
    };

    let mut body = iced::widget::Column::new().spacing(gap).width(Length::Fill);
    for (index, group) in nav.groups.into_iter().enumerate() {
        // 组间分隔线只属于 drawer：rail 88 宽里一条通栏线只会切碎药丸的节奏（§21.8 的分区是 drawer 语义）
        if index > 0 && matches!(nav.layout, NavLayout::Drawer) {
            body = body.push(container(divider::full::<M>(ui)).padding(Padding {
                top: ui.space(unlucky_design::spacing::Space::X2),
                bottom: ui.space(unlucky_design::spacing::Space::X2),
                left: 0.0,
                right: 0.0,
            }));
        }
        // 标题只在 drawer 侧出现（rail 88 宽放不下文字，§21.8 的分区是 drawer 语义）
        if let (NavLayout::Drawer, Some(title)) = (nav.layout, group.title) {
            body = body.push(
                container(line::<M>(
                    ui,
                    title,
                    TypeRole::TitleSmall,
                    ui.color(Role::OnSurfaceVariant),
                ))
                .width(Length::Fill)
                .padding(Padding {
                    top: ui.space(unlucky_design::spacing::Space::X2),
                    right: geometry.pad_start,
                    bottom: ui.space(unlucky_design::spacing::Space::X1),
                    left: geometry.pill_inset + geometry.pad_start,
                }),
            );
        }
        for item in group.items {
            body = body.push(nav_row(ui, BASE, item, geometry));
        }
    }

    container(body)
        .width(Length::Fixed(geometry.width))
        .height(Length::Fill)
        // 轨顶留白走 padding（不是 spacer 元素：会被 column 的 spacing 再加一档，M68 P3-5）
        .padding(Padding {
            top: ui.space(unlucky_design::spacing::Space::X6),
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        })
        .style(move |_theme| container::Style {
            // §21.3 标准（常驻）= Surface + L0；形状 CornerLargeEnd（内侧两角 16）
            background: Some(Background::Color(ui.layer(MaterialLayer::Canvas))),
            border: Border {
                radius: geometry.radius,
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

#[cfg(test)]
mod tests;
