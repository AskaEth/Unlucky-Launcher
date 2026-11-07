//! 搜索（`research/15` §20）：官方是**两个组件**——[`search_bar`]（收起）+ [`search_view`]（展开）。
//!
//! ## 参考值（`[源]` = A1；本机 `material3-android:1.4.0` 的 `SearchBarTokens` / `SearchBar.kt`）
//!
//! | # | 检查项 | 参考值 | 落点 |
//! |---|---|---|---|
//! | 20.1 | 高 + 圆角 | 高 **56**、圆角 **full = 28**（药丸，不是圆角矩形） | `LM.search_bar_height` + `Component::Search.spec().shape` |
//! | 20.2 | 宽 | min **360** / max **720**（G20：iced 无 min-width，只施加 max） | [`SEARCH_MAX_WIDTH`](super::geometry::SEARCH_MAX_WIDTH) |
//! | 20.3 | 容器色 / 高程 | `SurfaceContainerHigh` + **L0**（token 写 L3，默认值是 L0，按观感取 L0） | `Role::SurfaceContainerHigh` |
//! | 20.4 | 图标位 | leading 24/`OnSurface`、trailing 24/`OnSurfaceVariant`；**可视边 16 = pad 12 + offset 4** | `ComponentMetrics::SEARCH_BAR` |
//! | 20.5 | 文本 / 占位 / 聚焦 | 输入 `BodyLarge`+`OnSurface`；占位 `OnSurfaceVariant`；聚焦 `Secondary` 1px 指示 | `TypeRole` / `Role::Secondary` |
//! | 20.6 | docked 面板 | 头部 **56**、形状 `CornerExtraLarge` = **28** | `ShapeScale::ExtraLarge` |
//! | 20.8 | 头部 / 结果区分隔 | **1px `Outline`**（**不是** `OutlineVariant`！） | 见下 |
//! | 20.9 | 面板尺寸 | 顶距 8、最小高 **240**、docked 宽 **360** | §20B |
//!
//! ## 三处实现口径
//!
//! 1. **图标可视边一律 `pad + icon_offset` 求和**（`SEARCH_BAR.pad_start = 12` + `icon_offset = 4` = 16）。
//!    M69 把这两个值钉成不变量，本文件**不写 16、也不写 12/4**——只做加法。
//! 2. `20.8` 的分隔线用 `Role::Outline`，而 `components::divider` 给的是 `OutlineVariant`
//!    （那是正文分隔线的角色）。两个角色在 A1 里明确不同，所以这里**不复用** `divider`，
//!    只按一像素线（`stroke::HAIRLINE`）画一条 —— 复用会把 20.8 判成「报 OutlineVariant 即错」。
//! 3. bar 的 hover / pressed / disabled 在 `SearchBarTokens` 里**没有** token。
//!    hover/pressed 按通用状态层（`interaction::overlay_color`，8%/10%），disabled 按令牌的
//!    12%/38%，都登记为 [估]（D14 / D15）；焦点指示用 A1 的 `FocusIndicatorColor = Secondary`。

use iced::widget::text::LineHeight;
use iced::widget::{column, container, row, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding, Pixels};

use unlucky_design::color::Role;
use unlucky_design::component_map::{Component, ComponentMetrics};
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::icons;
use crate::theme::{Ui, STROKE_PX};

use super::super::interaction::{self, Interaction};
use super::super::layout;
use super::geometry::{SEARCH_MAX_WIDTH, SEARCH_MIN_WIDTH, SEARCH_VIEW_MIN_HEIGHT};

/// 搜索条（收起形态）。
///
/// 状态矩阵用 [`SearchBar::state`] 强制；运行期五态由 `Tracker`/`text_input` 的真实事件驱动。
#[derive(Debug, Clone)]
pub struct SearchBar<'a> {
    value: &'a str,
    placeholder: &'a str,
    trailing: Option<IconKind>,
    state: Option<Interaction>,
    width: Length,
}

impl<'a> SearchBar<'a> {
    /// 一条收起态搜索条（默认占位文案由页面给，组件内不拼中文）。
    pub fn new(value: &'a str) -> Self {
        Self {
            value,
            placeholder: "",
            trailing: Some(IconKind::Filter),
            state: None,
            width: Length::Fill,
        }
    }

    /// 占位文案。
    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = placeholder;
        self
    }

    /// 强制视觉态（gallery 取证）。
    pub fn state(mut self, state: Interaction) -> Self {
        self.state = Some(state);
        self
    }

    /// 定宽（默认 `Fill`）。
    pub fn width(mut self, width: Length) -> Self {
        self.width = width;
        self
    }
}

/// 搜索条内的一行内容（bar 与 view 头部共用，保证「展开时头部不跳」——20.6）。
#[allow(clippy::too_many_arguments)]
fn bar_content<'a, M: Clone + 'a>(
    ui: &'a Ui,
    value: &'a str,
    placeholder: &'a str,
    trailing: Option<IconKind>,
    state: Interaction,
    on_input: impl Fn(String) -> M + 'a,
) -> Element<'a, M> {
    // 20.4：可视边 = pad + offset（**求和**，不抄 16）
    let bar: ComponentMetrics = Component::Search.metrics().expect("Search 有组件级度量");
    // 20.4：图标**可视边** = pad + offset（M69 不变量）。这里反解出偏移，两侧共用同一个量。
    let icon_edge = bar.pad_start + bar.icon_offset;
    let icon_offset = icon_edge - bar.pad_start;

    let ink = if state.is_disabled() {
        interaction::disabled_content_over_role(ui, Role::OnSurface, true)
    } else {
        ui.color(Role::OnSurface)
    };
    let subtle = if state.is_disabled() {
        ink
    } else {
        ui.color(Role::OnSurfaceVariant)
    };

    let body_style = ui.style(TypeRole::BodyLarge);
    let input = text_input(placeholder, value)
        .on_input_maybe(if state.is_disabled() {
            None
        } else {
            Some(on_input)
        })
        .size(body_style.size_px)
        .line_height(LineHeight::Absolute(Pixels(body_style.line_height_px)))
        .font(ui.font_of(body_style))
        .padding(Padding::default())
        .width(Length::Fill)
        .style(move |_theme, _status| text_input::Style {
            background: Background::Color(Color::TRANSPARENT),
            border: Border::default(),
            icon: subtle,
            placeholder: subtle,
            value: ink,
            selection: ink,
        });

    // 前置图标：左边 = pad（容器内边距 12）+ offset（4）= 16。两层分别来自令牌的两个字段。
    let leading = container(layout::fixed(
        bar.icon,
        bar.icon,
        icons::view::<M>(IconKind::Search, ink),
    ))
    .padding(Padding {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: icon_offset,
    });

    let mut contents = row![leading, input]
        .align_y(Alignment::Center)
        .spacing(bar.gap);

    if let Some(kind) = trailing {
        // 尾随图标：右边 = pad_end + offset（镜像同式），所以右缘也在 16 上
        let trailing = container(layout::fixed(
            bar.icon,
            bar.icon,
            icons::view::<M>(kind, subtle),
        ))
        .padding(Padding {
            top: 0.0,
            right: icon_offset,
            bottom: 0.0,
            left: 0.0,
        });
        contents = contents.push(trailing);
    }

    container(contents)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::Center)
        .padding(Padding {
            top: 0.0,
            right: bar.pad_end,
            bottom: 0.0,
            left: bar.pad_start,
        })
        .into()
}

/// 收起态搜索条（56 高、药丸、`SurfaceContainerHigh`、无可见阴影）。
pub fn search_bar<'a, M: Clone + 'a>(
    ui: &'a Ui,
    bar: SearchBar<'a>,
    on_input: impl Fn(String) -> M + 'a,
) -> Element<'a, M> {
    let state = bar.state.unwrap_or(Interaction::Enabled);
    let component = Component::Search.spec();
    let height = ui.metrics().search_bar_height;

    let background = if state.is_disabled() {
        interaction::disabled_colors(ui, Role::Surface).0
    } else {
        let role = Role::SurfaceContainerHigh;
        interaction::overlay_color(ui, state, Role::OnSurface, role).unwrap_or(ui.color(role))
    };

    // 20.5：聚焦指示 = 1px `Secondary`（A1 的 token 存在但 Compose 未消费 → 标 [规格]）
    let focus_ring = matches!(state, Interaction::Focused);
    let border = Border {
        color: if focus_ring {
            ui.color(Role::Secondary)
        } else {
            Color::TRANSPARENT
        },
        width: if focus_ring { STROKE_PX } else { 0.0 },
        radius: ui.radius(component.shape, height),
    };

    container(bar_content(
        ui,
        bar.value,
        bar.placeholder,
        bar.trailing,
        state,
        on_input,
    ))
    .width(bar.width)
    // 20.2：max **720**（`SearchBarMaxWidth`）。min 360 无法在 iced 表达（G20），由页面保证列宽。
    .max_width(SEARCH_MAX_WIDTH)
    .height(Length::Fixed(height))
    .style(move |_theme| container::Style {
        background: Some(Background::Color(background)),
        border,
        ..container::Style::default()
    })
    .into()
}

/// 展开态搜索面板（docked 形态：桌面 1180 窗口永远用它，`20C` 的结论）。
///
/// 形状：头部 56（半径 28）+ 1px `Outline` 分隔 + 结果区（最小 240）。
#[derive(Debug, Clone)]
pub struct SearchView<'a> {
    value: &'a str,
    placeholder: &'a str,
    empty: &'a str,
    results: &'a [&'a str],
    width: f32,
}

impl<'a> SearchView<'a> {
    /// 一个展开面板（`empty` = 空结果文案，由页面给 i18n 文本）。
    pub fn new(value: &'a str, results: &'a [&'a str], empty: &'a str) -> Self {
        Self {
            value,
            placeholder: "",
            empty,
            results,
            width: SEARCH_MIN_WIDTH,
        }
    }

    /// 占位文案。
    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = placeholder;
        self
    }
}

/// 展开态搜索面板。
pub fn search_view<'a, M: Clone + 'a>(
    ui: &'a Ui,
    view: SearchView<'a>,
    on_input: impl Fn(String) -> M + 'a,
) -> Element<'a, M> {
    let panel = ui.color(Role::SurfaceContainerHigh);
    let header_height = ui.metrics().search_bar_height;
    let body_style = ui.style(TypeRole::BodyMedium);

    // 20.8：头部/结果区分隔是 **`Outline`**（正文分隔线才是 OutlineVariant）
    let divider = container(iced::widget::space::horizontal())
        .width(Length::Fill)
        .height(Length::Fixed(STROKE_PX))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(ui.color(Role::Outline))),
            ..container::Style::default()
        });

    let mut rows = column![].width(Length::Fill).spacing(0);
    for result in view.results {
        rows = rows.push(
            container(
                text(*result)
                    .size(body_style.size_px)
                    .font(ui.font_of(body_style))
                    .color(ui.color(Role::OnSurface)),
            )
            .width(Length::Fill)
            .padding(Padding {
                top: ui.space(Space::X2),
                right: ui.space(Space::X4),
                bottom: ui.space(Space::X2),
                left: ui.space(Space::X4),
            }),
        );
    }
    if view.results.is_empty() {
        let small = ui.style(TypeRole::BodySmall);
        rows = rows.push(
            container(
                text(view.empty)
                    .size(small.size_px)
                    .font(ui.font_of(small))
                    .color(ui.color(Role::OnSurfaceVariant)),
            )
            .width(Length::Fill)
            .padding(Padding {
                top: ui.space(Space::X4),
                right: ui.space(Space::X4),
                bottom: ui.space(Space::X4),
                left: ui.space(Space::X4),
            }),
        );
    }

    let body = container(rows)
        .width(Length::Fill)
        .height(Length::Fixed(SEARCH_VIEW_MIN_HEIGHT))
        .align_y(Alignment::Start);

    let panel_content = column![
        container(bar_content(
            ui,
            view.value,
            view.placeholder,
            Some(IconKind::Close),
            Interaction::Enabled,
            on_input,
        ))
        .width(Length::Fill)
        .height(Length::Fixed(header_height)),
        divider,
        body,
    ];

    container(panel_content)
        .width(Length::Fixed(view.width))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(panel)),
            // 20.6：docked 面板 = CornerExtraLarge（28）；bar ⇄ view 半径不变（§20C）
            border: Border {
                radius: ui.radius_dp(ShapeScale::ExtraLarge),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

#[cfg(test)]
mod tests;
