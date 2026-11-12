//! 导航项的公共类型与一行渲染（`drawer.rs` 的私有子模块：单文件 ≤400 行，AGENTS.md §3-1）。
//!
//! 拆出去的是「一项长什么样」（`NavItem` / `NavGroup` + 颜色求值 + 一行布局），
//! `drawer.rs` 只留「容器长什么样」（宽度、圆角、分组标题、分隔线）。

use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::{button, container, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding, Pixels, Shadow};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::typography::TypeRole;

use crate::icons;
use crate::theme::Ui;

use super::super::super::interaction::{self, Base, Interaction};
use super::super::super::layout;
use super::super::geometry::{NavLayout, NavPaneGeometry};

/// 一条导航项。
#[derive(Debug, Clone)]
pub struct NavItem<'a, M> {
    pub(super) label: &'a str,
    pub(super) icon: IconKind,
    pub(super) selected: bool,
    pub(super) on_press: Option<M>,
    pub(super) state: Option<Interaction>,
}

impl<'a, M> NavItem<'a, M> {
    /// 一个可点的导航项（图标 + 文案）。
    pub fn new(label: &'a str, icon: IconKind, on_press: M) -> Self {
        Self {
            label,
            icon,
            selected: false,
            on_press: Some(on_press),
            state: None,
        }
    }

    /// 当前项（药丸 + `OnSecondaryContainer`）。
    pub fn selected(mut self) -> Self {
        self.selected = true;
        self
    }

    /// 强制视觉态（gallery 取证；不动状态机）。
    pub fn state(mut self, state: Interaction) -> Self {
        self.state = Some(state);
        self
    }

    /// 转禁用：**只清消息**（修饰器；图标 / 选中态 / 槽位全部保留）。
    pub fn disabled(mut self) -> Self {
        self.on_press = None;
        self
    }
}

/// 一个分组（drawer 侧可带标题；rail 侧标题会被忽略——rail 只有图标）。
#[derive(Debug, Clone)]
pub struct NavGroup<'a, M> {
    pub(super) title: Option<&'a str>,
    pub(super) items: Vec<NavItem<'a, M>>,
}

impl<'a, M> NavGroup<'a, M> {
    /// 无标题分组。
    pub fn new() -> Self {
        Self {
            title: None,
            items: Vec::new(),
        }
    }

    /// 带标题分组（§21.8：`TitleSmall` + `OnSurfaceVariant`，不可点）。
    pub fn titled(title: &'a str) -> Self {
        Self {
            title: Some(title),
            items: Vec::new(),
        }
    }

    /// 追加一项。
    pub fn item(mut self, item: NavItem<'a, M>) -> Self {
        self.items.push(item);
        self
    }
}

impl<'a, M> Default for NavGroup<'a, M> {
    fn default() -> Self {
        Self::new()
    }
}

/// 一项的颜色（按态 + 选中求值）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ItemPalette {
    /// 药丸底色（`None` = 不画药丸 / 不叠状态层）。
    pub(super) pill: Option<Color>,
    /// 图标色。
    pub(super) icon: Color,
    /// 标签色。
    pub(super) label: Color,
}

pub(super) fn palette(
    ui: &Ui,
    base: Base,
    layout: NavLayout,
    state: Interaction,
    selected: bool,
) -> ItemPalette {
    if state.is_disabled() {
        let ink = interaction::disabled_content_over_role(ui, base, Role::OnSurface, false);
        return ItemPalette {
            pill: None,
            icon: ink,
            label: ink,
        };
    }

    if selected {
        let pill = interaction::overlay_color(
            ui,
            state,
            Role::OnSecondaryContainer,
            Role::SecondaryContainer,
        )
        .unwrap_or_else(|| ui.color(Role::SecondaryContainer));
        return ItemPalette {
            pill: Some(pill),
            icon: ui.color(Role::OnSecondaryContainer),
            // rail 的选中标签是 `Secondary`（12b 5.6）；drawer 是 `OnSecondaryContainer`（§21.7）
            label: match layout {
                NavLayout::Rail => ui.color(Role::Secondary),
                NavLayout::Drawer => ui.color(Role::OnSecondaryContainer),
            },
        };
    }

    // 未选中：静息 `OnSurfaceVariant`，hover/focus/pressed 提到 `OnSurface`
    let ink = match state {
        Interaction::Enabled => ui.color(Role::OnSurfaceVariant),
        _ => ui.color(Role::OnSurface),
    };
    ItemPalette {
        pill: interaction::overlay_color(ui, state, Role::OnSurface, base.role()),
        icon: ink,
        label: ink,
    }
}

/// 一块文字（显式行高，让项高与行高对得上）。
pub(super) fn line<'a, M: 'a>(
    ui: &'a Ui,
    value: &'a str,
    role: TypeRole,
    color: Color,
) -> Element<'a, M> {
    let style = ui.style(role);
    text(value)
        .size(style.size_px)
        .line_height(LineHeight::Absolute(Pixels(style.line_height_px)))
        // 单行（项高固定 52/56）：长分组名/实例名不许折行（偏离 D-M79-5）
        .wrapping(Wrapping::None)
        .font(ui.font_of(style))
        .color(color)
        .into()
}

/// 一项（rail：图标在上、标签在下；drawer：图标在左、标签在右）。
pub(super) fn nav_row<'a, M: Clone + 'a>(
    ui: &'a Ui,
    base: Base,
    item: NavItem<'a, M>,
    geometry: NavPaneGeometry,
) -> Element<'a, M> {
    let forced = item.state.or(if item.on_press.is_none() {
        Some(Interaction::Disabled)
    } else {
        None
    });
    let state = forced.unwrap_or(Interaction::Enabled);
    let colors = palette(ui, base, geometry.layout, state, item.selected);
    let radius = iced::border::Radius::new(geometry.pill_height / 2.0);

    let icon = layout::fixed(
        geometry.icon,
        geometry.icon,
        icons::view::<M>(item.icon, colors.icon),
    );

    // 药丸本体：宽高都来自几何（rail 56×32 / drawer 216×56），居中交给父级 align_x
    let pill = move |content: Element<'a, M>| -> Element<'a, M> {
        let fill = colors.pill;
        container(content)
            .width(Length::Fixed(geometry.pill_width))
            .height(Length::Fixed(geometry.pill_height))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(move |_theme| container::Style {
                background: fill.map(Background::Color),
                border: Border {
                    radius,
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .into()
    };

    let body: Element<'a, M> = match geometry.layout {
        NavLayout::Rail => {
            // rail：药丸（56×32，图标居中）+ 4 + 标签（LabelMedium）
            let mut stack = iced::widget::Column::new()
                .spacing(geometry.gap)
                .width(Length::Fixed(geometry.width))
                .align_x(Alignment::Center);
            stack = stack.push(pill(icon));
            // 标签要裁到轨宽：单行超长时切在 88 内，不会溢出到内容区（D-M79-5）
            stack = stack.push(
                container(line::<M>(
                    ui,
                    item.label,
                    TypeRole::LabelMedium,
                    colors.label,
                ))
                .width(Length::Fixed(geometry.width))
                .align_x(Alignment::Center)
                .clip(true),
            );
            stack.into()
        }
        NavLayout::Drawer => {
            // drawer：药丸就是一整行（216×56），内容从 pad_start 16 起、图标↔label 12
            let row = iced::widget::Row::new()
                .align_y(Alignment::Center)
                .spacing(geometry.gap)
                .padding(Padding {
                    top: 0.0,
                    right: geometry.pad_end,
                    bottom: 0.0,
                    left: geometry.pad_start,
                });
            // label 吃掉剩余宽度并裁切（单行 + 不压到药丸右内边距，D-M79-5）
            pill(Element::from(
                row.push(icon)
                    .push(
                        container(line::<M>(
                            ui,
                            item.label,
                            TypeRole::LabelLarge,
                            colors.label,
                        ))
                        .width(Length::Fill)
                        .clip(true),
                    )
                    .width(Length::Fill),
            ))
        }
    };

    let height = geometry.item_height;
    let clickable = button(body)
        .width(Length::Fill)
        .height(Length::Fixed(height))
        .padding(0)
        .style(move |_theme, _status| button::Style {
            // 底色由药丸给（rail 的 hover 层也画在药丸上，与 drawer 同口径）
            background: None,
            text_color: colors.label,
            border: Border::default(),
            shadow: Shadow::default(),
            snap: true,
        });
    let element: Element<'a, M> = match item.on_press {
        Some(message) => clickable.on_press(message).into(),
        None => clickable.into(),
    };

    // drawer 的项外距 12（`ItemPadding`）：药丸两侧由内缩给出，所以整行再包一层水平内边距
    match geometry.layout {
        NavLayout::Drawer => container(element)
            .width(Length::Fill)
            .padding(Padding {
                top: 0.0,
                right: geometry.pill_inset,
                bottom: 0.0,
                left: geometry.pill_inset,
            })
            .into(),
        NavLayout::Rail => element,
    }
}
