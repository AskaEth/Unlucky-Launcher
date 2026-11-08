//! Tabs（`research/12b` §6）：**Primary**（下方 3px 指示条）与 **Secondary**（1px 分隔线）。
//!
//! ## 参考值（`[源]` = A1，本轮 `javap` 实读本机 AAR）
//!
//! ```text
//! PrimaryNavigationTabTokens:   ContainerHeight 48、IconAndLabelTextContainerHeight 64、
//!                               ActiveIndicatorHeight 3、ActiveIndicatorShape RoundedCornerShape(3dp)、
//!                               IconSize 24、ContainerShape CornerNone、LabelTextFont TitleSmall
//!                               ActiveIndicatorColor=Primary；Active*/Inactive* 各态内容色（见下表）
//! SecondaryNavigationTabTokens: ContainerHeight 48、DividerHeight、DividerColor=SurfaceVariant、
//!                               ActiveLabelTextColor=OnSurface、InactiveLabelTextColor=OnSurfaceVariant
//! ```
//!
//! | 项 | Primary | Secondary |
//! |---|---|---|
//! | 选中内容色 | **`Primary`** | **`OnSurface`**（不是 Primary——6.5 明写） |
//! | 未选内容色 | `OnSurfaceVariant` | `OnSurfaceVariant` |
//! | hover / focus / pressed | 未选项提到 **`OnSurface`**，选中项保持不变 | 同左 |
//! | 底部条 | 仅选中项：**3px `Primary`**，宽度**跟随该 tab** | 所有项：**1px `SurfaceVariant`**（连成一条线） |
//!
//! 尺寸/圆角/线厚全部来自 [`TabsGeometry`]（**G22/G23**：`LayoutMetrics` 没有 Tabs 档）。
//!
//! ## 指示条怎么做到「宽度跟随选中项」
//!
//! iced 没有「取到渲染后文本宽度」的公开 API，所以这里用 [`stack`] 的**分层语义**：
//!
//! 1. `Stack` 的**底层**是 tab 自己的内容容器（`Length::Shrink` → 宽度 = 文本实测宽 + 2×16）；
//! 2. `Stack` 的**上层**是 `Length::Fill` 的指示条。`Stack::layout` 先按**底层**定尺寸，再用
//!    「已知尺寸」的 `Limits` 布局其余层（`iced_widget-0.14.2/src/stack.rs` 的
//!    `size = limits.resolve(self.width, self.height, base.size())`）——于是 Fill 的指示条
//!    拿到的正好是「这个 tab 的宽」，**不是**整行宽。
//!
//! 这也顺带避开 Q8 的另一面：把条写成 `center_x(Fill)` 会把它撑成整行。
//!
//! ## 横向滚动
//!
//! 整行外面套 `scrollable`（`Direction::Horizontal`，滚动条 `Scrollbar::hidden()`）：
//! tab 宽度因此是**内容宽**而不是「行宽 ÷ n」——官方 `ScrollableTabRow` 的口径。
//! `TabMinWidth = 90` **本轮不施加**（iced 无 min-width，登记在 G23）。

use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::{button, container, scrollable, space, text, Row, Stack};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding, Pixels, Shadow};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;
use unlucky_design::typography::TypeRole;

use crate::icons;
use crate::theme::Ui;

use super::super::interaction::{self, Interaction};
use super::super::layout;
use super::geometry::{TabsGeometry, TABS_ICON_HEIGHT};

/// 两种 Tabs（`research/12b` §6；M3 的 `PrimaryTabRow` / `SecondaryTabRow`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TabsVariant {
    /// 主标签页：选中项下方 3px `Primary` 指示条。
    Primary,
    /// 次标签页：1px `SurfaceVariant` 分隔线 + 选中项 `OnSurface` 文字。
    Secondary,
}

impl TabsVariant {
    /// 两种变体（gallery 状态矩阵按这个次序出图）。
    pub const ALL: [TabsVariant; 2] = [TabsVariant::Primary, TabsVariant::Secondary];

    /// 稳定名字（截图文件名 / 图例用）。
    pub const fn name(self) -> &'static str {
        match self {
            TabsVariant::Primary => "primary",
            TabsVariant::Secondary => "secondary",
        }
    }

    /// 是否是 Primary（指示条 3px）。
    pub const fn is_primary(self) -> bool {
        matches!(self, TabsVariant::Primary)
    }
}

/// 一个标签页。
#[derive(Debug, Clone)]
pub struct Tab<'a, M> {
    label: &'a str,
    icon: Option<IconKind>,
    selected: bool,
    on_press: Option<M>,
    state: Option<Interaction>,
}

impl<'a, M> Tab<'a, M> {
    /// 一个未选中的 tab。
    pub fn new(label: &'a str, on_press: M) -> Self {
        Self {
            label,
            icon: None,
            selected: false,
            on_press: Some(on_press),
            state: None,
        }
    }

    /// 选中（内容色换成激活档 + 画指示条）。
    pub fn selected(mut self) -> Self {
        self.selected = true;
        self
    }

    /// 前置图标（24，`IconSize::Toolbar`；带图标的容器高官方是 64，本轮只做文字档）。
    pub fn icon(mut self, kind: IconKind) -> Self {
        self.icon = Some(kind);
        self
    }

    /// 强制视觉态（gallery 取证；不动状态机）。
    pub fn state(mut self, state: Interaction) -> Self {
        self.state = Some(state);
        self
    }

    /// 转禁用：**只清消息**（修饰器，不是构造函数）。
    pub fn disabled(mut self) -> Self {
        self.on_press = None;
        self
    }
}

/// 一行标签页。
#[derive(Debug, Clone)]
pub struct Tabs<'a, M> {
    variant: TabsVariant,
    tabs: Vec<Tab<'a, M>>,
}

impl<'a, M> Tabs<'a, M> {
    /// Primary tabs（3px 指示条）。
    pub fn primary() -> Self {
        Self {
            variant: TabsVariant::Primary,
            tabs: Vec::new(),
        }
    }

    /// Secondary tabs（1px 分隔线）。
    pub fn secondary() -> Self {
        Self {
            variant: TabsVariant::Secondary,
            tabs: Vec::new(),
        }
    }

    /// 追加一个 tab。
    pub fn tab(mut self, tab: Tab<'a, M>) -> Self {
        self.tabs.push(tab);
        self
    }
}

/// 一个 tab 的内容色（A1 的 `Active*` / `Inactive*` / 各态表，见模块文档）。
fn content_color(ui: &Ui, primary: bool, state: Interaction, selected: bool) -> Color {
    if state.is_disabled() {
        // A1 的 Tabs 没有 Disabled* token（实读：Primary/Secondary 两套都只有 Active/Inactive × 四态），
        // 按 ListItem 的 38% 口径处理 —— 登记 `[估]`。
        return interaction::disabled_content_over_role(ui, Role::OnSurface, false);
    }
    if selected {
        // 6.5：Primary 选中 `Primary`；Secondary 选中 `OnSurface`
        return if primary {
            ui.color(Role::Primary)
        } else {
            ui.color(Role::OnSurface)
        };
    }
    match state {
        // 未选中静息 `OnSurfaceVariant`；hover/focus/pressed 提到 `OnSurface`（A1 的 Inactive* 四态）
        Interaction::Enabled => ui.color(Role::OnSurfaceVariant),
        _ => ui.color(Role::OnSurface),
    }
}

/// Primary 指示条的颜色（只有选中项有；禁用项降档，否则「点不动却亮着」）。
fn bar_color(ui: &Ui, state: Interaction, selected: bool) -> Option<Color> {
    if !selected {
        return None;
    }
    if state.is_disabled() {
        return Some(interaction::disabled_content_over_role(
            ui,
            Role::OnSurface,
            false,
        ));
    }
    Some(ui.color(Role::Primary))
}

/// 6.6：Secondary 的分隔线色 = **`SurfaceVariant`**（不是 `OutlineVariant`——那是 §22 的分隔线）。
///
/// 色值只在这里出现一次：整行那条线与逐 tab 的实现都从这里取。
fn secondary_line_color(ui: &Ui) -> Color {
    ui.color(Role::SurfaceVariant)
}

/// 一行标签页（自带横向滚动）。
pub fn tabs<'a, M: Clone + 'a>(ui: &'a Ui, tabs: Tabs<'a, M>) -> Element<'a, M> {
    let geometry = TabsGeometry::of(ui);
    let primary = tabs.variant.is_primary();
    // 6.1：带图标时容器高 **64**（`IconAndLabelTextContainerHeight`），纯文字 48
    let height = if tabs.tabs.iter().any(|tab| tab.icon.is_some()) {
        TABS_ICON_HEIGHT
    } else {
        geometry.height
    };

    let mut row = Row::new()
        .height(Length::Fixed(height))
        .align_y(Alignment::Center);
    for tab in tabs.tabs {
        row = row.push(tab_cell(ui, tab, geometry, primary, height));
    }

    let body: Element<'a, M> = if primary {
        row.into()
    } else {
        // A1 的 Secondary 分隔线是**整行一条直线**，不是每个 tab 一段：
        // 逐段画会在相邻段之间留缝（视觉 r1 P2-2）。这里用 `Stack` 把一条 Fill 线压在整行底下，
        // 底座是 tab 行本身（Shrink → 内容宽），所以线长 = tab 行总宽，横向滚动时跟着走。
        let line = container(space::vertical())
            .width(Length::Fill)
            .height(Length::Fixed(geometry.secondary_bar))
            .style(move |_theme| container::Style {
                background: Some(Background::Color(secondary_line_color(ui))),
                ..container::Style::default()
            });
        Stack::new()
            .push(container(row))
            .push(
                container(line)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_y(Alignment::End),
            )
            .into()
    };

    scrollable(body)
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::hidden(),
        ))
        .height(Length::Fixed(height))
        .into()
}

/// 一个 tab 单元（内容 + 底部条）。
fn tab_cell<'a, M: Clone + 'a>(
    ui: &'a Ui,
    tab: Tab<'a, M>,
    geometry: TabsGeometry,
    primary: bool,
    height: f32,
) -> Element<'a, M> {
    let forced = tab.state.or(if tab.on_press.is_none() {
        Some(Interaction::Disabled)
    } else {
        None
    });
    let state = forced.unwrap_or(Interaction::Enabled);
    let selected = tab.selected;
    let color = content_color(ui, primary, state, selected);
    let style = ui.style(TypeRole::TitleSmall);

    let label = text(tab.label)
        .size(style.size_px)
        .line_height(LineHeight::Absolute(Pixels(style.line_height_px)))
        // 单行（容器高固定 48/64）：长标签让 tab 变宽 + 横向滚动，不折行（偏离 D-M79-5）
        .wrapping(Wrapping::None)
        .font(ui.font_of(style))
        .color(color);

    let mut content = Row::new().align_y(Alignment::Center).spacing(geometry.pad);
    if let Some(kind) = tab.icon {
        content = content.push(layout::fixed(
            geometry.icon,
            geometry.icon,
            icons::view::<M>(kind, color),
        ));
    }
    content = content.push(label);

    let bar_height = geometry.bar(primary);
    // 底层：内容自己决定宽度（Shrink），底部留出条的高度 —— 未选中的 Primary tab 也占同样的高
    let base = container(content)
        .height(Length::Fixed(height))
        // Q21：要给居中就两轴都显式给（`Container` 默认 Left/Top）
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .padding(Padding {
            top: 0.0,
            right: geometry.pad,
            bottom: bar_height,
            left: geometry.pad,
        });

    // Primary：逐 tab 画条（要跟随该 tab 的宽度）；Secondary：整行一条，在 `tabs()` 里画
    let bar = if primary {
        bar_color(ui, state, selected)
    } else {
        None
    };
    let radius = geometry.indicator_radius();
    let mut stack = Stack::new().push(base);
    if let Some(color) = bar {
        // 上层：Fill —— `Stack` 先按底层定尺寸，所以这里的 Fill = 这个 tab 的宽（不是整行）
        stack = stack.push(
            container(
                container(space::vertical())
                    .width(Length::Fill)
                    .height(Length::Fixed(bar_height))
                    .style(move |_theme| container::Style {
                        background: Some(Background::Color(color)),
                        border: Border {
                            radius,
                            ..Border::default()
                        },
                        ..container::Style::default()
                    }),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(Alignment::End),
        );
    }

    let fill = interaction::overlay_color(ui, state, Role::OnSurface, Role::Surface);
    let button = button(stack)
        .height(Length::Fixed(height))
        .padding(0)
        .style(move |_theme, _status| button::Style {
            background: fill.map(Background::Color),
            text_color: color,
            // ContainerShape = CornerNone（tab 本身是直角；圆角只在指示条上）
            border: Border {
                radius: ui.radius_dp(ShapeScale::None),
                ..Border::default()
            },
            shadow: Shadow::default(),
            snap: true,
        });

    match tab.on_press {
        Some(message) => button.on_press(message).into(),
        None => button.into(),
    }
}

#[cfg(test)]
mod tests;
