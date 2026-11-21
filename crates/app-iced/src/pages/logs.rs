//! 日志页（M95）：**单列**版面的第二个用例，也是本项目卖点（崩溃日志诊断）的落地页。
//!
//! ## 一屏由三块组成
//!
//! ```text
//! ┌ 页头（两行 84：标题 + 运行状态；右侧 Open folder / Clear）──────────┐
//! ├ 会话选择器（M453：SESSIONS + 条数 + Refresh；空/单/多三档，见 sessions）──┤
//! ├ 诊断卡片（只在有结论时出现，**按真严重度分组**：CRITICAL / WARNING / NOTICE）──┤
//! │   DIAGNOSIS 4        CRITICAL 1 / WARNING 2 / NOTICE 1             │
//! │   integrity.unsatisfied_link（规则 id，等宽）                       │
//! │   结论（目录模板 + 捕获组） / 说明 / Suggested fix / Confidence 85% │
//! │   ─ One-line summary ─ (等宽可复制的一行) [Copy summary]            │
//! ├ 日志行列表（等宽 + 折行 + **独立滚动**，吃满剩余高度）──────────────┤
//! └──────────────────────────────────────────────────────────────────┘
//! ```
//!
//! 数据源只有两个（页面**不读盘**，路径从不进过页面）：
//!
//! | 来源 | 谁造 | 用途 |
//! |---|---|---|
//! | presenter 快照（`log_lines` / `diagnosis` / `log_sessions`） | `app/view/logs.rs::logs_model` | 产品运行时 |
//! | [`demo`](mod@demo) 内嵌的崩溃日志片段 | `pages::demo_model`（`ULL_LOGS_FIXTURE=empty\|live\|crash\|builtin`） | 出图 |
//!
//! ## M95 的取舍（逐条）
//!
//! | # | 取舍 | 为什么 |
//! |---|---|---|
//! | 1 | **折行**而不是横向滚动 | 见 [`lines`] 模块文档：一行 200+ 字符的现实下，横滚与外壳竖向滚动打架，且等宽族的 CJK 回退会毁掉「按列对齐」这个横滚唯一的好处 |
//! | 2 | 诊断卡片在日志列表**之上** | 崩溃时用户先要结论；把结论放在 14 行日志下面等于让人先翻页 |
//! | 3 | 规则原文不再当**数据**画：结论走目录 key（M392） | 界面文案全走 i18n key。M392 之前 `cause.summary` 是 core 直出的中文，英文界面会冒出整句中文；现在 DTO 给 `cause_key` + `groups`，页面按语言从目录取文案（见 [`diagnostic`] 的 `texts_of`） |
//! | 4 | 分组用**真严重度**（Critical/Warning/Info，M394 `SEVGROUP`） | 分组依据是「这毛病有多严重」，取自 core 规则表的 `severity`。M95 r4 到 M394 之间用的是置信度档（那时 DTO 没有严重度，拿置信度硬凑严重度会凑错 12/64 条）；M392 把 `severity` 补进 DTO 后，分组才换得回来。**置信度没删**：它降级成卡片末行的一行标记（见 [`model::Confidence`]） |
//! | 5 | 页头保持**两行 84** | M84 定的版式（运行状态在副标题上）；计数留在两张卡片自己的标题行，避免为了拼计数把 i18n 字符串漏进页头生命期 |
//!
//! ## 指令（只有真业务命令才 dispatch）
//!
//! | 控件 | 消息 | 落到哪 |
//! |---|---|---|
//! | Copy summary | [`Message::CopySummary`] | `app/update.rs` → `iced::clipboard::write` |
//! | Open folder | `Action::OpenLogs` | `app/update/page_messages.rs` → `app/logs_folder::open()` → `core::platform::opener::open_dir`（内部 `child::quiet`） |
//! | Clear | `Action::ClearLogs` | `app/update.rs` → 清空 presenter 的 `log_lines` |
//! | **会话一行**（M453） | [`Message::LogsOpenSession`] | `app/update/page_messages.rs` → `AppCommand::OpenLogSession(id)` → presenter `host/dispatch.rs:119`（读那场的行 + 结论） |
//! | **Refresh**（M453） | [`Message::LogsRefreshSessions`] | 同上 → `AppCommand::RefreshLogSessions` → `host/dispatch.rs:116`（重扫目录 + 发 `LogSessionsChanged`） |
//! | **回到实时**（M462） | [`Message::LogsOpenSession`]（载荷 = 被历史视图挡住那一场） | 同一条链：`AppCommand::OpenLogSession(id)` → `host/mod.rs::open_session` 认出「正在尾随的那一场」（它不在索引里），整份重读后发回 `LogSessionOpened` + `LogLinesChanged` ⇒ presenter 退出历史视图、实时行续上 |
//! | 空态的 CTA | [`Message::Navigate`] | 压一层路由栈回实例页（真导航，不是占位动作） |
//!
//! ## M453：会话选择器（用户 2026-11-20 的成本令）
//!
//! 三件事：①页顶多一块[选择器](sessions) ②两格新指令（上表的 M453 两行）③空/单/多三档版面。
//! 视图形状与三档判据写在 [`sessions`] 的模块文档；**哪些场景只有手动刷新才看得到**写在
//! `app/update/page_messages.rs` 的 `LogsRefreshSessions` 分支与报告里。
//!
//! 这一改动**没有**自动刷新（进页时）：产品导航轨发的是 `Message::Select(NavItem)`
//! （`app/update.rs:171`），落点不在本 mission 的 scope 里；host 已经在三处主动推
//! `LogSessionsChanged`（启动 `refresh_all`、每场启动结束、安装收尾），手动按钮覆盖剩下的。
//!
//! **M386 的订正**：这一格以前写着「还没有 `AppCommand`（报 finding，现在是占位分支）」。那是
//! **过期文档**——M188 已经把这条接到了 `app/logs_folder::open()`（点下去真的会开目录）。M386 做了
//! 两件真事：①把打开器从崩溃弹窗那份裸 `Command::new("explorer")` 换成 core 唯一的平台 opener
//! （走 `child::quiet`，见 `logs_folder::open` 的文档与 `the_logs_folder_opens_through_the_quiet_opener`）；
//! ②把这句话改对。**一句说反了的文档比没有文档更坏**：下一个人会照着「占位分支」去重写一遍。
//!
//! 截图的强弱边界写在 `logs/tests.rs` 每条断言下面。
//!
//! ## M102：页面块吃满页宽 + 单词不再被劈开
//!
//! - 正文块走 [`frame::body`]（左缘 = 页头标题左缘 = 112，宽 = 可用宽度 1044 @1180），
//!   旧版把整页块压成 424 居中，视觉复审 ① 就是它；
//! - 日志行与一行摘要先过 [`super::layout::wrap::breakable`]：在 `.` `_` `:` `/` `\` 后面插零宽空格，
//!   于是 `graphics.gl_operation_failure` 断在 `_` 后面，而不是被按字形劈成 `gl_o` / `peration`。
//!   零宽空格只进显示串，`Line::text` 与 `Logs::summary()`（剪贴板）仍是原文。

mod demo;
mod diagnostic;
mod lines;
mod model;
mod panel;
mod sessions;

#[cfg(test)]
mod diagnostic_tests;
#[cfg(test)]
mod grouping_tests;
#[cfg(test)]
mod sessions_tests;
#[cfg(test)]
mod tests;

/// 诊断卡在正文里的高度份额（见 [`panel`] 的模块文档：两块必须都是定高 + 内部滚动）。
///
/// r2 是 3:2 —— 1180×760 下诊断卡的滚动视口只剩 141px，而一条完整结论（分组标题 → 建议动作 →
/// 置信度）要 177px，于是**首屏看不到任何一条完整结论**（视觉复审 r2 的 P2-A：任务要求的
/// 「建议动作」在验收尺寸下不可见）。改成 4:2：视口 ~184px，一条结论整块可见。
const DIAGNOSIS_SHARE: u16 = 4;
/// 日志卡的高度份额。
const LOGS_SHARE: u16 = 2;

pub use demo::demo;
pub use model::{Confidence, Finding, Level, Line, Logs, Outcome, Severity};
pub use sessions::Session;

use iced::widget::column;
use iced::{Element, Length};

use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_i18n::catalog::{Catalog, Key};
use unlucky_i18n::Lang;
use unlucky_presenter::router::Route;

use crate::app::components::buttons::{self, Button};
use crate::app::components::feedback::empty_state::{empty_state, EmptyState};
use crate::theme::Ui;

use super::frame::{self, Header};
use super::layout::FrameMetrics;
use super::{Action, Message};

/// 整页。
pub fn view<'a>(
    ui: &'a Ui,
    metrics: FrameMetrics,
    logs: &'a Logs,
    lang: Lang,
) -> Element<'a, Message> {
    let catalog = Catalog::of(lang);

    let header = Header::new(catalog.text(Key::NavLogs))
        .subtitle(subtitle(&catalog, logs))
        .action(buttons::text(
            ui,
            Button::new(
                catalog.text(Key::LogsOpenFolder),
                Message::Action(Action::OpenLogs),
            ),
        ))
        .action(buttons::text(
            ui,
            Button::new(
                catalog.text(Key::LogsClear),
                Message::Action(Action::ClearLogs),
            ),
        ));

    let body = if logs.is_empty() {
        // `logs.is_empty()` 只算**行与诊断**：会话选择器在下面照画（M453）——用户来这一页可能就是
        // 为了切到别的一场，而「这一场什么都没有」不该把切走的入口一起藏掉。
        frame::body(metrics, stack(ui, logs, lang, empty(ui, &catalog)))
    } else if logs.findings.is_empty() {
        // 没有结论：日志卡片独吞剩余高度（这样「正常运行的一场」不会留一块空卡片）。
        frame::body(
            metrics,
            stack(ui, logs, lang, lines::card(ui, logs, lang, Length::Fill)),
        )
    } else {
        // 有结论：两块**都定高**、各自独立滚动（诊断 4 : 日志 2）。
        // 定高不是偷懒：第一版让诊断卡按内容撑开，三条结论就把日志卡压成 0 高度（截图见报告）。
        frame::body(
            metrics,
            stack(
                ui,
                logs,
                lang,
                column![
                    diagnostic::card(ui, logs, lang, Length::FillPortion(DIAGNOSIS_SHARE)),
                    lines::card(ui, logs, lang, Length::FillPortion(LOGS_SHARE)),
                ]
                .width(Length::Fill)
                .height(Length::Fill)
                .spacing(ui.space(Space::X4)),
            ),
        )
    };

    frame::page(ui, metrics, header, body)
}

/// 页顶会话选择器 + 下面那几块（M453）。
///
/// 选择器**永远是第一块**：它决定下面每一块画的是哪一场，放在结论下面等于让人先读完一场
/// 才看见「其实还有别的场」。
fn stack<'a>(
    ui: &'a Ui,
    logs: &'a Logs,
    lang: Lang,
    rest: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    column![sessions::card(ui, logs, lang), rest.into()]
        .width(Length::Fill)
        .height(Length::Fill)
        .spacing(ui.space(Space::X4))
        .into()
}

/// 页头副标题：有会话身份（实例 · 时间 · 结局）就显示它，否则退回静态文案。
fn subtitle<'a>(catalog: &Catalog, logs: &'a Logs) -> &'a str {
    logs.status
        .as_deref()
        .unwrap_or_else(|| catalog.text(Key::LogsSubtitleIdle))
}

/// 空态（`feedback::EmptyState` 真组件：图标块 + 标题 + 说明 + 一个 CTA）。
fn empty<'a>(ui: &'a Ui, catalog: &Catalog) -> Element<'a, Message> {
    empty_state(
        ui,
        EmptyState::new(
            IconKind::Logs,
            catalog.text(Key::LogsEmptyTitle),
            catalog.text(Key::LogsEmptyBody),
        )
        .action(
            catalog.text(Key::LogsEmptyAction),
            Message::Navigate(Route::Instances),
        ),
    )
}
