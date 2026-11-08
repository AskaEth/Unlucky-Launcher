//! 会话里的两条**横幅**（M119 从 `conversation.rs` 拆出来守 400 行硬线）：
//! 失败态（具名错误）与写操作确认。
//!
//! ## 为什么必须存在
//!
//! 任务书第 4 条：「presenter 返回错误时 UI 必须显示**具名**错误而不是静默无语」。M104 的壳没有
//! 任何错误位——`Submit` 只在本地数组追加一句「没发给模型」的提示。现在两条来源都有了落点：
//!
//! | 来源 | 触发 | 画什么 |
//! |---|---|---|
//! | `AgentHost::last_failure` | `agent_submit` 返回 `Err`（端点不可达 / 路由未实现） | 标题 = [`Key::AgentErrorTitle`]，正文 = 原因码对应的 catalog 译文 |
//! | `AgentRail::error` | 后端发 `AgentEvent::Failed`（这一轮回复中止） | 标题同上，正文 = presenter 已脱敏的那句原因 |
//!
//! 两者都不把后端诊断原文直接上屏：诊断串只进日志（M114 复审 r1 立的口径）。
//!
//! ## 确认条
//!
//! `rail.confirm` 非空 ⇒ 一轮写操作在等用户裁决，而 `busy` 会一直锁着输入（presenter 的
//! `sync_busy` 语义：`busy = streaming || confirm.is_some()`）。所以这段 UI 不是装饰：
//! **不按就不动**。两个按钮都带文字标签（M104 的「不得只有图标」同一条规矩）。

use iced::widget::{column, container, row, Row};
use iced::{Alignment, Background, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Catalog, Key};

use unlucky_presenter::dto::ConfirmRequest;

use crate::app::components::buttons::{self, Button};
use crate::theme::Ui;

use super::super::state::Focus;
use super::super::{Failure, Message};
use super::focused_state;
use super::text_role_owned;

/// 失败横幅：`ErrorContainer` 图标块 + 标题/正文（M3 内联 alert 的形状，与账户页横幅同源）。
pub(super) fn failure<'a>(
    ui: &'a Ui,
    failure: Failure<'a>,
    catalog: Catalog,
) -> Element<'a, Message> {
    banner(
        ui,
        catalog.text(failure.title).to_string(),
        failure.body.to_string(),
        Vec::new(),
    )
}

/// 写操作确认条：一行请求摘要 + Allow / Deny。
pub(super) fn confirm<'a>(
    ui: &'a Ui,
    request: &'a ConfirmRequest,
    catalog: Catalog,
    focus: Option<Focus>,
) -> Element<'a, Message> {
    // 摘要来自 presenter（已过 `scrub_text`）：它是「要改什么」的唯一描述，必须显示，
    // 否则用户是在盲签。句框走 catalog，摘要本身是正文。
    let title = catalog.render(
        Key::AgentConfirmRequest,
        &[("summary", request.summary.as_str())],
    );
    // 两个按钮都在 Tab 环上（`Focus::ConfirmAllow` / `Focus::ConfirmDeny`）：键盘用户
    // 用 Enter/Space 就能裁决，不必去找鼠标（P2-2 的那条死路）
    let deny = focus == Some(Focus::ConfirmDeny);
    let allow = focus == Some(Focus::ConfirmAllow);
    let actions: Vec<Element<'a, Message>> = vec![
        ring(
            ui,
            buttons::outlined(
                ui,
                focused_state(
                    Button::new(catalog.text(Key::AgentConfirmDeny), Message::Confirm(false))
                        .leading(IconKind::Close),
                    deny,
                ),
            ),
            deny,
        ),
        ring(
            ui,
            buttons::filled(
                ui,
                focused_state(
                    Button::new(catalog.text(Key::AgentConfirmAllow), Message::Confirm(true))
                        .leading(IconKind::Check),
                    allow,
                ),
            ),
            allow,
        ),
    ];
    banner(ui, title, String::new(), actions)
}

/// 焦点环：`Primary` 的 2dp 描边（与输入框聚焦同一套语言）。
///
/// 为什么不能只用 `Interaction::Focused`（10% 状态层）：在**实心主色**按钮上那层几乎看不出来
/// ——M119 r1 复审要求「焦点确实落在可操作元素上」且要能出图证明，10% 层在浅色图里等于没有。
/// 所以键盘焦点在这两个按钮上额外加一圈描边（鼠标用户的界面不受影响：只有 `focused` 才画）。
fn ring<'a>(ui: &'a Ui, button: Element<'a, Message>, focused: bool) -> Element<'a, Message> {
    // 外圈留 3px 空气：环与按钮之间不贴边，才看得成「一圈」而不是「两条重合的描边」。
    // 两个按钮**都**加这层 padding（`focused` 与否一样），所以焦点变化不移动布局。
    let gap = ui.space(Space::X1) * 0.75;
    let outer = ui.metrics().control_height + gap * 2.0;
    container(button)
        .padding(gap)
        .style(move |_theme| container::Style {
            border: if focused {
                iced::Border {
                    color: ui.color(Role::Primary),
                    width: crate::theme::STROKE_PX * 2.0,
                    // 与按钮同一种「药丸」：`Full` 的半径 = 边长 / 2（这里用加过 padding 的边长）
                    radius: ui.radius(ShapeScale::Full, outer),
                }
            } else {
                iced::Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

/// 横幅本体：图标块 + 标题/正文 + 右侧动作。
///
/// 形状与 `pages::account::widgets::alert` 同源（面板底走卡片层 + 1dp 描边，
/// `ErrorContainer` / `OnErrorContainer` 只作左侧图标块的强调，不是通栏红条）。
fn banner<'a>(
    ui: &'a Ui,
    title: String,
    body: String,
    actions: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    let side = ui.space(Space::X8);
    let glyph_px = ui.space(Space::X6);
    let chip = container(crate::icons::view_at(
        IconKind::Error,
        ui.color(Role::Error),
        glyph_px,
    ))
    .width(Length::Fixed(side))
    .height(Length::Fixed(side))
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_theme| container::Style {
        background: Some(Background::Color(ui.color(Role::ErrorContainer))),
        border: iced::Border {
            radius: ui.radius(ShapeScale::Full, side),
            ..iced::Border::default()
        },
        ..container::Style::default()
    });

    let mut lines = column![text_role_owned(
        ui,
        TypeRole::TitleSmall,
        true,
        title,
        ui.color(Role::OnSurface),
        Length::Fill,
    )]
    .spacing(ui.space(Space::X1));
    if !body.is_empty() {
        lines = lines.push(text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            body,
            ui.color(Role::OnSurfaceVariant),
            Length::Fill,
        ));
    }

    // 文字行与动作**分行**：右栗只有 280~360，chip + 两个按钮已占掉大半宽，
    // 把文字挤在同一条 Row 里会被压成一条竖缝（M119 实测：320 宽下标题整段看不见）。
    let head = Row::new()
        .spacing(ui.space(Space::X3))
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .push(chip)
        .push(container(lines).width(Length::Fill));

    let mut content = column![Element::from(head)].spacing(ui.space(Space::X2));
    if !actions.is_empty() {
        content = content.push(
            container(
                row(actions)
                    .spacing(ui.space(Space::X2))
                    .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .align_x(Alignment::End),
        );
    }

    container(content)
        .width(Length::Fill)
        .padding(ui.metrics().card_padding)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(ui.layer(MaterialLayer::Card))),
            border: ui.outlined(ShapeScale::Medium),
            ..container::Style::default()
        })
        .into()
}
