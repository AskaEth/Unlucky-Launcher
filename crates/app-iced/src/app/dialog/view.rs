//! 模态层的控件树：**遮罩 + 卡片 + 主/次按钮**，正文与路径全部来自 [`super::Modal`]。
//!
//! ## 命中语义（为什么点遮罩能关、点卡片不关）
//!
//! `iced::widget::Stack` 的 `update` 从上往下（后画的先收）派发，遇到 `shell.is_event_captured()`
//! 立刻停。所以三层的顺序是：
//!
//! ```text
//! stack![ 页面树, 遮罩（mouse_area→Dismiss，整窗）, 居中容器（透明）→ opaque(卡片) ]
//! ```
//!
//! 卡片那层用 `opaque` 把卡片内的左键按下**吃掉**（[`super::Message`] 不需要再有一条专用
//! 消息），于是点卡片里的空白不会冒到遮罩去关框；卡片里的按钮自己会先捕获。
//! **`opaque` 只包卡片**是这条语义成立的前提：包住整窗居中容器的话，命中区就变成整窗，
//! 遮罩永远收不到按下（r1 复审 P2-1 抓到的原实现）。

use iced::widget::text::LineHeight;
use iced::widget::{button, column, container, mouse_area, row, scrollable, text, Space};
use iced::{Alignment, Background, Border, Color, Element, Font, Length, Pixels, Shadow};

use unlucky_design::color::Role;
use unlucky_design::component_map::Component;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::spacing::Space as Sp;
use unlucky_design::typography::TypeRole;

use crate::app::components::buttons;
use crate::app::components::feedback::dialog::scrim;
use crate::app::components::feedback::panel::DialogGeometry;
use crate::pages::frame;
use crate::theme::Ui;

use super::super::message::Message;
use super::{Message as DialogMessage, Modal};

/// 卡片内**滚动区**高度（详情 + 路径在这里滚；页脚按钮永远在卡片里）。
const SCROLL_HEIGHT: f32 = 300.0;
/// 有导出结论时滚动区让出的高度（那一段会多占两行，不让就只能压扁页脚按钮）。
const SCROLL_HEIGHT_WITH_NOTICE: f32 = 210.0;
/// 卡片最大高（兜底：不许把边框撑出屏幕）。
const CARD_HEIGHT: f32 = 660.0;

/// 把模态层叠在 `base` 上（没有弹窗时由调用方原样返回 `base`，这里是纯叠加）。
///
/// ## 三层的命中语义（r1 复审 P2-1 的修法）
///
/// ```text
/// stack![ 页面树, 遮罩(mouse_area→Dismiss, 整窗), 居中容器(透明) → opaque(卡片) ]
/// ```
///
/// 关键在于**卡片那层只有卡片那么大**：`opaque(卡片)` 的命中区就是卡片本身，点卡片里
/// 只会被它吃掉（`shell.capture_event`），点卡片外则**继续往下走**到遮罩 → `Dismiss`。
/// 反例（r1 抓到的原实现）：`mouse_area(整窗居中容器).on_press(Swallow)` —— 整窗都在它
/// 的 bounds 里，任何一次卡片外点击都先被它捕获，遮罩永远收不到，于是「点遮罩关闭」
/// 只存在于注释里。
pub(crate) fn layer<'a>(
    ui: &'a Ui,
    base: Element<'a, Message>,
    modal: &'a Modal,
) -> Element<'a, Message> {
    let backdrop = mouse_area(
        container(Space::new())
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_theme| container::Style {
                background: Some(Background::Color(backdrop_color(ui))),
                ..container::Style::default()
            }),
    )
    .on_press(Message::Dialog(DialogMessage::Dismiss));

    // 透明的一层只负责居中；会吃点击的 `opaque` 只包卡片。
    let centered = container(card_plane(ui, modal))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .padding(ui.space(Sp::X6));

    iced::widget::stack![base, backdrop, centered].into()
}

/// 遮罩色：**组件层那一份不透明预合成**（黑 @0.32 叠在画布上）。
///
/// 为什么不自己拿 `Role::OnSurface` 加 α：它在深色档是 **tone 90（近白）**，32% 铺上去会把
/// 整窗**提亮**（r1 复审实测 (9,21,28) → (129,138,145)）；而且 α 混色在 wgpu/tiny-skia 两档
/// 合成口径不同（linear vs sRGB），D27 要求 CPU 档零半透明。组件层 [`scrim`] 两件事都解决了。
///
/// 单独成函数是为了让 [`super::tests`] 断言**视图真正用的那一份**（P1 的守卫）。
pub(super) fn backdrop_color(ui: &Ui) -> Color {
    scrim(ui)
}

/// 卡片层：`opaque` 让「卡片内的点击」被吃掉，不再冒到遮罩（点遮罩才关）。
///
/// 可见性放宽到 `pub(super)` 只为让 [`super::tests`] 能断言它的 `size_hint`（P2-1 的守卫）。
pub(super) fn card_plane<'a>(ui: &'a Ui, modal: &'a Modal) -> Element<'a, Message> {
    iced::widget::opaque(card(ui, modal))
}

/// 卡片本体（**尺寸 = 内容**，不声明 `Fill` —— 见 [`layer`] 的命中语义）。
fn card<'a>(ui: &'a Ui, modal: &'a Modal) -> Element<'a, Message> {
    // 几何全部走组件层（r1 复审 P1 的纪律项：别绕过现成的 Dialog 档）。
    let geometry = DialogGeometry::of(ui);
    let shape = Component::Dialog.spec().shape;

    // 头（标题 + 三句正文）与脚（按钮）**固定**，中间那段滚——按钮滚出屏幕等于功能不存在
    // （视觉复审在日志页踩过同一坑：诊断卡把摘要页脚顶出去了）。
    let mut header = column![].spacing(ui.space(Sp::X2)).width(Length::Fill);
    header = header.push(role(
        ui,
        TypeRole::HeadlineSmall,
        true,
        modal.title.as_str(),
    ));
    for line in &modal.body {
        header = header.push(wrapped(ui, line.as_str(), ui.color(Role::OnSurface)));
    }

    let mut middle = column![].spacing(ui.space(Sp::X2)).width(Length::Fill);
    if let Some(card) = &modal.card {
        // **详细说明在前、路径在后**：用户先要知道「发生了什么」，再要「文件在哪」。
        if card.expanded {
            if let Some(title) = &card.detail_title {
                middle = middle.push(role(ui, TypeRole::LabelMedium, false, title.as_str()));
            }
            for line in &card.detail_lines {
                middle = middle.push(mono(ui, line.clone()));
            }
            middle = middle.push(frame::divider(ui).map(Message::Page));
        }
        for (index, (label, path)) in card.paths.iter().enumerate() {
            middle = middle.push(
                button(
                    column![
                        role(ui, TypeRole::LabelMedium, false, label.as_str()),
                        mono(ui, path.clone()),
                    ]
                    .spacing(0.0)
                    .width(Length::Fill),
                )
                .width(Length::Fill)
                .padding(0)
                .style(|_theme, _status| button::Style {
                    background: None,
                    text_color: Color::TRANSPARENT,
                    border: Border::default(),
                    shadow: Shadow::default(),
                    snap: true,
                })
                .on_press(Message::Dialog(DialogMessage::CopyPath(index))),
            );
        }
    }

    let mut footer = column![].spacing(ui.space(Sp::X1)).width(Length::Fill);
    if let Some(notice) = &modal.notice {
        footer = footer.push(wrapped(ui, notice.as_str(), ui.color(Role::Primary)));
    }
    footer = footer.push(actions(ui, modal));

    // 中间那段给**定高**预算：`Shrink` 会让滚动区按内容高排版，把页脚顶到卡片外面
    // （实测第一版就是这样：导出/关闭两个按钮整排不可见）。
    let body = if modal.card.is_some() {
        column![
            header,
            scrollable(middle).height(Length::Fixed(if modal.notice.is_some() {
                SCROLL_HEIGHT_WITH_NOTICE
            } else {
                SCROLL_HEIGHT
            })),
            frame::divider(ui).map(Message::Page),
            footer,
        ]
    } else {
        column![header, frame::divider(ui).map(Message::Page), footer,]
    }
    .spacing(ui.space(Sp::X3))
    .width(Length::Fill);

    // 宽度取官方**上限** 560（`DialogGeometry::max_width`）而不是本项目默认 480：
    // 崩溃卡片多两行路径 + 三枚辅助按钮，480 会把主按钮压成竖排（实测过）。
    // 边界：组件层日后若改宽度，这里不会跟（已记 P3 台账）。
    container(body)
        .width(Length::Fixed(geometry.max_width))
        .max_height(CARD_HEIGHT)
        .padding(geometry.padding)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(ui.layer(MaterialLayer::Overlay))),
            border: ui.outlined(shape),
            ..container::Style::default()
        })
        .into()
}

/// 底部按钮区：**两行**——辅助操作（开文件夹 / 复制路径 / 展开）在上，主次按钮在下靠右。
///
/// 为什么分两行：五个按钮塞一行在 560 的卡宽里会把主按钮压成竖排（实测第一版就是这样，
/// 「导出诊断包」被挤成两行单字）。
fn actions<'a>(ui: &'a Ui, modal: &'a Modal) -> Element<'a, Message> {
    let mut column_actions = column![].spacing(ui.space(Sp::X2)).width(Length::Fill);

    if let Some(card) = &modal.card {
        column_actions = column_actions.push(
            row![
                buttons::outlined(
                    ui,
                    buttons::Button::new(
                        card.actions[1].as_str(),
                        Message::Dialog(DialogMessage::OpenFolder),
                    ),
                ),
                buttons::outlined(
                    ui,
                    buttons::Button::new(
                        card.actions[2].as_str(),
                        Message::Dialog(DialogMessage::CopyPaths),
                    ),
                ),
                buttons::text(
                    ui,
                    buttons::Button::new(
                        card.toggle.as_str(),
                        Message::Dialog(DialogMessage::Toggle),
                    ),
                ),
            ]
            .spacing(ui.space(Sp::X2))
            .align_y(Alignment::Center),
        );
    }

    let mut line = row![Space::new().width(Length::Fill)].spacing(ui.space(Sp::X2));
    if let Some(secondary) = &modal.secondary {
        line = line.push(buttons::text(
            ui,
            buttons::Button::new(
                secondary.as_str(),
                Message::Dialog(DialogMessage::Secondary),
            ),
        ));
    }
    if !modal.blocking_progress {
        line = line.push(buttons::filled(
            ui,
            buttons::Button::new(
                modal.primary.as_str(),
                Message::Dialog(DialogMessage::Primary),
            ),
        ));
    }
    column_actions.push(line.align_y(Alignment::Center)).into()
}

/// 令牌排版档的文字（`pages::frame` 的那一套，映射成本层的消息类型）。
fn role<'a>(
    ui: &'a Ui,
    role: TypeRole,
    emphasized: bool,
    content: &'a str,
) -> Element<'a, Message> {
    frame::text_role(
        ui,
        role,
        emphasized,
        content,
        ui.color(Role::OnSurfaceVariant),
    )
    .map(Message::Page)
}

/// 等宽文字（路径 / 堆栈 / 捕获输出——这些必须能一眼看出字符边界）。
fn mono<'a>(ui: &'a Ui, content: String) -> Element<'a, Message> {
    let style = ui.style(TypeRole::BodySmall);
    text(content)
        .size(style.size_px)
        .line_height(LineHeight::Absolute(Pixels(style.line_height_px)))
        .font(Font {
            family: iced::font::Family::Name(ui.families().mono),
            ..Font::DEFAULT
        })
        .color(ui.color(Role::OnSurface))
        .width(Length::Fill)
        .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
        .into()
}

/// 正文一段：令牌排版档 + 按词折行（长路径只按词折会把卡片顶破）。
fn wrapped<'a>(ui: &'a Ui, content: &'a str, color: Color) -> Element<'a, Message> {
    let style = ui.style(TypeRole::BodyMedium);
    text(content)
        .size(style.size_px)
        .line_height(LineHeight::Absolute(Pixels(style.line_height_px)))
        .font(ui.font_of(style))
        .color(color)
        .width(Length::Fill)
        .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
        .into()
}
