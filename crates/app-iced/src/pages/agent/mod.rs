//! # AI 助手（右栗三档 + `Route::Agent` 独立页）
//!
//! 用户定案（D16）：「右侧常驻可折叠，允许拆分成独立页」+「中栗宽度可调（磁铁吸附）」。
//! 本模块交付**壳**：三档状态、布局数学、两个宿主共用的一份会话视图。
//!
//! **会话数据来自 presenter**（M119）：唯一来源是 `app/state.rs` 里的 `agent_host` →
//! `AgentRail`。壳里不再有样本会话、也不再自己数未读——M104 当时的 `demo::transcript()`
//! 只剩「截图取证要不要自动跑一轮」这一件事（见 [`demo`]）。
//!
//! ## 文件与职责
//!
//! | 文件 | 职责 |
//! |---|---|
//! | [`state`] | 三档状态机 + **布局数学**（纯函数，可脱机单测）+ [`AgentUi`]（挂在 `App` 上的纯 UI 态） |
//! | `conversation` | **会话视图组件**：右栗与独立页唯一的对话 UI（输入契约见该文件） |
//! | [`demo`] | `ULL_AGENT_*` 取证开关（自动跑一轮 / 三档 / 宽度 / 焦点环） |
//! | 本文件 | 两个**宿主**（[`docked`] 右栗 / [`page`] 独立页）+ 消息枚举 + 拖拽层 |
//! | `tests.rs` / `metrics/tests.rs` | 三档迁移 / 单实现判据 / a11y 文案；数值表与磁铁在后者 |
//!
//! ## 为什么物理路径在 `pages/agent/`、逻辑路径却是 `crate::shell::agent`
//!
//! M104 的 scope 给了 `pages/agent/**`，但**没给** `pages/mod.rs`（那是四条页面线共用的声明文件）。
//! 于是 `shell/mod.rs` 用 `#[path = "../pages/agent/mod.rs"]` 挂载它——与 `app/mod.rs` 挂
//! `../components/mod.rs` 是同一个先例。**没有第二份实现**：物理目录只有这一处。
//!
//! ## 三档状态（[`RailState`]）
//!
//! | 档位 | 右栗 | 会话在哪 | 切法 |
//! |---|---|---|---|
//! | `Collapsed` | 48 图标条 + 未读点 | 不画 | 点条上的 AI 图标 / `Esc` |
//! | `Expanded` | 280~360（默认 320）+ 8 拖拽缝 | 面板内 | 条上图标 / `⌄` 收起 / 拆出后收回 |
//! | `Detached` | 0 | `Route::Agent` 独立页 | `⇥` 拆出 / 导航第五项 `Assistant` |
//!
//! ## 为什么消息不走 `pages::Message`
//!
//! 助手栏是**外壳级视图**（挂在任何页面右边），不是某一页的消息：塞进 `pages::Message` 要动
//! `app/update/page_messages.rs` 的穷尽 match，语义也不对。所以是一等公民 [`Message`] +
//! `app::Message::Agent(..)` 一次映射，落点 `app/update/agent.rs`。

pub(crate) mod demo;
mod metrics;
mod state;

mod conversation;
mod strip;

#[cfg(test)]
mod tests;

pub use state::{AgentUi, Focus, RailState};

use iced::widget::{column, container, mouse_area, row, rule, space, text, tooltip};
use iced::{mouse, Alignment, Background, Border, Element, Length, Padding, Theme as IcedTheme};

use unlucky_design::color::Role;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Catalog, Key};

use unlucky_presenter::agent::AgentRail;
use unlucky_presenter::dto::AppEvent;

use crate::pages::{self, layout};
use crate::theme::Ui;

use conversation::{Conversation, Host};
use metrics::RESIZE_GUTTER;

/// 助手栏消息（**唯一**的消息类型；`app/view.rs` 用 `Message::Agent(..)` 包一层）。
#[derive(Debug, Clone)]
pub enum Message {
    /// 折叠条 → 展开。
    Expand,
    /// `⌄` 收起 / 独立页上 `Esc`。
    Collapse,
    /// `⇥` 拆成独立页（同时压一层 `Route::Agent`）。
    Detach,
    /// `⇤` 收回右栗（离开 `Route::Agent`）。
    Dock,
    /// 输入框的编辑动作（由 `text_editor::on_action` 来）。
    Edit(iced::widget::text_editor::Action),
    /// 发送（`Send` 键 / 输入框里按 Enter）。
    Submit,
    /// 中断流式回复（`AppCommand::StopAgent`；收敛靠后端事件）。
    Stop,
    /// 允许 / 拒绝当前待确认的写操作（`rails.confirm` 非空时才有按钮）。
    Confirm(bool),
    /// **事件回流**（M119）：presenter 侧产生的 `AppEvent` 整批回到 `update` 的那条通道。
    ///
    /// 为什么是消息而不是直接改状态：异步后端在后台线程里产事件，UI 只能在节拍里抽；
    /// 抽出来的这一批必须和「用户点击」走同一条漏斗（`on_agent`），否则两处各改一次状态，
    /// 未读/流式标记会打架。落点见 `app/update/agent.rs` 的 `Events` 分支。
    Events(Vec<AppEvent>),
    /// 按下拖拽缝。
    DragStart,
    /// 拖拽中：窗口坐标里的指针 x。
    DragTo(f32),
    /// 松手。
    DragEnd,
    /// 键盘：`Esc`（收栗 / 独立页收回）。
    Escape,
    /// 键盘：`Tab`（走焦点环；`shift` = 反向）。
    Tab { shift: bool },
    /// 键盘：`Enter`（激活软焦点上的按钮）。
    Activate,
    /// 原始键盘事件（`app/subscription.rs` 的 `keyboard::listen()` 搬过来的）。
    ///
    /// 订阅层只负责搬运，**解释权**在 `app/update/agent.rs` 的 `on_agent_key`：
    /// `Esc` / `Tab` / `Enter` 各自在什么状态下做什么，集中在一个函数里一眼看完。
    Key(iced::keyboard::Event),
}

/// 错误横幅的**展示输入**（M119「失败态可见」的唯一落点）。
///
/// 为什么要包一层而不是把 `String` 传进来：标题必须是 catalog 键（`lang` 说哪种语言就哪种），
/// 正文则有两种合法来源——具名原因码对应的 catalog 译文（端不可达 / 未实现），或 presenter
/// 已经脱敏过的这一轮错误原文（`AgentRail::error`）。两者都是「已 sanitize 的展示串」，
/// 页面不再加工（UI 层不复制脱敏逻辑）。
#[derive(Debug, Clone, Copy)]
pub struct Failure<'a> {
    /// 横幅标题的 catalog 键。
    pub title: Key,
    /// 横幅正文（已脱敏 / 已是译文）。
    pub body: &'a str,
}

/// 「这是模拟助手」的**四处文案**（M119 r2 / 复审 P2-1）。
///
/// ## 为什么是一个结构体而不是四个 `if`
///
/// 上一轮只有标题胶囊由 `simulated` 驱动，另外三处（副标题 / 输入框占位符 / 空态正文）是
/// **无条件取词**，而它们的译文里写着「模拟 / 没接真实模型」——接上真 transport 之后，这三句会
/// 反方向地留着（`simulated == false` 时界面上还在说「没接真实模型」）。复审把这条列为 P2。
///
/// 现在四处**只认这一个开关**：`Disclaimer::of(catalog, simulated)` 一次算好，视图没有第二种
/// 取词路径。`pages/agent/tests/disclaimer.rs` 钉住「四处一起出现 / 一起消失」。
#[derive(Debug, Clone, Copy)]
pub struct Disclaimer<'a> {
    /// 这套后端是不是模拟的（标题旁的胶囊只在 `true` 时画）。
    pub simulated: bool,
    /// 标题下的副标题。
    pub subtitle: &'a str,
    /// 输入框占位符。
    pub placeholder: &'a str,
    /// 空会话正文。
    pub empty_body: &'a str,
}

impl<'a> Disclaimer<'a> {
    /// 按 `simulated` 选模拟态或真实态那一套文案。
    #[must_use]
    pub fn of(catalog: Catalog, simulated: bool) -> Self {
        Self {
            simulated,
            subtitle: if simulated {
                catalog.text(Key::AgentSubtitle)
            } else {
                catalog.text(Key::AgentSubtitleReal)
            },
            placeholder: if simulated {
                catalog.text(Key::AgentPlaceholder)
            } else {
                catalog.text(Key::AgentPlaceholderReal)
            },
            empty_body: if simulated {
                catalog.text(Key::AgentEmptyBody)
            } else {
                catalog.text(Key::AgentEmptyBodyReal)
            },
        }
    }
}

/// 两个宿主共用的**会话入参**（`Session` = 从 presenter 借来的那一份 + 两个展示事实）。
///
/// 为什么是一个结构体而不是继续加形参：`docked`/`page` 已经收到 6 个形参，这一轮还要加
/// 「是不是模拟后端」——那会变成 7 个，正是塔台登记过的可维护性观察项。收成结构体之后两个宿主
/// 各 3 个形参，且以后再加展示事实只改这个结构体。
#[derive(Clone, Copy)]
pub struct Session<'a> {
    /// 三档/草稿/焦点（纯 UI 态）。
    pub agent: &'a AgentUi,
    /// 会话内容 / 未读 / busy / 待确认（presenter 的唯一真相）。
    pub rail: &'a AgentRail,
    /// 失败横幅的展示输入。
    pub failure: Option<Failure<'a>>,
    /// 这套后端是不是模拟的（M119 r1 / 复审 P2-1）。`true` ⇒ 头部常驻「模拟」标记，
    /// 且三处文案走模拟态（见 [`Disclaimer`]：**四处一起变**，只认这一个开关）。
    pub simulated: bool,
}

/// 停靠档的右栗（`None` = 已拆成独立页，右栗不占地）。
///
/// 两档共用同一条 `Element` 通道交给外壳的 `row!`：折叠是 48 的条，展开是「8 拖拽缝 + 面板」。
pub fn docked<'a>(
    ui: &'a Ui,
    session: Session<'a>,
    catalog: Catalog,
    window_width: f32,
) -> Option<Element<'a, Message>> {
    // 判档用**这一窗宽下实际生效**的档位：窗窄到放不下 280 时，`Expanded` 事实上就是折叠条
    // （否则会画一条 48 宽、还白扣 8px 拖拽缝的「面板」）。
    match session.agent.effective_rail(window_width) {
        RailState::Detached => None,
        RailState::Collapsed => Some(strip::strip(ui, session.rail, catalog)),
        RailState::Expanded => Some(panel(ui, session, catalog, window_width)),
    }
}

/// `Route::Agent` 独立页（与右栗**同一个** `conversation::view`）。
///
/// 页级骨架（页边距 + 正文列宽）与其它页面同源：列宽取 [`layout::frame_metrics`]，单列 ≤424。
pub fn page<'a>(
    ui: &'a Ui,
    session: Session<'a>,
    catalog: Catalog,
    window_width: f32,
) -> Element<'a, Message> {
    let metrics = layout::frame_metrics(pages::PaneKit::Single, window_width);

    let body = conversation::view(ui, spec(session, catalog, Host::Page), catalog);

    container(
        container(body)
            .width(Length::Fixed(metrics.column))
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(metrics.gutter)
    .align_x(Alignment::Center)
    .align_y(Alignment::Start)
    .into()
}

/// 拖拽层：拖拽期间铺满窗口，把指针 x 变成 [`Message::DragTo`]。
///
/// 为什么铺满整窗：`mouse_area::on_move` 只在**它自己的边界内**回报坐标，而拖拽时指针一定会离开
/// 那 8px 的缝隙。铺满整窗后 `Point.x` 就是窗口坐标 ⇒ `目标宽 = 窗口宽 − x`。
///
/// **三条收尾路径**（复审 P3-7：`mouse_area` 在 `!cursor.is_over` 时直接 return，窗**外**松手
/// 的 `on_release` 永远不来，于是叠层会留在最上层、右栗继续跟着指针缩放）：
///
/// | 路径 | 触发 | 语义 |
/// |---|---|---|
/// | `on_release` | 窗内松手 | 正常结束 |
/// | `on_exit` | 指针离开窗口 | 「拖到窗外」= 收在当前值（不是把指针丢掉继续跟） |
/// | `on_press` | 拖拽中再按一次 | 兜底清态（万一上两条都没来，一次点击就能恢复） |
pub fn drag_overlay<'a>(agent: &AgentUi) -> Option<Element<'a, Message>> {
    if !agent.is_dragging() {
        return None;
    }

    Some(
        mouse_area(
            container(space::horizontal())
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .interaction(mouse::Interaction::ResizingHorizontally)
        .on_move(|point| Message::DragTo(point.x))
        .on_release(Message::DragEnd)
        .on_press(Message::DragEnd)
        .on_exit(Message::DragEnd)
        .into(),
    )
}

// ---------------------------------------------------------------- 展开档：拖拽缝 + 面板

fn panel<'a>(
    ui: &'a Ui,
    session: Session<'a>,
    catalog: Catalog,
    window_width: f32,
) -> Element<'a, Message> {
    let width = session.agent.rail_width(window_width);

    let body = container(conversation::view(
        ui,
        spec(session, catalog, Host::Rail),
        catalog,
    ))
    .width(Length::Fixed(width))
    .height(Length::Fill)
    .padding(Padding {
        top: ui.space(Space::X6),
        right: ui.space(Space::X4),
        bottom: ui.space(Space::X4),
        left: ui.space(Space::X4),
    })
    .style(move |_theme| container::Style {
        background: Some(Background::Color(ui.layer(MaterialLayer::Canvas))),
        ..container::Style::default()
    });

    row![splitter(ui, width, catalog), body]
        .width(Length::Shrink)
        .height(Length::Fill)
        .into()
}

/// 8px 拖拽缝：1px 分隔线居中 + 水平缩放光标 + 按下即开拖 + **可读标签**（tooltip）。
///
/// 复审 P3-5：这条 8px 的缝此前没有任何可读文字，而 `AgentResize` / `AgentWidthLabel` 两条 key
/// 在视图里一次没用（只在测试里被断言「不空」）——那是空头支票。现在 tooltip 同时给出
/// 「拖它做什么」（`AgentResize`）与「当前多宽」（`AgentWidthLabel`），两个 key 都真用上了。
fn splitter<'a>(ui: &'a Ui, width: f32, catalog: Catalog) -> Element<'a, Message> {
    let line = container(
        rule::vertical::<IcedTheme>(ui.metrics().divider_thickness).style(move |_theme| {
            rule::Style {
                color: ui.color(Role::OutlineVariant),
                radius: ui.radius_dp(ShapeScale::None),
                fill_mode: rule::FillMode::Full,
                snap: true,
            }
        }),
    )
    .width(Length::Fixed(RESIZE_GUTTER))
    .height(Length::Fill)
    .align_x(Alignment::Center);

    let grab = mouse_area(line)
        .interaction(mouse::Interaction::ResizingHorizontally)
        .on_press(Message::DragStart);

    let tip_style = ui.style(TypeRole::BodySmall);
    let pixels = format!("{width:.0}");
    let tip = container(column![
        text(catalog.text(Key::AgentResize))
            .size(tip_style.size_px)
            .font(ui.font_of(tip_style))
            .color(ui.color(Role::OnSurface)),
        text(catalog.render(Key::AgentWidthLabel, &[("width", pixels.as_str())]))
            .size(tip_style.size_px)
            .font(ui.font_of(tip_style))
            .color(ui.color(Role::OnSurfaceVariant)),
    ])
    .padding(Padding::from([ui.space(Space::X1), ui.space(Space::X2)]))
    .style(move |_theme| container::Style {
        background: Some(Background::Color(ui.layer(MaterialLayer::Overlay))),
        border: Border {
            radius: ui.radius_dp(ShapeScale::Small),
            ..Border::default()
        },
        ..container::Style::default()
    });

    tooltip(grab, tip, tooltip::Position::Left).into()
}

/// 会话视图的入参（**唯一**的组装点：两个宿主都走这里，不许各拼一份）。
fn spec<'a>(session: Session<'a>, catalog: Catalog, host: Host) -> Conversation<'a> {
    Conversation {
        // 会话内容、未读、busy、待确认：**全部**来自 presenter（M119）
        messages: &session.rail.messages,
        busy: session.rail.busy,
        unread: session.rail.unread,
        confirm: session.rail.confirm.as_ref(),
        failure: session.failure,
        note: Disclaimer::of(catalog, session.simulated),
        draft: &session.agent.draft,
        blank: session.agent.draft_is_blank(),
        rail: session.agent.rail,
        host,
        // 只有键盘巡航过才画焦点环（鼠标操作时那是噪音，而且会被误读成 tonal 按钮）
        focus: session.agent.keyboard_mode.then_some(session.agent.focus),
    }
}
