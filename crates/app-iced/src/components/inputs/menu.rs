//! 菜单（`research/12b` §8）：`Menu` 弹出层 + `DropdownMenuItem`。
//!
//! ## 参考值（`[源]` = A1，`MenuTokens` / `Menu.kt` 1.4.0）
//!
//! | # | 检查项 | 参考值 | 落点 |
//! |---|---|---|---|
//! | 8.1 | 容器圆角 | 官方 **4**（`ContainerShape = CornerExtraSmall`）；`12b` 说「本项目 12」——**取令牌表的 4**（报告偏离 D18） | `Component::Menu.spec().shape` |
//! | 8.2 | 容器色 / 高程 | `SurfaceContainer` + **L2** | `Role::SurfaceContainer` + `Elevation::Level2` |
//! | 8.3 | 项高 | **48** | `MENU_ITEM_HEIGHT`（G19） |
//! | 8.4 | 项内边距 | 水平 **12**、菜单上下留白 **8** | `Space::X3` / `Space::X2` |
//! | 8.5 | 字级 / 图标 | 文本 `BodyMedium 14/20`；图标 **24** | `TypeRole` + `IconSize::Toolbar` |
//! | 8.6 | 选中项 | 容器 `SecondaryContainer` + 文本 `OnSecondaryContainer`；指示器水平内缩 **4**、圆角 **8** | [`MenuItem::selected`] |
//! | 8.7 | 宽度 | 不窄于触发元素、长文本不换行（`ellipsis` 本轮不做，登记 D19） | [`Menu::width`] |
//!
//! ## 弹出层阴影
//!
//! 高程取 `Component::Menu.spec().elevation`（= L2），阴影参数取 `Elevation::shadow()`
//! （`ShadowSpec`：blur 6 / offset 2 / alpha 0.14，令牌层给的值），阴影色取 `Role::Shadow`。
//! **容器色不取 `MaterialLayer::Overlay`**：那是 `SurfaceContainerHigh`，比 A1 的 `SurfaceContainer`
//! 高一档；`MaterialLayer` 只用来确认「弹出层属于 Overlay 语义」，颜色以 A1 为准。
//!
//! 阴影不是所有用法都要：被放进**滚动视口**（会被裁切）的菜单要用 [`Menu::flat`]——
//! CPU 后端画阴影时绕过裁切掩码，阴影会漏到视口外压暗下面的内容（M238，理由与实证见
//! `menu/shadow.rs` 的 `flat` 文档）。
//!
//! ## 分组分隔线
//!
//! 复用 [`crate::app::components::divider::full`]（一像素线 + `OutlineVariant`），**不自己画线**。
//! 菜单水平内边距 12 由容器给，所以分隔线自然落在 12 内缩后的宽度上。

use iced::widget::text::LineHeight;
use iced::widget::{button, column, container, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding, Pixels, Shadow};

use unlucky_design::color::Role;
use unlucky_design::component_map::Component;
use unlucky_design::elevation::Elevation;
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::icons;
use crate::theme::Ui;

use super::super::divider;
use super::super::interaction::{self, Base, Interaction};
use super::super::layout;
use super::geometry::MenuGeometry;

mod shadow;

use shadow::panel_shadow;

/// 菜单里的一项。
#[derive(Debug, Clone)]
pub struct MenuItem<'a, M> {
    label: &'a str,
    leading: Option<IconKind>,
    trailing: Option<IconKind>,
    on_press: Option<M>,
    selected: bool,
    state: Option<Interaction>,
}

impl<'a, M> MenuItem<'a, M> {
    /// 一个可点的项。
    pub fn new(label: &'a str, on_press: M) -> Self {
        Self {
            label,
            leading: None,
            trailing: None,
            on_press: Some(on_press),
            selected: false,
            state: None,
        }
    }

    /// 一个禁用的项（无消息 = 点不动，与按钮家族同一口径）。
    pub fn disabled(label: &'a str) -> Self {
        Self {
            label,
            leading: None,
            trailing: None,
            on_press: None,
            selected: false,
            state: None,
        }
    }

    /// 前置槽位（24）。
    pub fn leading(mut self, kind: IconKind) -> Self {
        self.leading = Some(kind);
        self
    }

    /// 后置槽位（24；勾选/快捷键提示用）。
    pub fn trailing(mut self, kind: IconKind) -> Self {
        self.trailing = Some(kind);
        self
    }

    /// 选中项（`SecondaryContainer` + `OnSecondaryContainer`）。
    pub fn selected(mut self) -> Self {
        self.selected = true;
        self
    }

    /// 强制视觉态（gallery 状态矩阵取证）。
    pub fn state(mut self, state: Interaction) -> Self {
        self.state = Some(state);
        self
    }
}

/// 菜单里的一行：普通项或分组分隔线。
#[derive(Debug, Clone)]
pub enum Entry<'a, M> {
    /// 一个可点/禁用的项。
    Item(MenuItem<'a, M>),
    /// 分组分隔线（复用 `components::divider`）。
    Divider,
}

/// 菜单弹出层。
#[derive(Debug, Clone)]
pub struct Menu<'a, M> {
    entries: Vec<Entry<'a, M>>,
    width: Length,
    shadow: bool,
}

impl<'a, M> Default for Menu<'a, M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, M> Menu<'a, M> {
    /// 空菜单（默认按容器宽度伸展，宽度由调用方定）。
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            width: Length::Fill,
            shadow: true,
        }
    }

    /// 追加一项。
    pub fn item(mut self, item: MenuItem<'a, M>) -> Self {
        self.entries.push(Entry::Item(item));
        self
    }

    /// 追加一条分组分隔线。
    pub fn divider(mut self) -> Self {
        self.entries.push(Entry::Divider);
        self
    }

    /// 菜单宽度（8.7：不窄于触发元素由调用方保证）。
    pub fn width(mut self, width: Length) -> Self {
        self.width = width;
        self
    }
}

/// 一项的颜色（8.5 / 8.6 / 禁用）。
#[derive(Debug, Clone, Copy, PartialEq)]
struct ItemPalette {
    /// 项底色；`None` = 透明（不画状态层）。
    fill: Option<Color>,
    /// 文本色。
    ink: Color,
    /// 图标色（前置槽位；A1 是 `OnSecondaryContainer`）。
    icon: Color,
    /// 圆角档位（选中指示器 8）。
    shape: ShapeScale,
}

/// 解析一项的颜色：选中优先，其余按五态叠状态层（`interaction::overlay_color`）。
fn item_palette(ui: &Ui, base: Base, state: Interaction, selected: bool) -> ItemPalette {
    if state.is_disabled() {
        // 禁用：内容 38%，叠在**菜单面板的底**上（项本身透明，透出来的就是面板色）
        let ink = interaction::disabled_content_over_role(ui, base, Role::OnSurface, false);
        return ItemPalette {
            fill: None,
            ink,
            icon: ink,
            shape: ShapeScale::Small,
        };
    }

    if selected {
        // 选中项也要跟态走：状态层叠在 SecondaryContainer 上（不透明结果，D27）
        let fill = interaction::overlay_color(
            ui,
            state,
            Role::OnSecondaryContainer,
            Role::SecondaryContainer,
        )
        .unwrap_or_else(|| ui.color(Role::SecondaryContainer));
        return ItemPalette {
            fill: Some(fill),
            ink: ui.color(Role::OnSecondaryContainer),
            icon: ui.color(Role::OnSecondaryContainer),
            shape: ShapeScale::Small,
        };
    }

    ItemPalette {
        // 状态层叠在**菜单容器色**上（`PANEL_BASE` = SurfaceContainer），得到不透明结果（D27）
        fill: interaction::overlay_color(ui, state, Role::OnSurface, base.role()),
        ink: ui.color(Role::OnSurface),
        icon: ui.color(Role::OnSecondaryContainer),
        shape: ShapeScale::Small,
    }
}

/// 菜单弹出层的底（**面板画这个色，项就叠在它上面**）。
///
/// 8.2 的 `MenuTokens.ContainerColor = SurfaceContainer`：面板用 `ui.color(PANEL_BASE.role())` 画底，
/// 项的禁用态（38% 内容）与状态层拿同一个 [`Base`] 合成——两边同源，改色不会只改一半（M234）。
pub const PANEL_BASE: Base = Base::of(Role::SurfaceContainer);

/// 菜单弹出层（`SurfaceContainer` + L2 阴影；[`Menu::flat`] 时无阴影）。
pub fn menu<'a, M: Clone + 'a>(ui: &'a Ui, menu: Menu<'a, M>) -> Element<'a, M> {
    let geometry = MenuGeometry::of(ui);
    let spec = Component::Menu.spec();
    let elevation = spec.elevation;
    debug_assert_eq!(
        elevation,
        Elevation::Level2,
        "MenuTokens.ContainerElevation"
    );
    let shadow = panel_shadow(ui, elevation.shadow(), menu.shadow);
    let panel = ui.color(PANEL_BASE.role());
    let shape = geometry.shape;

    let mut rows = column![].width(Length::Fill);
    for entry in menu.entries {
        rows = rows.push(match entry {
            Entry::Item(item) => item_row(ui, PANEL_BASE, item, geometry),
            // 分组分隔线：复用 `divider::full`（一像素线 + OutlineVariant），不自己画
            Entry::Divider => divider::full::<M>(ui),
        });
    }

    container(layout::inset(0.0, geometry.pad_v, rows))
        .width(menu.width)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(panel)),
            border: Border {
                radius: ui.radius_dp(shape),
                ..Border::default()
            },
            shadow,
            ..container::Style::default()
        })
        .into()
}

/// 一行菜单项（用 `button` 承载命中与四态，几何与颜色自绘）。
fn item_row<'a, M: Clone + 'a>(
    ui: &'a Ui,
    base: Base,
    item: MenuItem<'a, M>,
    geometry: MenuGeometry,
) -> Element<'a, M> {
    let forced = item.state.or(if item.on_press.is_none() {
        Some(Interaction::Disabled)
    } else {
        None
    });
    // 选中项的背景也要跟态走：禁用时不许留满档 SecondaryContainer
    let colors = item_palette(
        ui,
        base,
        forced.unwrap_or(Interaction::Enabled),
        item.selected,
    );
    let body = ui.style(TypeRole::BodyMedium);

    let label = text(item.label)
        .size(body.size_px)
        .line_height(LineHeight::Absolute(Pixels(body.line_height_px)))
        .font(ui.font_of(body))
        .color(colors.ink);

    let mut row = iced::widget::Row::new()
        .align_y(Alignment::Center)
        .spacing(geometry.gap);
    if let Some(kind) = item.leading {
        // Q9：图标必须经固定尺寸容器
        row = row.push(layout::fixed(
            geometry.icon,
            geometry.icon,
            icons::view::<M>(kind, colors.icon),
        ));
    }
    row = row.push(label);
    if let Some(kind) = item.trailing {
        // **撑开**：label 与 trailing 之间插一个水平 Fill，trailing 才会被推到行右内边距上
        // （M3 里 trailing 是「快捷键 / 子菜单箭头 / 勾」，必须在一列菜单里成一条竖线）。
        // 没有 trailing 时**不插**：label 仍在左边，不会被居中或右对齐。
        row = row.push(iced::widget::space::horizontal());
        row = row.push(layout::fixed(
            geometry.icon,
            geometry.icon,
            icons::view::<M>(kind, colors.icon),
        ));
    }

    // 行必须撑满 item 宽度：上面的 Fill 空间要有宽度可撑（Q8 的教训：撑开靠容器，不靠子元素）。
    // 顺带把内容垂直居中——`button` 只把内容放在 padding-box 的左上角。
    let content = iced::widget::container(row)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::Center);

    let selected = item.selected;
    let style = move |_theme: &iced::Theme, status: button::Status| {
        let state = forced.unwrap_or_else(|| Interaction::from_status(status));
        let colors = item_palette(ui, base, state, selected);
        button::Style {
            background: colors.fill.map(Background::Color),
            text_color: colors.ink,
            border: Border {
                radius: ui.radius_dp(colors.shape),
                ..Border::default()
            },
            shadow: Shadow::default(),
            snap: true,
        }
    };

    let inset = ui.space(Space::X1);
    let button = button(content)
        .width(Length::Fill)
        .height(Length::Fixed(geometry.item_height))
        .padding(Padding {
            top: 0.0,
            // 选中指示器水平内缩 4（8.6 / 参考图纸），内容仍落在 12 上：12 = 4 + 8
            right: geometry.pad_h - inset,
            bottom: 0.0,
            left: geometry.pad_h - inset,
        })
        .style(style);

    let element: Element<'a, M> = match item.on_press {
        Some(message) => button.on_press(message).into(),
        None => button.into(),
    };

    container(element)
        .width(Length::Fill)
        .padding(Padding {
            top: 0.0,
            right: inset,
            bottom: 0.0,
            left: inset,
        })
        .into()
}

#[cfg(test)]
mod tests;
