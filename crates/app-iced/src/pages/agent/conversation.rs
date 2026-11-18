//! **会话视图组件**：右栗面板与 `Route::Agent` 独立页的**同一份**对话 UI。
//!
//! ## 输入契约（纯数据入、消息出）
//!
//! ```text
//! Conversation { messages, busy, unread, confirm, failure, draft, blank, rail, host, focus }
//!        ──view()──▶  Element<Message>
//! ```
//!
//! - **入**：全部是**借用**的纯数据。会话三件（`messages` / `busy` / `unread` / `confirm`）
//!   直接从 presenter 的 `AgentRail` 借来（M119），组件自己不持有会话状态；`failure` 是 app 层
//!   备好的**展示输入**（标题是 catalog 键、正文是已脱敏串），见 `super::Failure`。
//! - **出**：控件树只发 [`super::Message`]（`Collapse` / `Detach` / `Dock` / `Submit` / `Stop` /
//!   `Confirm` / `Edit` / `Expand`），app 层再包一层 `Message::Agent(..)`。于是同一份视图可以被
//!   两个宿主摆在不同宽度里，而「点了发生什么」只有一处实现（`app/update/agent.rs`）。
//!
//! ## 两个宿主只差「外壳」
//!
//! | 宿主 | 谁调用 | 给的宽度 | 差别 |
//! |---|---|---|---|
//! | [`Host::Rail`] | [`super::docked`]（右栗面板） | 280~360（本组件吃 Fill） | 头部动作是 `⌄` 收起 + `⇥` 拆出 |
//! | [`Host::Page`] | [`super::page`]（独立页） | 行宽上限 424 | 头部动作是 `⇤` 收回；标题是页级标题 |
//!
//! **判据不是「长得像」而是「同一个函数」**：`tests::both_hosts_render_the_same_conversation`
//! 用源码扫描钉住「两个宿主都只通过 [`view`] 画对话」，`super::mod` 里不许出现第二份气泡/输入框装配。
//!
//! ## 控件与可达性（任务第 4 条逐项）
//!
//! | 要求 | 落点 |
//! |---|---|
//! | 输入框多行 | `text_editor`（真多行，不是 `text_input` 的单行） |
//! | Enter 发送 / Shift+Enter 换行 | [`composer`] 的 `key_binding`：`Binding::Custom(Submit)` vs `Binding::Enter`，用 `modifiers.shift()` 分流（**只有输入框聚焦时生效**，不会抢别处的 Enter） |
//! | 发送禁用态 | 草稿空白或流式中 → `Button::disabled`（M3 的 10% 容器 / 38% 内容，不是自创灰色） |
//! | 流式中断 | 只在 `busy` 时出现 `Stop`（`IconKind::Close` + 文字标签） |
//! | 未读点 | 折叠条上的圆点（[`super::strip`]）+ 面板头部的「N unread」胶囊 |
//! | 失败态 | [`banners::failure`]：具名标题（catalog）+ 已脱敏正文（M119） |
//! | 写操作确认 | [`banners::confirm`]：请求摘要 + Allow / Deny 两个文字按钮（M119） |
//! | `⌄` / `⇥` / `⇤` | 头部两个文字按钮，图标只是**前导**，标签一律可读 |
//! | 键盘 | `Esc` 收栗 / `Tab` 走焦点环 / `Enter` 激活焦点键（订阅在 `app/subscription.rs`） |

use iced::widget::text_editor;
use iced::widget::{column, container, row, rule, space, text};
use iced::{Background, Border, Element, Length, Padding, Theme};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Catalog, Key};

use unlucky_presenter::dto::{ChatMessage, ConfirmRequest};

use crate::app::components::buttons::{self, Button};
use crate::app::components::interaction::Interaction;
use crate::theme::Ui;

use super::state::{Focus, RailState};
use super::{Disclaimer, Failure, Message};

mod banners;
mod composer;
mod transcript;

pub(super) use composer::{text_role, text_role_owned};

/// 会话视图落在哪种外壳里（只影响头部动作与标题密度，**不影响**对话本体）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Host {
    /// 右侧栏（280~360）。
    Rail,
    /// 独立页（列宽 ≤424）。
    Page,
}

/// 会话视图的**输入契约**（见模块文档；全部是借用的纯数据）。
#[derive(Clone, Copy)]
pub struct Conversation<'a> {
    /// 会话内容（presenter 的 `AgentRail::messages`，零条 = 空会话，画引导空态）。
    pub messages: &'a [ChatMessage],
    /// 输入框内容（iced 的 `text_editor` 自己持有编辑状态）。
    pub draft: &'a text_editor::Content,
    /// 是否在流式回复 / 有待确认写操作（来自 rail 的 `busy`：决定 `Stop` 出现与 `Send` 禁用）。
    pub busy: bool,
    /// 未读数（>0 时头部画胶囊；来自 rail 的 `unread`）。
    pub unread: usize,
    /// 草稿是否空白（发送键的禁用判据，由 `AgentUi::draft_is_blank` 给）。
    pub blank: bool,
    /// 待确认的写操作（`Some` = 画确认条）。
    pub confirm: Option<&'a ConfirmRequest>,
    /// 失败横幅（`Some` = 画具名错误；见 [`Failure`]）。
    pub failure: Option<Failure<'a>>,
    /// 「这是模拟助手」的**四处文案**（副标题 / 占位符 / 空态正文 / 标题胶囊），
    /// 全部由 [`Disclaimer::of`] 一次算好——视图里没有第二种取词路径。
    ///
    /// M119 r1 / r2 复审 P2-1：生产默认后端是脚本化 rig，界面上曾**没有任何模拟标识**；
    /// 上一轮又只有标题胶囊认这个开关，另外三处会反方向地留在「没接真实模型」上。
    pub note: Disclaimer<'a>,
    /// 三档状态（决定头部画哪组动作）。
    pub rail: RailState,
    /// 宿主。
    pub host: Host,
    /// 键盘软焦点（`None` = 用户在用鼠标，**不画焦点环**）。
    ///
    /// 为什么不直接给 `Focus`：焦点环是键盘巡航的可视证据，鼠标操作时还挂着 10% 状态层会让人
    /// 以为那是一个 tonal 按钮（r1 出图实测）。`AgentUi::keyboard_mode` 就是这道闸。
    pub focus: Option<Focus>,
}

/// 画会话视图（**右栗与独立页的唯一实现**）。
pub fn view<'a>(ui: &'a Ui, spec: Conversation<'a>, catalog: Catalog) -> Element<'a, Message> {
    let mut body = column![header(ui, spec, catalog), divider(ui)]
        .spacing(ui.space(Space::X2))
        .width(Length::Fill)
        .height(Length::Fill);

    // 失败横幅放在会话**上方**：错误是这一轮的结果，比历史更需要先看见（M119）。
    if let Some(failure) = spec.failure {
        body = body.push(banners::failure(ui, failure, catalog));
    }

    body = body.push(if spec.messages.is_empty() {
        transcript::empty(ui, catalog, spec.note.empty_body)
    } else {
        transcript::list(ui, spec, catalog)
    });

    // 确认条贴着输入区（动作在下面，读完就能按）。
    if let Some(request) = spec.confirm {
        body = body.push(banners::confirm(ui, request, catalog, spec.focus));
    }

    body.push(divider(ui))
        .push(composer::composer(ui, spec, catalog))
        .into()
}

// ---------------------------------------------------------------- 头部

fn header<'a>(ui: &'a Ui, spec: Conversation<'a>, catalog: Catalog) -> Element<'a, Message> {
    let mut title = row![text_role(
        ui,
        TypeRole::TitleLarge,
        true,
        catalog.text(Key::AgentTitle),
        ui.color(Role::OnSurface),
        Length::Shrink,
    )]
    .spacing(ui.space(Space::X2))
    .align_y(iced::Alignment::Center);
    // 常驻标记：与标题同一行，不点开、不 hover、不藏在设置里（P2-1 的判据）
    if spec.note.simulated {
        title = title.push(simulated_chip(ui, catalog));
    }
    title = title.push(unread_badge(ui, spec.unread, catalog));

    let titles = column![
        title,
        text_role(
            ui,
            TypeRole::LabelMedium,
            false,
            spec.note.subtitle,
            ui.color(Role::OnSurfaceVariant),
            Length::Fill,
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);

    // 独立页的头部与页面同一套（标题在左、动作在右）；右栗太窄，动作单独一行，免得挤成两截。
    match spec.host {
        Host::Rail => column![
            titles,
            row![actions(ui, spec, catalog)]
                .width(Length::Fill)
                .spacing(ui.space(Space::X2)),
        ]
        .spacing(ui.space(Space::X2))
        .width(Length::Fill)
        .into(),
        Host::Page => row![
            titles,
            row![actions(ui, spec, catalog)]
                .spacing(ui.space(Space::X2))
                .align_y(iced::Alignment::Center),
        ]
        .width(Length::Fill)
        .align_y(iced::Alignment::Center)
        .into(),
    }
}

/// 头部动作：`⌄` 收起 / `⇥` 拆出（停靠时）或 `⇤` 收回（独立页）。
///
/// 每个按钮都带**文字标签**（图标只做前导）：任务要求「不得只有图标」。焦点环用
/// `Interaction::Focused`（组件的 10% 状态层），键序见 [`Focus`]。
fn actions<'a>(ui: &'a Ui, spec: Conversation<'a>, catalog: Catalog) -> Element<'a, Message> {
    let mut items: Vec<Element<'a, Message>> = Vec::new();

    // 页宿主只有一个正确动作：收回右栗。这里先按**宿主**分流（而不是先按档位）是结构性兜底：
    // 就算哪天档位同步漏了，独立页也不会再出现「Open assistant / Collapse / Pop out」这种
    // 右栗才该有的按钮（复审 r1 P2-1 的可见症状）。
    if spec.host == Host::Page {
        return row![label_button(
            ui,
            catalog.text(Key::AgentDock),
            IconKind::ChevronLeft,
            Some(Message::Dock),
            spec.focus == Some(Focus::Detach),
        )]
        .spacing(ui.space(Space::X2))
        .align_y(iced::Alignment::Center)
        .into();
    }

    match spec.rail {
        RailState::Collapsed => items.push(label_button(
            ui,
            catalog.text(Key::AgentOpen),
            IconKind::ChevronRight,
            Some(Message::Expand),
            spec.focus == Some(Focus::Collapse),
        )),
        RailState::Expanded => {
            items.push(label_button(
                ui,
                catalog.text(Key::AgentCollapse),
                IconKind::ChevronDown,
                Some(Message::Collapse),
                spec.focus == Some(Focus::Collapse),
            ));
            items.push(label_button(
                ui,
                catalog.text(Key::AgentDetach),
                IconKind::Expand,
                Some(Message::Detach),
                spec.focus == Some(Focus::Detach),
            ));
        }
        RailState::Detached => items.push(label_button(
            ui,
            catalog.text(Key::AgentDock),
            IconKind::ChevronLeft,
            Some(Message::Dock),
            spec.focus == Some(Focus::Detach),
        )),
    }

    row(items)
        .spacing(ui.space(Space::X2))
        .align_y(iced::Alignment::Center)
        .into()
}

fn label_button<'a>(
    ui: &'a Ui,
    label: &'a str,
    icon: IconKind,
    on_press: Option<Message>,
    focused: bool,
) -> Element<'a, Message> {
    let button = match on_press {
        Some(message) => Button::new(label, message).leading(icon),
        None => Button::disabled(label).leading(icon),
    };
    buttons::text(ui, focused_state(button, focused))
}

/// 焦点环：键盘走到这一格时叠 10% 的 `Focused` 状态层（组件库的取证状态也是它）。
pub(super) fn focused_state<'a>(button: Button<'a, Message>, focused: bool) -> Button<'a, Message> {
    if focused {
        button.state(Interaction::Focused)
    } else {
        button
    }
}

/// 「模拟」标记：`SurfaceVariant` / `OnSurfaceVariant` 的 tonal 小胶囊（中性信息，不是告警）。
fn simulated_chip<'a>(ui: &'a Ui, catalog: Catalog) -> Element<'a, Message> {
    let style = ui.style(TypeRole::LabelMedium);

    container(
        text(catalog.text(Key::AgentSimulated))
            .size(style.size_px)
            .font(ui.font_of(style))
            .color(ui.color(Role::OnSurfaceVariant)),
    )
    .padding(Padding::from([ui.space(Space::X1), ui.space(Space::X2)]))
    .style(move |_theme| container::Style {
        background: Some(Background::Color(ui.color(Role::SurfaceVariant))),
        border: Border {
            radius: ui.radius(ShapeScale::Full, ui.metrics().control_height / 2.0),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

fn unread_badge<'a>(ui: &'a Ui, unread: usize, catalog: Catalog) -> Element<'a, Message> {
    if unread == 0 {
        return space::horizontal().into();
    }

    let count = unread.to_string();
    let label = catalog.render(Key::AgentUnread, &[("count", count.as_str())]);
    let style = ui.style(TypeRole::LabelMedium);

    container(
        text(label)
            .size(style.size_px)
            .font(ui.font_of(style))
            .color(ui.color(Role::OnSecondaryContainer)),
    )
    .padding(Padding::from([ui.space(Space::X1), ui.space(Space::X2)]))
    .style(move |_theme| container::Style {
        background: Some(Background::Color(ui.color(Role::SecondaryContainer))),
        border: Border {
            radius: ui.radius(ShapeScale::Full, ui.metrics().control_height / 2.0),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

/// 一像素分隔线（`pages::frame::divider` 绑在 `pages::Message` 上，这里要自己的消息类型）。
fn divider<'a>(ui: &'a Ui) -> Element<'a, Message> {
    rule::horizontal::<Theme>(ui.metrics().divider_thickness)
        .style(move |_theme| rule::Style {
            color: ui.color(Role::OutlineVariant),
            radius: ui.radius_dp(ShapeScale::None),
            fill_mode: rule::FillMode::Full,
            snap: true,
        })
        .into()
}
