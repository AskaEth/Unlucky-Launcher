//! 会话选择器（M453 `LOGSESSIONS`）：日志页顶部「看的是哪一场」的那块卡，外加它的点击落点。
//!
//! ## 为什么需要它（用户 2026-11-20 的成本令）
//!
//! M453 之前这一页只有「当前那一场」：会话索引（`presenter::state::log_sessions`）早就通到了视图，
//! 但页面没有一处能**选**它——`AppCommand::OpenLogSession` / `RefreshLogSessions` 在 iced 侧只出现
//! 在注释里。于是崩溃过的那一场、上个版本跑的那一场，用户看不了也切不了。
//!
//! ## 三档，各自长什么样
//!
//! | 会话数 | 卡片高度 | 正文 |
//! |---|---|---|
//! | 0 | `Shrink`（一行字高） | 一句话（[`Key::LogsSessionsEmpty`]）：**不能是一块空白** |
//! | 1..=[`VISIBLE_ROWS`] | `Shrink` | 直接列行，**没有滚动区**（一场的时候不该有滚动条这种冗余 UI） |
//! | > [`VISIBLE_ROWS`] | `Fixed(visible_height)` | 定高 + 内部滚动（[`panel::scroll`]）：会话再多也不能把页头/诊断卡挤出去 |
//!
//! 高度是**令牌算出来的**（[`visible_height`]：行高 × 可见行数 + 间距 + 标题行 + 卡内距），
//! 不写像素字面量，也**不占** `DIAGNOSIS_SHARE : LOGS_SHARE` 的份额——那两块的比例是 M95 定的
//! （诊断 4 : 日志 2），第三块卡插进来不该悄悄改掉它。
//!
//! ## 一行用什么画
//!
//! [`ListItem`]（组件库的列表项：44 高、圆角 8、选中态 `SecondaryContainer`）。选它而不是手搓
//! `button` 行：它就是这个仓「从一列东西里挑一个」的既有语汇（实例列表、设置页的列表都用它），
//! 选中态、hover 状态层、行高全在组件层，页面不再复制一份。
//!
//! 行文案 = `实例 · 时间 · 结局`，与页头副标题**同一个模板**（[`super::model::status_line`]）：
//! 用户看页头说「开发测试 · 11-02 19:40 · 崩溃」，选择器里那一行逐字相同，不用做二次对表。
//!
//! ## 当前那一场为什么还能点
//!
//! 选中的那一行**不禁用**、也不换成「当前项」标记（`Chip::current` 那一档）：再点一次就是
//! 「重新读一遍这场」（presenter 的 `open_session` 是纯读），而禁用会把行压成 38% 的灰
//! （那是「不可用」的语汇，见 `selection/paint.rs`）。代价是「看起来可点、点了没变化」——
//! 但那一次点击**确实**重读了文件，所以它不是空动作。
//!
//! ## 空列表的判据在页面侧
//!
//! 会话索引只在**结束时**追加（`presenter::host::logs::sessions`），所以「正在跑「的那一场不在里面：
//! 此时日志有行、会话 0 场——这正是空态那句话要出现的地方（不是「什么都没有」的整页空态）。
//!
//! ## M462：回到实时那一场
//!
//! 同一个事实（正在跑那一场不在索引里）带来一条用户陷阱：点了一条历史会话之后正文就被历史视图
//! 挡着（M459），而退出的两个条件——「被挡住那一场收尾」「换了一场在跑」——在游戏一直跑时都不发生。
//! 于是标题行右侧多一颗 [`Key::LogsBackToLive`] 文字按钮（[`back_to_live`]），落点是既有的
//! [`Message::LogsOpenSession`]，载荷 = 被挡住那一场的 id（[`back_to_live_message`]）。
//!
//! **没有在跑的会话时这颗按钮不存在**（`Logs::back_to_live` 为 `None`）：判据在 presenter
//! （`LogView::live_return`），不在页面里——页面只照它画。`DevDocs/artifacts/m462/` 的两张图
//! 就是这两档（有 / 没有该入口）。

use iced::widget::{column, row, Space};
use iced::{Alignment, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space as Spacing;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::catalog::{Catalog, Key};
use unlucky_i18n::Lang;

use crate::app::components::buttons::{self, Button};
use crate::app::components::navigation::{list_item, ListItem};
use crate::theme::Ui;

use super::super::frame;
use super::super::Message;
use super::model::{self, Logs, Outcome};
use super::panel;

/// 不出现滚动条时最多列几行（超过这一档就定高 + 内部滚动，见模块文档的三档表）。
pub(super) const VISIBLE_ROWS: usize = 2;

/// 一场历史会话（`presenter::dto::LogSession` 的展示投影）。
///
/// 前四个字段是数据（原样转交），[`label`](Self::label) 是**按语言拼好的行文案**：`ListItem`
/// 借用 `&'a str`，渲染期拼出来的 `String` 活不过本次 `card()`，所以文案只能由视图模型持有
/// ——与 `Logs::status` 同一条理由（见 [`Logs::with_status`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// 会话 id（`AppCommand::OpenLogSession` 的载荷，原样转交，页面不解释它）。
    pub id: String,
    /// 实例名。
    pub instance: String,
    /// 展示用时间（`10-29 18:40`，presenter 已经格式化好）。
    pub when: String,
    /// 结局。
    pub outcome: Outcome,
    /// 行文案（[`fill_labels`] 拼；拼之前等于 [`when`](Self::when)）。
    pub label: String,
}

impl Session {
    /// 一场会话（`label` 先填成时间，语言相关的那半留给 [`fill_labels`]）。
    #[must_use]
    pub fn new(id: &str, instance: &str, when: &str, outcome: Outcome) -> Self {
        Self {
            id: id.to_owned(),
            instance: instance.to_owned(),
            when: when.to_owned(),
            outcome,
            label: when.to_owned(),
        }
    }
}

/// 按语言把每行的文案拼好（[`Logs::with_status`] 调它，与页头副标题同一模板同一时机）。
///
/// 结局文案走 `LaunchOutcome*` 三 key（`status_line` 内部），所以中英各出各的；`when` 缺席
/// 这种情形不存在（presenter 总会给一个串），但真的空着时退回原样，**不编一个词**。
pub(super) fn fill_labels(sessions: &mut [Session], lang: Lang) {
    for session in sessions {
        let line = model::status_line(
            Some(&session.instance),
            Some(&session.when),
            Some(session.outcome),
            lang,
        );
        session.label = line.unwrap_or_else(|| session.when.clone());
    }
}

/// 会话选择器卡片（高度三档见模块文档）。
pub fn card<'a>(ui: &'a Ui, logs: &'a Logs, lang: Lang) -> Element<'a, Message> {
    let catalog = Catalog::of(lang);
    let count = logs.sessions.len();

    let head = row![
        frame::text_role(
            ui,
            TypeRole::LabelMedium,
            true,
            catalog.text(Key::LogsSessionsTitle),
            ui.color(Role::OnSurfaceVariant),
        ),
        Space::new().width(Length::Fill),
        frame::text_role_owned(
            ui,
            TypeRole::LabelMedium,
            false,
            count.to_string(),
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .spacing(ui.space(Spacing::X2));

    // M462：历史视图下**唯一**回实时那一场的路。`None` ⇒ 一个控件都不画（判据在 presenter，
    // 见 [`back_to_live`] 的模块文档与 `pages::Logs::back_to_live`）。
    let head = match logs.back_to_live.as_deref() {
        Some(id) => head.push(back_to_live(ui, &catalog, id)),
        None => head,
    };

    let head = head.push(card_button(
        ui,
        catalog.text(Key::LogsSessionsRefresh),
        Message::LogsRefreshSessions,
    ));

    let body = if count == 0 {
        empty(ui, &catalog)
    } else if count > VISIBLE_ROWS {
        panel::scroll(ui, list(ui, logs))
    } else {
        list(ui, logs)
    };

    let height = if count > VISIBLE_ROWS {
        Length::Fixed(visible_height(ui))
    } else {
        Length::Shrink
    };

    panel::card(
        ui,
        height,
        column![head, body]
            .spacing(ui.space(Spacing::X2))
            .width(Length::Fill),
    )
}

/// 定高档的高度：`VISIBLE_ROWS` 行 + 行间距 + 标题行 + 卡内距（**全部取令牌**）。
///
/// 为什么自己算而不是 `FillPortion`：份额是同一列里各块**互相**分配的比例，第三块卡拿一份就
/// 等于把 M95 定的诊断 4 : 日志 2 一起改掉；这里是「这块卡自己需要多少」，与别人无关。
///
/// **标题行的高度是 `control_height`（40），不是 `LabelMedium` 的行高**：首行右侧那颗
/// `Refresh` 按钮是标准控件高，它把这一行撑到 40。M453 r1 按文字行高算（16），实测滚动视口只有
/// ~60px ⇒ 第二行被从中间切掉（`DevDocs/artifacts/m453/` 的第一版图就是这个形状）。文字行高只
/// 在**没有按钮**的那几块卡里才是标题行的真高度。
pub(super) fn visible_height(ui: &Ui) -> f32 {
    let metrics = ui.metrics();
    let head = metrics
        .control_height
        .max(ui.style(TypeRole::LabelMedium).line_height_px);
    let rows = metrics.row_height * VISIBLE_ROWS as f32;
    let gaps = ui.space(Spacing::X1) * (VISIBLE_ROWS.saturating_sub(1)) as f32;

    rows + gaps + ui.space(Spacing::X2) + head + metrics.card_padding * 2.0
}

/// 会话列表（顺序 = presenter 给的顺序：页面**不重排**，与诊断结论同一条纪律）。
fn list<'a>(ui: &'a Ui, logs: &'a Logs) -> Element<'a, Message> {
    column(
        logs.sessions
            .iter()
            .map(|session| item(ui, logs, session))
            .collect::<Vec<_>>(),
    )
    .spacing(ui.space(Spacing::X1))
    .width(Length::Fill)
    .into()
}

/// 一行会话：可点的 [`ListItem`]，当前显示的那一场带选中态。
fn item<'a>(ui: &'a Ui, logs: &'a Logs, session: &'a Session) -> Element<'a, Message> {
    let spec = ListItem::new(&session.label, row_message(session)).base(panel::BASE);
    let spec = if logs.selected.as_deref() == Some(session.id.as_str()) {
        spec.selected()
    } else {
        spec
    };

    list_item(ui, spec)
}

/// 一行 → 一条页面消息：**唯一**把「点了哪一场」翻成 [`Message`] 的地方。
///
/// 单独成函数是为了可判：`sessions_tests::clicking_a_row_names_that_session` 直接钉它，
/// `sessions_tests::the_card_wires_every_row_through_row_message` 扫 [`card`] 的源码确认
/// 按钮真的走这条（不是另写一份 id 拼接）。
pub(super) fn row_message(session: &Session) -> Message {
    Message::LogsOpenSession(session.id.clone())
}

/// 「回到实时那一场」的入口（M462）：卡片标题行的一颗文字按钮。
///
/// ## 为什么放在标题行，而不是列表里多一行
///
/// ①**它回答的是同一个问题**——「我现在在看哪一场」，而标题行就是这一块的标题。②列表里的行是
/// `实例 · 时间 · 结局`（[`Session::label`]，与页头副标题同一模板）；正在跑那一场**不在会话索引
/// 里**（索引只在收尾时追加，`core::launch::session::SessionWriter::finish`），没有实例名、没有
/// 时间、没有结局——一个只有「回到实时」四个字的行夹在中间看起来像坏了。③列表在
/// [`VISIBLE_ROWS`] 以上是定高滚动的（三档表在模块文档里），滚下去就看不见的入口不能当逃生口。
///
/// ## 它什么时候不存在
///
/// `logs.back_to_live` 为 `None` 时**一个控件都不画**（[`card`] 里那条 `match`）。判据在
/// presenter 的 `LogView::live_return()`：历史视图 + 被挡住那一场**仍在跑**。收尾之后被挡住位
/// 还留着（M459 的尾巴行判据要用），但它的 `holding` 已经落下 ⇒ 没有入口。没有在跑的会话时
/// **不存在这颗按钮**，所以「点了没用的东西」在界面上不可能出现。
fn back_to_live<'a>(ui: &'a Ui, catalog: &Catalog, id: &'a str) -> Element<'a, Message> {
    card_button(
        ui,
        catalog.text(Key::LogsBackToLive),
        back_to_live_message(id),
    )
}

/// 卡片内的文字按钮（标题行那两颗：`Refresh` 与 M462 的「回到实时」）。
///
/// 两者共用这一处 `.base(panel::BASE)`：卡片里的控件状态层要叠在**卡底**上，不是画布底
/// （与诊断卡的 Copy summary 同一条）。共用不是为了省行数，是为了**只有一处**底——两颗按钮
/// 各写一份迟早会漂（`pages/tests/base_wiring.rs` 那张接线表数的是这一处）。
fn card_button<'a>(ui: &'a Ui, label: &'a str, message: Message) -> Element<'a, Message> {
    buttons::text(ui, Button::new(label, message).base(panel::BASE))
}

/// 入口 → 一条页面消息：**唯一**把「回到实时」翻成 [`Message`] 的地方（与 [`row_message`]
/// 同一条纪律：`sessions_tests` 直接钉载荷，另一条用例扫源码确认卡片走的是它）。
///
/// 落点复用既有的 [`Message::LogsOpenSession`]：`id` 就是被历史视图挡住那一场，链条是
/// `app/update/page_messages.rs` → `AppCommand::OpenLogSession(id)` → `CoreHost::open_session`
/// ——在那儿它由**尾随器**认出来（不在会话索引里，`host::logs::open` 的 `find` 找不到它），
/// 整份重读后发回 `LogSessionOpened` + `LogLinesChanged` ⇒ presenter 退出历史视图。
pub(super) fn back_to_live_message(id: &str) -> Message {
    Message::LogsOpenSession(id.to_owned())
}

/// 空列表：一句话（不是一块空白，也不是整页空态——那时日志可能正在实时滚动）。
fn empty<'a>(ui: &'a Ui, catalog: &Catalog) -> Element<'a, Message> {
    frame::text_role_owned(
        ui,
        TypeRole::BodySmall,
        false,
        catalog.text(Key::LogsSessionsEmpty).to_owned(),
        ui.color(Role::OnSurfaceVariant),
    )
}
