//! ListItem（`research/12b` §4）：一行 / 两行 / 三行，leading / trailing 槽位，选中态，五态状态层。
//!
//! ## 参考值（`[源]` = A1，本轮 `javap` 实读本机 AAR 的 `ListTokens` / `ListItemColors`）
//!
//! | # | 检查项 | 参考值 | 落点 |
//! |---|---|---|---|
//! | 4.1 | 一行高 | A1 **56** → 本项目 **44**（`LM.row_height`，登记偏离 −12） | [`ListItemGeometry::heights`] |
//! | 4.2 | 两 / 三行高 | A1 **72 / 88** → 本项目 **60 / 76**（每多一行 +16，与官方同 delta） | 同上 |
//! | 4.3 | 左右内边距 | **16 / 16** | `pad_start` / `pad_end` |
//! | 4.4 | leading | 图标 **24** `OnSurfaceVariant` | `IconSize::Toolbar` |
//! | 4.5 | 字级 | 主 `BodyLarge 16/24`、辅助 `BodyMedium 14/20`、overline `LabelSmall 11/16` | `TypeRole` |
//! | 4.7 | 状态 | hover/focus/pressed **文本与图标色不变**，只叠状态层（8/10/10%） | [`interaction`] |
//! | 4.8 | 选中 trailing | 图标 → **`Primary`** | `Role::Primary` |
//!
//! ## 禁用是**修饰器**，不是构造函数（照 `selection/chip.rs` 复审 P2-1 的结论）
//!
//! [`ListItem::disabled`] **只清消息**：图标、选中态、状态层全部保留。早期 chip 的
//! `Chip::disabled(label)` 构造函数把图标和选中态一起丢了，图上是「另一行」，不是「这一行禁用」。
//!
//! ## 禁用内容角色是 **`OnSurface`**，不是默认的 `OnSurfaceVariant`
//!
//! A1 实读（`javap -p -c androidx.compose.material3.tokens.ListTokens`）：
//!
//! ```text
//! ListItemDisabledLabelTextColor       = OnSurface   （+ ListItemDisabledLabelTextOpacity = 0.38f）
//! ListItemDisabledLeadingIconColor     = OnSurface   （+ 0.38f）
//! ListItemDisabledTrailingIconColor    = OnSurface   （+ 0.38f）
//! ```
//!
//! 所以这里必须走 `interaction::disabled_content_over_role(ui, OnSurface, false)`；
//! 用默认的 `disabled_colors()`（写死 `OnSurfaceVariant`）会偏浅一档。
//!
//! ## 选中态：本项目口径（**偏离登记 D-M79-1**）
//!
//! A1 的 `ListItemColors` **没有** selected 容器色（实读字段：containerColor / headline / leadingIcon /
//! overline / supporting / trailingIcon + 三个 disabled），选中只把 trailing 图标换成 `Primary`。
//! mission 要求「选中态（SecondaryContainer）」，且本项目的菜单项（M77）与 drawer 项都用
//! `SecondaryContainer` 表示「当前项」——三处保持同一语言，故 ListItem 也填 `SecondaryContainer`
//! + `OnSecondaryContainer`。**这是实质偏离**：A1 的 ListItem 选中行不换底色。

use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::{button, container, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding, Pixels, Shadow};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;

use crate::icons;
use crate::theme::Ui;

use super::super::interaction::{self, Interaction};
use super::super::layout;
use super::geometry::{
    typography_headline, typography_overline, typography_supporting, ListItemGeometry,
};

/// 一个列表项（一行 / 两行 / 三行由槽位决定）。
#[derive(Debug, Clone)]
pub struct ListItem<'a, M> {
    overline: Option<&'a str>,
    headline: &'a str,
    supporting: Option<&'a str>,
    leading: Option<IconKind>,
    trailing: Option<IconKind>,
    on_press: Option<M>,
    selected: bool,
    state: Option<Interaction>,
}

impl<'a, M> ListItem<'a, M> {
    /// 一行项（主文本 + 可点消息）。
    pub fn new(headline: &'a str, on_press: M) -> Self {
        Self {
            overline: None,
            headline,
            supporting: None,
            leading: None,
            trailing: None,
            on_press: Some(on_press),
            selected: false,
            state: None,
        }
    }

    /// 加一行辅助文本 → **两行**项（4.5：辅助 `BodyMedium`）。
    pub fn supporting(mut self, supporting: &'a str) -> Self {
        self.supporting = Some(supporting);
        self
    }

    /// 加上方 overline → **三行**项（4.5：overline `LabelSmall`，`OnSurfaceVariant`）。
    pub fn overline(mut self, overline: &'a str) -> Self {
        self.overline = Some(overline);
        self
    }

    /// 前置槽位（图标 24，`OnSurfaceVariant`）。
    pub fn leading(mut self, kind: IconKind) -> Self {
        self.leading = Some(kind);
        self
    }

    /// 后置槽位（未选中 `OnSurface`，选中 `Primary`，见 4.8）。
    pub fn trailing(mut self, kind: IconKind) -> Self {
        self.trailing = Some(kind);
        self
    }

    /// 选中态（`SecondaryContainer` + `OnSecondaryContainer`；偏离 D-M79-1 见模块文档）。
    pub fn selected(mut self) -> Self {
        self.selected = true;
        self
    }

    /// 强制视觉态（gallery 状态矩阵取证；不动状态机）。
    pub fn state(mut self, state: Interaction) -> Self {
        self.state = Some(state);
        self
    }

    /// 转禁用：**只清消息**，槽位与选中态一律保留。
    pub fn disabled(mut self) -> Self {
        self.on_press = None;
        self
    }

    /// 行数（1 + 有 overline + 有 supporting）。
    pub fn lines(&self) -> usize {
        usize::from(self.overline.is_some()) + 1 + usize::from(self.supporting.is_some())
    }
}

/// 一项的颜色（按态 + 选中求值；禁用按 A1 的 `OnSurface`@38%）。
#[derive(Debug, Clone, Copy, PartialEq)]
struct ItemPalette {
    fill: Option<Color>,
    headline: Color,
    supporting: Color,
    leading: Color,
    trailing: Color,
}

fn palette(ui: &Ui, state: Interaction, selected: bool) -> ItemPalette {
    if state.is_disabled() {
        // A1：ListItemDisabled*Color = OnSurface × 0.38（无容器时叠在 surface 上）
        let ink = interaction::disabled_content_over_role(ui, Role::OnSurface, false);
        return ItemPalette {
            fill: None,
            headline: ink,
            supporting: ink,
            leading: ink,
            trailing: ink,
        };
    }

    if selected {
        let fill = interaction::overlay_color(
            ui,
            state,
            Role::OnSecondaryContainer,
            Role::SecondaryContainer,
        )
        .unwrap_or_else(|| ui.color(Role::SecondaryContainer));
        return ItemPalette {
            fill: Some(fill),
            headline: ui.color(Role::OnSecondaryContainer),
            supporting: ui.color(Role::OnSecondaryContainer),
            leading: ui.color(Role::OnSecondaryContainer),
            // 4.8：选中时 trailing 图标是 `Primary`（A1 ListItemSelectedTrailingIconColor）
            trailing: ui.color(Role::Primary),
        };
    }

    ItemPalette {
        // 状态层叠在**行自己的底**（Surface）上，得到不透明结果（D27）
        fill: interaction::overlay_color(ui, state, Role::OnSurface, Role::Surface),
        headline: ui.color(Role::OnSurface),
        supporting: ui.color(Role::OnSurfaceVariant),
        leading: ui.color(Role::OnSurfaceVariant),
        trailing: ui.color(Role::OnSurface),
    }
}

/// 一行文本（显式行高：三行块的 16+24+20 必须能精确等于 60，靠的就是它）。
fn line<'a, M: 'a>(
    ui: &'a Ui,
    value: &'a str,
    role: unlucky_design::typography::TypeRole,
    color: Color,
) -> Element<'a, M> {
    let style = ui.style(role);
    text(value)
        .size(style.size_px)
        .line_height(LineHeight::Absolute(Pixels(style.line_height_px)))
        // **单行**：行高是固定值（44/60/76），`Text` 默认 `Wrapping::Word` 会把长实例名
        // 折成两行、把三行块顶穿。官方要的是单行 ellipsis，iced 没有 ellipsis（D19 先例）
        // → 单行 + 超长由外层 `clip(true)` 直接切（偏离 D-M79-5）。
        .wrapping(Wrapping::None)
        .font(ui.font_of(style))
        .color(color)
        .into()
}

/// 列表项（`research/12b` §4）。
pub fn list_item<'a, M: Clone + 'a>(ui: &'a Ui, item: ListItem<'a, M>) -> Element<'a, M> {
    let geometry = ListItemGeometry::of(ui);
    let lines = item.lines();
    let height = geometry.height(lines);
    // 禁用 = 「没有消息」；`state` 是取证强制值，优先级更高（矩阵里画 hover 的禁用项也成立）
    let forced = item.state.or(if item.on_press.is_none() {
        Some(Interaction::Disabled)
    } else {
        None
    });
    let colors = palette(ui, forced.unwrap_or(Interaction::Enabled), item.selected);

    let mut body = iced::widget::Column::new().spacing(0);
    if let Some(overline) = item.overline {
        body = body.push(line::<M>(
            ui,
            overline,
            typography_overline(),
            colors.supporting,
        ));
    }
    body = body.push(line::<M>(
        ui,
        item.headline,
        typography_headline(),
        colors.headline,
    ));
    if let Some(supporting) = item.supporting {
        body = body.push(line::<M>(
            ui,
            supporting,
            typography_supporting(),
            colors.supporting,
        ));
    }

    let mut content = iced::widget::Row::new()
        .align_y(Alignment::Center)
        .spacing(geometry.gap);
    if let Some(kind) = item.leading {
        // Q9：图标必须经固定尺寸容器（否则会被按钮的 Fill 约束撑满）
        content = content.push(layout::fixed(
            geometry.icon,
            geometry.icon,
            icons::view::<M>(kind, colors.leading),
        ));
    }
    // 文本块吃掉剩余宽度并**裁切**：单行超长时切在行内边距上，不会压到 trailing
    content = content.push(container(body).width(Length::Fill).clip(true));
    if let Some(kind) = item.trailing {
        content = content.push(layout::fixed(
            geometry.icon,
            geometry.icon,
            icons::view::<M>(kind, colors.trailing),
        ));
    }

    let pad_y = geometry.pad_y(ui, lines);
    // 行必须撑满 item 宽度（Fill 空间要有宽度可撑），垂直靠 align_y 居中（`button` 默认贴左上）
    let body = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::Center);

    let selected = item.selected;
    let style = move |_theme: &iced::Theme, status: button::Status| {
        let state = forced.unwrap_or_else(|| Interaction::from_status(status));
        let colors = palette(ui, state, selected);
        button::Style {
            background: colors.fill.map(Background::Color),
            text_color: colors.headline,
            // 4.x：ListItem 的形状是 `CornerNone`（直角），没有圆角档
            border: Border {
                radius: ui.radius_dp(ShapeScale::None),
                ..Border::default()
            },
            shadow: Shadow::default(),
            snap: true,
        }
    };

    let button = button(body)
        .width(Length::Fill)
        .height(Length::Fixed(height))
        .padding(Padding {
            top: pad_y,
            right: geometry.pad_end,
            bottom: pad_y,
            left: geometry.pad_start,
        })
        .style(style);

    let element: Element<'a, M> = match item.on_press {
        Some(message) => button.on_press(message).into(),
        None => button.into(),
    };
    element
}

#[cfg(test)]
mod tests;
