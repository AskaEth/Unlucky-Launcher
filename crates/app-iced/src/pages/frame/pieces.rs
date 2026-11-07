//! 页面用的**占位控件**：卡片 / 分隔线 / 空态 / 文本按钮 / 图标 / 容器样式。
//!
//! 全部走令牌（颜色、圆角、排版、间距、一像素线），但**没有完整状态机**——
//! 这些是给页面骨架站稳版式用的临时件，M74/M75 的 `components/` 家族合入后由另一条 mission 替换。
//! 每个函数都在文档里点明它对应哪个组件的档位（12b 的参考值）。

use iced::widget::text::LineHeight;
use iced::widget::{button, column, container, row, rule, scrollable, space, svg};
use iced::Theme as IcedTheme;
use iced::{Alignment, Background, Border, Color, Element, Length, Padding, Pixels, Shadow};

use unlucky_design::color::{Role, StateLayer};
use unlucky_design::component_map::Component;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::icons::{IconKind, IconPaint};
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::{TypeRole, TypeStyle};

use crate::icons;
use crate::pages::{Action, Message};
use crate::theme::Ui;

/// 空态图标块边长（12b 15.1：96×96，圆角 `Large`）。
pub(super) const EMPTY_ICON_BLOCK: f32 = 96.0;
/// 空态字形渲染尺寸（12b 15.1：图标 32~48 `[估]`，取 40）。
pub(super) const EMPTY_ICON_GLYPH: f32 = 40.0;
/// 滚动条轨宽（视觉复审 P2-2：原来用 iced 默认的 10px，压过了列表内容）。
///
/// 取 6：细到不抢视线，又还在命中/可见的下限之上；滑块做成 full 圆角药丸（半径 3）。
pub const SCROLLBAR_WIDTH: f32 = 6.0;
/// 滑块离卡片内边的留白（2px，避免贴边）。
pub const SCROLLBAR_MARGIN: f32 = 2.0;

/// 描边卡片（占位：M75 的 `components` 会提供真正的 `OutlinedCard`）。
pub fn card<'a>(ui: &'a Ui, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    let spec = Component::OutlinedCard.spec();
    container(content)
        .width(Length::Fill)
        .padding(ui.metrics().card_padding)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(ui.layer(MaterialLayer::Card))),
            border: ui.outlined(spec.shape),
            ..container::Style::default()
        })
        .into()
}

/// 一像素分隔线（`OutlineVariant`，厚度取令牌）。
pub fn divider<'a>(ui: &'a Ui) -> Element<'a, Message> {
    rule::horizontal::<IcedTheme>(ui.metrics().divider_thickness)
        .style(move |_theme| rule::Style {
            color: ui.color(Role::OutlineVariant),
            radius: ui.radius_dp(ShapeScale::None),
            fill_mode: rule::FillMode::Full,
            snap: true,
        })
        .into()
}

/// 空态（12b §15）：图标块 96 + 24 标题 + 12 副文案 + ≤3 个快捷入口，**整块垂直居中**。
pub fn empty_state<'a>(
    ui: &'a Ui,
    icon: IconKind,
    title: &'a str,
    body: &'a str,
    actions: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    let spec = Component::EmptyState.spec();

    let block = container(glyph(
        icon,
        ui.color(Role::OnSurfaceVariant),
        EMPTY_ICON_GLYPH,
    ))
    .width(EMPTY_ICON_BLOCK)
    .height(EMPTY_ICON_BLOCK)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_theme| container::Style {
        background: Some(Background::Color(ui.layer(MaterialLayer::Overlay))),
        border: Border {
            radius: ui.radius_dp(spec.shape),
            ..Border::default()
        },
        ..container::Style::default()
    });

    let mut stack = column![
        block,
        text_role_centered(ui, spec.text, false, title, ui.color(Role::OnSurface)),
        text_role_centered(
            ui,
            spec.support.unwrap_or(TypeRole::BodySmall),
            false,
            body,
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X3))
    .align_x(Alignment::Center);

    if !actions.is_empty() {
        stack = stack.push(
            row(actions)
                .spacing(ui.space(Space::X2))
                .align_y(Alignment::Center),
        );
    }

    container(stack)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
}

/// 文本按钮占位（TextButton 档：透明底、`OnSurfaceVariant`、40 高、full 圆角）。
///
/// 只画静息、hover、pressed 三态（focus/disabled 归 M75 的 `interaction.rs`）；
/// 消息是页面自己的 [`Message::Action`]，与 app 的 `Message` 完全解耦。
pub fn text_action<'a>(ui: &'a Ui, label: &'a str, action: Action) -> Element<'a, Message> {
    let spec = Component::TextButton.spec();
    let height = ui.metrics().control_height;

    let content = text_role(
        ui,
        spec.text,
        false,
        label,
        ui.color(Role::OnSurfaceVariant),
    );

    button(container(content).align_y(Alignment::Center))
        .height(height)
        .padding(Padding::from([0.0, ui.space(Space::X4)]))
        .style(move |_theme: &IcedTheme, status| {
            let background = match status {
                button::Status::Hovered => Some(Background::Color(ui.state_over(
                    StateLayer::Hover,
                    Role::OnSurface,
                    Role::Surface,
                ))),
                button::Status::Pressed => Some(Background::Color(ui.state_over(
                    StateLayer::Pressed,
                    Role::OnSurface,
                    Role::Surface,
                ))),
                button::Status::Active | button::Status::Disabled => None,
            };

            button::Style {
                background,
                text_color: ui.color(Role::OnSurfaceVariant),
                border: Border {
                    radius: ui.radius(ShapeScale::Full, height),
                    ..Border::default()
                },
                shadow: Shadow::default(),
                snap: true,
            }
        })
        .on_press(Message::Action(action))
        .into()
}

/// 令牌排版档 → 文字元素（字号 / 行高 / 字重全来自令牌，不写字面量）。
pub fn text_role<'a>(
    ui: &'a Ui,
    role: TypeRole,
    emphasized: bool,
    content: &'a str,
    color: Color,
) -> Element<'a, Message> {
    text_widget(ui, role, emphasized, content, color).into()
}

/// 同 [`text_role`]，但文案是**拥有的**字符串（计数、时间戳这类运行时格式化出来的短串）。
///
/// 为什么需要它：文字控件**借用**它的内容（`&'a str`），所以 `format!` 出来的临时串不能直接进树；
/// iced 的 `text()` 接受 `String`（`IntoFragment` 为 `String` 实现了 owned 分支），这里把
/// 「令牌化 + owned 内容」两件事合成一处，页面就不必为了一个计数往视图模型里塞字段。
pub fn text_role_owned<'a>(
    ui: &Ui,
    role: TypeRole,
    emphasized: bool,
    content: String,
    color: Color,
) -> Element<'a, Message> {
    let style: TypeStyle = if emphasized {
        ui.emphasized(role)
    } else {
        ui.style(role)
    };

    iced::widget::text(content)
        .size(style.size_px)
        .line_height(LineHeight::Absolute(Pixels(style.line_height_px)))
        .font(ui.font_of(style))
        .color(color)
        .into()
}

/// 同 [`text_role`]，但**文字自身居中**（空态的标题与副文案：图标块是居中锚点）。
pub fn text_role_centered<'a>(
    ui: &'a Ui,
    role: TypeRole,
    emphasized: bool,
    content: &'a str,
    color: Color,
) -> Element<'a, Message> {
    text_widget(ui, role, emphasized, content, color)
        .align_x(iced::widget::text::Alignment::Center)
        .into()
}

fn text_widget<'a>(
    ui: &'a Ui,
    role: TypeRole,
    emphasized: bool,
    content: &'a str,
    color: Color,
) -> iced::widget::Text<'a> {
    let style: TypeStyle = if emphasized {
        ui.emphasized(role)
    } else {
        ui.style(role)
    };

    iced::widget::text(content)
        .size(style.size_px)
        .line_height(LineHeight::Absolute(Pixels(style.line_height_px)))
        .font(ui.font_of(style))
        .color(color)
}

/// 任意尺寸的令牌图标（`icons::RENDER_PX` 只有 24 一档；空态图标要 40）。
///
/// ## 图标语汇（视觉复审 P2-3）
///
/// 令牌里 **`Play` / `Stop` / `More` 是填充字形**，其余全是描边。页面骨架里**只用描边图标**：
/// 同屏把填充字形和描边字形混在一起，线宽语汇会打架（复审的原话是「Play 是填充字形，
/// 同屏其它图标是描边」）。真要做「主操作允许实心图标」这条例外，应当由组件线在
/// `components/` 里统一定义并只保留一处，而不是散落在页面里——所以这里干脆一个都不放。
/// 守门：`tests/no_hardcoded_tokens.rs` 的 `pages_use_only_stroked_icons`。
///
/// 描边宽度仍按令牌的 `Icon::stroke_width(render_px)` 折算，所以视觉线宽在任何尺寸下都是 1.5。
pub fn glyph(kind: IconKind, color: Color, render_px: f32) -> Element<'static, Message> {
    let icon = kind.icon();
    let paint = match icon.paint {
        IconPaint::Stroke => format!(
            "fill=\"none\" stroke=\"currentColor\" stroke-width=\"{}\" \
             stroke-linecap=\"round\" stroke-linejoin=\"round\"",
            icon.stroke_width(render_px)
        ),
        IconPaint::Fill => "fill=\"currentColor\" stroke=\"none\"".to_owned(),
    };

    let source = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{0}\" height=\"{0}\" \
         viewBox=\"0 0 24 24\"><path d=\"{1}\" {paint}/></svg>",
        render_px, icon.path
    );

    svg(svg::Handle::from_memory(source.into_bytes()))
        .width(render_px)
        .height(render_px)
        .style(move |_theme, _status| svg::Style { color: Some(color) })
        .into()
}

/// 固定宽 × 固定高的图标容器（Q9：按钮给的 Fill 约束会撑大字形，必须中间夹一层）。
pub fn glyph_box(kind: IconKind, color: Color, px: f32) -> Element<'static, Message> {
    container(glyph(kind, color, px))
        .width(px)
        .height(px)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
}

/// 竖直留白（布局拼装用；值来自令牌栅格）。
pub fn gap<'a>(px: f32) -> Element<'a, Message> {
    space::vertical().height(px).into()
}

/// 纯色容器样式（页面底色 / 留白带）。
pub fn filled(color: Color) -> container::Style {
    container::Style {
        background: Some(Background::Color(color)),
        ..container::Style::default()
    }
}

/// 滚动条配色（令牌角色）：轨道透明、滑块 `OutlineVariant`（低对比，弱于正文），
/// 轨宽 `SCROLLBAR_WIDTH`（6px）+ `SCROLLBAR_MARGIN`（2px 内缩）——视觉 r1 P2-2 的修法。
///
/// 只在**独立滚动区**上挂（实例页列表列），不许挂到整页上——整页滚动会把页头一起滚走。
pub fn scroll_style(ui: &Ui) -> impl Fn(&IcedTheme, scrollable::Status) -> scrollable::Style + '_ {
    let rail = scrollable::Rail {
        background: None,
        border: Border::default(),
        scroller: scrollable::Scroller {
            // 低对比：用 `OutlineVariant`（比 `Outline` 弱一档的容器描边角色）而不是 `Outline`，
            // 让滑块明显弱于正文；纯色不透明，CPU/GPU 两档一致。
            background: Background::Color(ui.color(Role::OutlineVariant)),
            border: Border {
                radius: ui.radius(ShapeScale::Full, SCROLLBAR_WIDTH),
                ..Border::default()
            },
        },
    };

    move |_theme, _status| scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
        auto_scroll: scrollable::AutoScroll {
            background: Background::Color(ui.color(Role::SurfaceContainerHigh)),
            border: Border::default(),
            shadow: Shadow::default(),
            icon: ui.color(Role::OnSurfaceVariant),
        },
    }
}

/// `crate::icons::RENDER_PX` 的再导出（页面里 24px 图标走这个常量，不写字面量）。
pub const ICON_PX: f32 = icons::RENDER_PX;

/// 小号图标（行尾 chevron 之类的辅助指示，16px）。
pub const ICON_SMALL_PX: f32 = 16.0;
