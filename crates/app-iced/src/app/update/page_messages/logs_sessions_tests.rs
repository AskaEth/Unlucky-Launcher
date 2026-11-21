//! M453：日志页两条会话意图**真的出了 UI 层**——判据看的是 `AppCommand` 队列的内容。
//!
//! ## 为什么断言队列内容而不是「有个可点的行」
//!
//! 只看「渲染了一个按钮」或「消息构造出来了」的用例，在把 id 传成空串、写死第一个 id、或者
//! `on_page` 把两条意图接反时**照样全绿**——那正是本 mission 要防的「看着像派发了」。
//! 所以这里走的是产品路径（`App::on_page` → `App::dispatch`），断的是后端收到的 `AppCommand`：
//! 内容相等（含 id）以外没有第二种通过方式。
//!
//! 用**只记账、不回事件**的后端（同 `app/tests/presenter.rs` / `app/update/download_wiring.rs`）：
//! `MockBackend` 会同步回 `LogSessionsChanged`，于是「本地乐观回显」与「事件回流」终态相同，
//! 「状态没有提前变」这一类断言会**假绿**。

use std::cell::RefCell;
use std::rc::Rc;

use unlucky_presenter::dto::{AppCommand, AppEvent, Backend};
use unlucky_presenter::settings_bridge::SettingsBridge;

use crate::app::state::App;
use crate::cli::Options;
use crate::pages;
use crate::theme::Families;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

/// 只记账的后端。
struct Recorder(Rc<RefCell<Vec<AppCommand>>>);

impl Backend for Recorder {
    fn drain_events(&mut self) -> Vec<AppEvent> {
        Vec::new()
    }

    fn dispatch(&mut self, command: AppCommand) {
        self.0.borrow_mut().push(command);
    }
}

/// 不碰磁盘的 App（mock 数据 + 内存设置桥），与 `page_messages.rs` 的 `headless` 同型。
fn headless(log: &Rc<RefCell<Vec<AppCommand>>>) -> App {
    App::with_presenter(
        Options::default(),
        FAMILIES,
        unlucky_design::color::SchemeFlavor::Light,
        Box::new(Recorder(Rc::clone(log))),
        SettingsBridge::memory_only(),
    )
}

/// 点某一场 ⇒ `AppCommand::OpenLogSession(该 id)`。
///
/// **抓得住**：①两条意图没有落到 `AppCommand`（点了没反应，编译器也不报——`Action(action)`
/// 那条占位分支就在旁边）；②id 传成空串 / 写死成第一场（第二段断言两场的 id 不同且各归各位）；
/// ③把 `OpenLogSession` 接成 `RefreshLogSessions`（第二条命令的内容对不上）。
/// **抓不住**：host 收到之后真的读到了那一场的行与结论（那是 presenter
/// `host::logs::open` 的事，本 crate 的 `MockBackend::dispatch` 里它对 `OpenLogSession` 只记一笔）。
#[test]
fn clicking_a_session_dispatches_open_log_session_with_that_id() {
    let log: Rc<RefCell<Vec<AppCommand>>> = Rc::new(RefCell::new(Vec::new()));
    let mut app = headless(&log);

    let _ = app.on_page(pages::Message::LogsOpenSession("s-2".to_string()));
    assert_eq!(
        *log.borrow(),
        vec![AppCommand::OpenLogSession("s-2".to_string())],
        "点一场必须派出一条同名命令，且 id 原样带走"
    );

    // 换一场：id 必须跟着行走（写死 / 空串的实现在这一条红）。
    let _ = app.on_page(pages::Message::LogsOpenSession("s-1".to_string()));
    assert_eq!(
        *log.borrow(),
        vec![
            AppCommand::OpenLogSession("s-2".to_string()),
            AppCommand::OpenLogSession("s-1".to_string()),
        ],
        "第二场的 id 必须来自消息载荷，不是第一场或空串"
    );

    assert!(
        app.watch_until.is_some(),
        "命令必须经过 `App::dispatch`（那里开观察窗等 host 回事件）；直接改本地状态不会有它"
    );
}

/// 点 Refresh ⇒ `AppCommand::RefreshLogSessions`，且**不动**本地日志行。
///
/// **抓得住**：①刷新没落到命令（按钮点下去只打一行日志）；②把刷新接成 `OpenLogSession`
/// （内容不符）；③顺手 `log_lines.clear()` 这类本地回显——刷新的语义是「重扫会话目录」，
/// 不是「清屏」，日志正文要等 host 的 `LogLinesChanged` 回来才换。
/// **抓不住**：host 重扫到了哪些会话（那是 `presenter::host::logs::sessions` 的读盘行为）；
/// 也抓不住「自动刷新」——本 mission 没做进页自动刷，理由写在
/// `app/update/page_messages.rs` 的 `LogsRefreshSessions` 分支。
#[test]
fn refresh_dispatches_refresh_log_sessions_and_does_not_touch_the_lines() {
    let log: Rc<RefCell<Vec<AppCommand>>> = Rc::new(RefCell::new(Vec::new()));
    let mut app = headless(&log);
    let before = app.state.log_sessions.len();

    let _ = app.on_page(pages::Message::LogsRefreshSessions);
    assert_eq!(
        *log.borrow(),
        vec![AppCommand::RefreshLogSessions],
        "刷新必须派出一条 RefreshLogSessions"
    );
    assert!(
        app.state.log_lines.is_empty(),
        "刷新不许本地清空 / 伪造日志行（要等 host 的 LogLinesChanged）"
    );
    assert_eq!(
        app.state.log_sessions.len(),
        before,
        "会话快照只能由后端的 `LogSessionsChanged` 改（recorder 一条事件都不回）"
    );
    assert!(
        app.watch_until.is_some(),
        "命令必须经过 `App::dispatch`（观察窗）"
    );
}
