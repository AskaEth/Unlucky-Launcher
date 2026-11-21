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
//!
//! ## 卡片边界（M444 → M449 分两条路）
//!
//! 改前这层 `Border` 是 `ui.outlined(shape)`：1px `OutlineVariant` 四边。M444 起走组件库**唯一**
//! 那份边界实现 `panel_border`（`components/rim.rs`），档位 **S4 Modal**（`11b` §1.0：对话框 /
//! 确认框）；M449 起**先看面板实际色**再决定画不画边（判断全在 `border::card_border`）：
//!
//! | 面板实际 α | 边是什么 | 实测·卡片上沿 (590,556) |
//! |---|---|---|
//! | 1.0（出厂 / CPU 档）· GPU | S4 rim：四边 1px，亮档黑 α0.08 / 暗档白 α0.14，**预合成到卡片底** | 浅 190,200,205 → **210,210,211**；深 60,74,61 → **61,66,61** |
//! | 1.0 · CPU | §1.6 降级成 1px `OutlineVariant`——与改前那一句**逐位相同** | 浅 / 深**边框带 0 个像素变** |
//! | **< 1.0**（壁纸模式的 GPU 档） | **不预合成、不画边**（只留圆角）：预合成的「底」不再等于实际绘制底 | 改前是一条算错 22 级的边：浅档比面板**亮** 6 级、深档是 39 级亮带，见 `DevDocs/artifacts/m449/` |
//!
//! **是替换，不是叠加**：`container::Style` 只有**一个** `border` 字段，两圈边挂不到同一层上；真要两圈得
//! 把卡片再包一层容器并给它 1px 内距，那会多出 2px 尺寸和一条 2px 双色带，违反 §1.2 的「1px 单圈」。
//! 所以同一个 1px 带换材质：GPU 给 rim，CPU 给 `OutlineVariant`，半透明面板给「没有边」。
//!
//! 代价写在明处：面板不透明时卡片还是**实色**（玻璃 tint α 由 D61 定案不做），于是浅档 GPU 这圈边比
//! 改前淡约 20 级（190 → 210，离卡片底 228 更近），深档基本不变（0,−8,0）。rim 本来的角色是「半透明
//! 玻璃面上的高光」，收益要等 tint 落地；这一步先把**取值**接对（改前那圈 `OutlineVariant` 是写死的，
//! 与渲染档无关）。取证（before/after × 浅/深 × CPU/GPU）见 `DevDocs/artifacts/m444/`，M449 的两档见
//! `DevDocs/artifacts/m449/`。

use iced::widget::text::LineHeight;
use iced::widget::{button, column, container, mouse_area, scrollable, text, Space};
use iced::{Alignment, Background, Border, Color, Element, Font, Length, Pixels, Shadow};

use unlucky_design::color::Role;
use unlucky_design::component_map::Component;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::spacing::Space as Sp;
use unlucky_design::typography::TypeRole;

use crate::app::components::feedback::dialog::scrim;
use crate::app::components::feedback::panel::DialogGeometry;
use crate::app::components::motion;
use crate::pages::frame;
use crate::theme::Ui;

use super::super::message::Message;
use super::{Message as DialogMessage, Modal};

mod actions;
mod border;

use border::card_border;

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
    // M256：模态进出只由**一个**进度驱动——遮罩做画布色→scrim 色的**预合成**插值（不透明），
    // 卡片做 0.96→1.0 的缩放。`progress = 1` 时两者都退化成改造前的原值（出图基线不变）。
    let progress = motion::modal().progress;

    let backdrop = mouse_area(
        container(Space::new())
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_theme| container::Style {
                background: Some(Background::Color(backdrop_color_at(ui, progress))),
                ..container::Style::default()
            }),
    )
    .on_press(Message::Dialog(DialogMessage::Dismiss));

    // 透明的一层只负责居中；会吃点击的 `opaque` 只包卡片。
    let centered = container(motion::scaled(
        card_plane(ui, modal),
        motion::modal_scale(progress),
    ))
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .padding(ui.space(Sp::X6));

    iced::widget::stack![base, backdrop, centered].into()
}

/// 遮罩色（静息 = 完全展开）：**组件层那一份不透明预合成**（黑 @0.32 叠在画布上）。
///
/// 不自己拿 `Role::OnSurface` 加 α：它在深色档是 **tone 90（近白）**，32% 铺上去会把整窗
/// **提亮**（r1 实测 (9,21,28) → (129,138,145)）；且 α 混色在 wgpu/tiny-skia 两档合成口径不同
/// （linear vs sRGB），D27 要求 CPU 档零半透明。组件层 [`scrim`] 两件事都解决了。
///
/// 只在测试构建里存在：让 [`super::tests`] 断言**视图真正用的那一份**；产品路径一律走
/// [`backdrop_color_at`]（`progress = 1` 那一端逐位相同）。
#[cfg(test)]
pub(super) fn backdrop_color(ui: &Ui) -> Color {
    backdrop_color_at(ui, 1.0)
}

/// 遮罩色在**动画进度** `progress` 上的取值（M256）：画布色 → scrim 色的预合成插值。
///
/// **为什么是"插值颜色"而不是"调 α"**：D27 禁 CPU 渲染路径上的面板级半透明（D36 只对**窗口级**
/// 统一 α 开过口子，面板 α 仍强制 1.0）。所以「淡入」不能靠 `Color { a: 0.1 }`，只能把**已经
/// 合成好的不透明色从画布色插值到 scrim 色，全程 `a = 1.0`，两个后端出图一致。
/// `1` 那一端必须**逐位等于**静息遮罩色，否则出图基线会漂。
///
/// ⚠️ **口径（复审 P2-3 订正）**：`0` 时这一层是**整块画布色的不透明矩形**，它把页面内容
/// **整块遮住**（不是"透明所以看不见"，也不是"看得到页面"）——D27 禁面板 α，所以只有这一种
/// 可行实现。观感是「画面先被画布色盖住、随进度变暗到 scrim」，不是「页面透过淡遮罩隐隐可见」。
pub(super) fn backdrop_color_at(ui: &Ui, progress: f32) -> Color {
    let t = progress.clamp(0.0, 1.0);
    if t >= 1.0 {
        return scrim(ui);
    }
    let from = ui.layer(MaterialLayer::Canvas);
    let to = scrim(ui);
    Color {
        r: from.r + (to.r - from.r) * t,
        g: from.g + (to.g - from.g) * t,
        b: from.b + (to.b - from.b) * t,
        // 预合成色两端都是不透明的：这里显式钉死 1.0，不给"淡入=半透明"留口子。
        a: 1.0,
    }
}

/// 卡片层：`opaque` 让「卡片内的点击」被吃掉，不再冒到遮罩（点遮罩才关）。
///
/// 可见性放宽到 `pub(super)` 只为让 `app/tests.rs` 的用例能断言它的 `size_hint`（P2-1 的守卫）。
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

    let mut foot = column![].spacing(ui.space(Sp::X1)).width(Length::Fill);
    if let Some(notice) = &modal.notice {
        foot = foot.push(wrapped(ui, notice.as_str(), ui.color(Role::Primary)));
    }
    foot = foot.push(actions::footer(ui, modal));

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
            foot,
        ]
    } else {
        column![header, frame::divider(ui).map(Message::Page), foot,]
    }
    .spacing(ui.space(Sp::X3))
    .width(Length::Fill);

    // 宽度取官方**上限** 560（`DialogGeometry::max_width`）而不是本项目默认 480：
    // 崩溃卡片多两行路径 + 三枚辅助按钮，480 会把主按钮压成竖排（实测过）。
    // 边界：组件层日后若改宽度，这里不会跟（已记 P3 台账）。
    //
    // 边界（M444 → M449）：预合成的「底」必须**是屏幕上那块底**，所以这里把面板实际色
    // （`layer()` 那一份，可能带面板 α）交给 `card_border`，由它决定画不画边——
    // 判断、误差公式与取证都在 `border.rs`，本文件只管把真值递过去。
    let panel = ui.layer(MaterialLayer::Overlay);
    let border = card_border(ui, panel, shape);
    container(body)
        .width(Length::Fixed(geometry.max_width))
        .max_height(CARD_HEIGHT)
        .padding(geometry.padding)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(panel)),
            border,
            ..container::Style::default()
        })
        .into()
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

#[cfg(test)]
mod motion_tests;
