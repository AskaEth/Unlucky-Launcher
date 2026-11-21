//! 搜索（`research/15` §20）：官方是**两个组件**——[`search_bar`]（收起）+ [`search_view`]（展开）。
//!
//! ## 参考值（`[源]` = A1；本机 `material3-android:1.4.0` 的 `SearchBarTokens` / `SearchBar.kt`）
//!
//! | # | 检查项 | 参考值 | 落点 |
//! |---|---|---|---|
//! | 20.1 | 高 + 圆角 | 高 **56**、圆角 **full = 28**（药丸，不是圆角矩形） | `LM.search_bar_height` + `Component::Search.spec().shape` |
//! | 20.2 | 宽 | min **360** / max **720**（G20：iced 无 min-width，只施加 max） | [`SEARCH_MAX_WIDTH`] |
//! | 20.3 | 容器色 / 高程 | **`SurfaceContainerHighest`** + **L0**（token 写 L3，默认值是 L0，按观感取 L0） | `Role::SurfaceContainerHighest` |
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
//!    （那是正文分隔线的角色）。两个角色在 A1 里明确不同，所以这里**只借用画法、不借用颜色**：
//!    线的颜色仍然是 `Role::Outline`，画法是 `divider::hairline`（M456 起；在那之前是本文件自己
//!    一处的 `Role::Outline` 填充盒，会被 wgpu 的 `crisp` 吸附成整物理行——M454 的缺陷）。
//!    复用 `divider::full/list` 那种**带色入口**才会把 20.8 判成「报 OutlineVariant 即错」。
//! 3. bar 的 hover / pressed / disabled 在 `SearchBarTokens` 里**没有** token。
//!    hover/pressed 按通用状态层（`interaction::overlay_color`，8%/10%），disabled 按令牌的
//!    12%/38%，都登记为 `[估]`（D14 / D15）；焦点指示用 A1 的 `FocusIndicatorColor = Secondary`。

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

use super::super::divider;
use super::super::interaction::{self, Base, Interaction};
use super::super::layout;
use super::geometry::{SEARCH_MAX_WIDTH, SEARCH_MIN_WIDTH, SEARCH_VIEW_MIN_HEIGHT};

mod trailing;

/// 搜索条（收起形态）。
///
/// 状态矩阵用 [`SearchBar::state`] 强制；运行期五态由 `Tracker`/`text_input` 的真实事件驱动。
///
/// ## 尾随动作（M460）
///
/// `M` 那个泛型参数只给 [`SearchBar::trailing_action`] 用：页面给一条消息，药丸就在**图标
/// 之后、同一个输入行里**画一个可点的 `IconButton`。不给 = 与 M460 之前逐像素相同（见
/// [`trailing`]），所以下载页那类调用方不用改、也不会变样。
#[derive(Debug, Clone)]
pub struct SearchBar<'a, M = ()> {
    value: &'a str,
    placeholder: &'a str,
    trailing: Option<IconKind>,
    /// 尾随动作（右端那个可点的图标按钮）；`None` = 只有图标槽。
    action: Option<M>,
    state: Option<Interaction>,
    width: Length,
    /// 脚下那一层的底（M234，禁用态合成用）。
    base: Base,
}

impl<'a, M> SearchBar<'a, M> {
    /// 一条收起态搜索条（默认占位文案由页面给，组件内不拼中文）。
    pub fn new(value: &'a str) -> Self {
        Self {
            value,
            placeholder: "",
            trailing: Some(IconKind::Filter),
            action: None,
            state: None,
            width: Length::Fill,
            base: Base::CANVAS,
        }
    }

    /// 右端加一个**尾随动作**（M460）：按下它就是这条消息。
    ///
    /// 画在输入行里（`Filter` 图标之后、不重叠）——MD3 搜索栏对「清除」的惯例位
    /// （`research/15` §20.4 的 trailing 槽），也是页面不必另起一行按钮的理由。
    pub fn trailing_action(mut self, message: M) -> Self {
        self.action = Some(message);
        self
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

    /// **告诉药丸它踩在什么底上**（M234）：禁用态那 12% 的容器叠在这个底上。
    ///
    /// 药丸自己的容器色是 `SurfaceContainerHighest`，但**禁用它不画那个色**——禁用画的是
    /// 「12% 叠父底」，所以这里要的是父底（卡片 = `surfaces::elevated::BASE`），不是它自己。
    pub fn base(mut self, base: impl Into<Base>) -> Self {
        self.base = base.into();
        self
    }
}

/// 搜索条内的一行内容（bar 与 view 头部共用，保证「展开时头部不跳」——20.6）。
#[allow(clippy::too_many_arguments)]
fn bar_content<'a, M: Clone + 'a>(
    ui: &'a Ui,
    base: Base,
    value: &'a str,
    placeholder: &'a str,
    trailing: Option<IconKind>,
    action: Option<M>,
    state: Interaction,
    on_input: impl Fn(String) -> M + 'a,
) -> Element<'a, M> {
    // 20.4：可视边 = pad + offset（**求和**，不抄 16）
    let bar: ComponentMetrics = Component::Search.metrics().expect("Search 有组件级度量");
    // 20.4：图标**可视边** = pad + offset（M69 不变量）。这里反解出偏移，两侧共用同一个量。
    let icon_edge = bar.pad_start + bar.icon_offset;
    let icon_offset = icon_edge - bar.pad_start;

    let ink = if state.is_disabled() {
        interaction::disabled_content_over_role(ui, base, Role::OnSurface, true)
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

    // 尾随位（M460）：图标槽 + 可选动作，是 `contents` 里的**一个子项**——「叉与输入框同一个
    // 容器」就是这么成立的（不是几何测算：它拼进的就是这一行）。图标盒留在本文件建：`icons.rs`
    // 的调用点白名单按**文件**计数（见 `trailing` 模块文档）。
    let tail_icon = trailing.map(|kind| {
        let glyph = layout::fixed(bar.icon, bar.icon, icons::view::<M>(kind, subtle));
        container(glyph)
            .padding(Padding::default().right(icon_offset))
            .into()
    });
    let tail = trailing::trailing(ui, bar, tail_icon, action);
    contents = contents.push(tail.row);

    container(contents)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::Center)
        .padding(Padding {
            top: 0.0,
            right: tail.pad_end,
            bottom: 0.0,
            left: bar.pad_start,
        })
        .into()
}

/// 收起态搜索条（56 高、药丸、`SurfaceContainerHighest`、无可见阴影）。
pub fn search_bar<'a, M: Clone + 'a>(
    ui: &'a Ui,
    bar: SearchBar<'a, M>,
    on_input: impl Fn(String) -> M + 'a,
) -> Element<'a, M> {
    let state = bar.state.unwrap_or(Interaction::Enabled);
    let component = Component::Search.spec();
    let height = ui.metrics().search_bar_height;

    let background = if state.is_disabled() {
        // 禁用：12% 叠在**药丸脚下的底**上（不是药丸自己的 SurfaceContainerHighest）
        interaction::disabled_colors(ui, bar.base).0
    } else {
        // 规格卡里搜索药丸与 filled 字段是**同一个底**（`SurfaceContainerHighest`）：
        // M229 r1 用的是 `High`，比 filled 字段低一档，两块并排时能看出色差（复审 F2 裁决抬档）。
        let role = Role::SurfaceContainerHighest;
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
        bar.base,
        bar.value,
        bar.placeholder,
        bar.trailing,
        bar.action,
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
    // 展开面板**不跟着药丸抬档**：它是浮层（菜单语言，`SurfaceContainerHigh`），
    // 规格卡只画了收起的药丸。两块不同档是层级差，不是漏改（复审 F2 只裁药丸）。
    let panel_role = Role::SurfaceContainerHigh;
    let panel = ui.color(panel_role);
    let header_height = ui.metrics().search_bar_height;
    let body_style = ui.style(TypeRole::BodyMedium);

    // 20.8：头部/结果区分隔是 **`Outline`**（正文分隔线才是 OutlineVariant）
    //
    // M456：这条线走 [`divider::hairline`]（`snap: false` 的自绘 quad）——它原来是本文件自己
    // 一处的 `Outline` 填充盒，在 wgpu 上会被 `crisp` 吸附成整物理行（125% ⇒ 0.80 / 150% ⇒ 1.33，
    // M454 的缺陷）。**颜色角色不变**：仍是 20.8 的 `Outline`（20.8 与 §22 的 `OutlineVariant`
    // 是两条不同语义的线，所以只借画法、不借颜色）。
    let rule = divider::hairline(ui.color(Role::Outline), STROKE_PX);

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
            Base::of(panel_role),
            view.value,
            view.placeholder,
            Some(IconKind::Close),
            None, // 展开面板的尾随位只有「收起」图标，没有动作（M460 的清除叉只在收起态药丸上）
            Interaction::Enabled,
            on_input,
        ))
        .width(Length::Fill)
        .height(Length::Fixed(header_height)),
        rule,
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
