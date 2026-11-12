//! **ELEVATED CARD**（规格卡 `CARD / DIALOG / SNACKBAR / LIST` 的第一个方框）。
//!
//! ## 规格卡原值（`DevDocs/artifacts/components-ink.svg`，`[spec]`）
//!
//! | 项 | 值 | 出处 |
//! |---|---|---|
//! | 方框 | 300×150、**rx=16**（`inkcat::metrics::CARD`） | `rect x=64 y=1312 rx=16` |
//! | 底色 | **`SurfaceContainer`**（Ink 板上比画布 `Surface` 亮一档） | 规格卡卡底那一格的 `fill` |
//! | 内边距 | **24**（文字 `x=88` − 框 `x=64`） | 同上 |
//! | 描边 | **无**（`stroke="none"`） | 同上 |
//! | 上标注 | **11 / 600**（`LabelSmall` + 半粗，字距 1） | `ELEVATED CARD` 那行 |
//! | 标题 | **16 / 400**（`BodyLarge`） | `Night bridge, bright rails.` 那行 |
//! | 副文案 | 13 / 400 → **14**（`BodyMedium`；13 落在 12/14 之间，按 `inkcat::type_scale` 取较大档） | `night / bridge / cat` 那行 |
//!
//! ## 与 `pages::frame::card`（描边卡）的分工
//!
//! - [`card`]：**靠层级差站立**的卡（无描边、`SurfaceContainer` 底、16 圆角）。规格卡的
//!   `ELEVATED CARD` 就是这一档，实例页的元信息卡用它；
//! - `pages::frame::card`：描边卡（`OutlineVariant` 1px + 卡片层底），大面积表单容器用它。
//!
//! 两种卡并存是**有意的**：规格卡只画了 elevated 一种，而桌面端的长表单需要「弱一点的容器」
//! （满屏都是 16 圆角 + 容器底，分隔线会看起来像浮块）。哪种卡用在哪儿由本节决定，不是随手挑。

use iced::widget::text::LineHeight;
use iced::widget::{column, container, text};
use iced::{Background, Border, Element, Length, Padding, Pixels};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::{TypeRole, TypeStyle, Weight};

use crate::theme::Ui;

use super::super::interaction::Base;

/// 卡片的底（**这个容器画什么，子控件的禁用态就叠什么**）。
///
/// 卡内控件的禁用色必须拿这个常量，不许自己写 `Role::Surface`：M234 之前 8 个调用点全传
/// 画布色，卡片里的禁用按钮于是把 10% 叠在画布上（M223 的「CR 1.05、形丢失」）。
/// 与 [`card`] 的 `background` 用同一个 `Role`，改色不会漂。
pub const BASE: Base = Base::of(Role::SurfaceContainer);

/// ELEVATED CARD 的内边距：**20**（= `LayoutMetrics::card_padding`）。
///
/// 为什么不是规格卡量到的 **24**（卡片文字 `x=88` − 框 `x=64`）：本组件唯一的消费者是实例页的
/// 元信息卡，外框是 `META_CARD_MIN_WIDTH`（160），卡内必须放下最长的值串（**12px 下实测 118px**；
/// 同一个串在 14px 下是 138px，见 [`TITLE_ROLE`]）。
/// 24 ⇒ 内宽 112 ⇒ 值串折行 ⇒ 同排卡底不齐（M229 复审 P1-2 的现场）；20 ⇒ 内宽 120 ≥ 118，
/// 且**不动卡片宽度契约**（`pages/layout.rs` 的 160 一个字没改）。
///
/// 登记为 **Structural** 偏离：规格卡的 24 是 300×150 展示卡的数，密集网格用 20；
/// 将来真做展示卡就再开一档（现在的 `PAD` 名字对应的是密集卡，不是规格卡原值）。
pub const PAD: f32 = 20.0;

/// 值的字级：`BodySmall`（12/16）。
///
/// 展示卡的内距与字号（24 + `BodyLarge` 16）在 160 外框里放不下值串；14 档也不够：实测最长的
/// 元信息值（时戳 `2026-10-29 19:22 UTC`）在 14px 下墨水宽 **138px**，而 160 − 2×20 = 120。
/// 12px 下它就是 118px 级 ⇒ 放得下（余量见 `pages/instances/tests.rs` 的断言）。
/// 「内距」与「字级」是一对：只改一个就会重新折行。
pub const TITLE_ROLE: TypeRole = TypeRole::BodySmall;

/// 一个 ELEVATED CARD 方框（只管壳：底、圆角、内边距；**无描边**）。
///
/// 消息类型与被包的内容同型，所以它能被任何页面复用（实例页 / 外观页 / 账户页各用自己的
/// `Message`）——这是「组件不 import 业务消息」那条契约的落地方式。
pub fn card<'a, M: 'a>(ui: &'a Ui, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    let shape = ui.card_shape();
    container(content)
        .width(Length::Fill)
        .padding(Padding::from([PAD, PAD]))
        .style(move |_theme| container::Style {
            // 规格卡的卡底是 **surface-container**（不是 `surface-container-low`）：
            // 层级靠与画布（`surface`）的 tone 差表达，不靠描边。
            // 子控件的禁用态拿同一个角色（[`BASE`]）合成，两边不会漂。
            background: Some(Background::Color(ui.color(BASE.role()))),
            border: Border {
                radius: ui.radius_dp(shape),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

/// 一张规格卡的三段文字：上标注（可选）/ 标题 / 副文案（可选）。
#[derive(Debug, Clone, Copy)]
pub struct Texts<'a> {
    /// 上标注（`ELEVATED CARD` 那种小字标签）。
    pub overline: Option<&'a str>,
    /// 值 / 标题（字级是 [`TITLE_ROLE`] = **12/400**）。
    pub title: &'a str,
    /// 副文案（`BodyMedium` 14/400）。
    pub support: Option<&'a str>,
}

/// 三段式内容（上标注 11/600 + 值 [`TITLE_ROLE`] **12/400** + 副文案 14/400，行间 4）。
pub fn texts<'a, M: 'a>(ui: &'a Ui, parts: Texts<'a>) -> Element<'a, M> {
    let mut stack = column![].spacing(ui.space(Space::X1));

    if let Some(overline) = parts.overline {
        stack = stack.push(role_text(
            ui,
            overline_style(),
            overline,
            Role::OnSurfaceVariant,
        ));
    }
    stack = stack.push(role_text(
        ui,
        ui.style(TITLE_ROLE),
        parts.title,
        Role::OnSurface,
    ));
    if let Some(support) = parts.support {
        stack = stack.push(role_text(
            ui,
            ui.style(TypeRole::BodyMedium),
            support,
            Role::OnSurfaceVariant,
        ));
    }

    stack.width(Length::Fill).into()
}

/// 上标注的排版（`LabelSmall` 11/16，字重 **600**：规格卡 `font-weight=600`；
/// M3 的 `LabelSmall` 是 500，`emphasized` 是 700 —— 两个都不是 600，所以在这里显式给 600）。
pub fn overline_style() -> TypeStyle {
    TypeStyle {
        weight: Weight::SemiBold,
        ..TypeRole::LabelSmall.style()
    }
}

fn role_text<'a, M: 'a>(
    ui: &'a Ui,
    style: TypeStyle,
    content: &'a str,
    role: Role,
) -> Element<'a, M> {
    text(content)
        .size(style.size_px)
        .line_height(LineHeight::Absolute(Pixels(style.line_height_px)))
        .font(ui.font_of(style))
        .color(ui.color(role))
        .into()
}

#[cfg(test)]
mod tests;
