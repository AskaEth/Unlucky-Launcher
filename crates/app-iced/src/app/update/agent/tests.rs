//! **树测试①（M119 验收核心）**：AI 右栏在**真实 presenter 状态**下渲染。
//!
//! 走的是同一条用户路径：`Message::Agent(Submit)` → `AgentHost::submit` → `Presenter::agent_submit`
//! → transport → 事件队列 → `Message::Agent(Events)` → `Presenter::apply` → `AgentRail`。
//! **没有一条断言看本地数组**：`transcript` 这类壳自己的会话副本 M119 已经删了，能断言的只剩
//! presenter 的 rail，而 rail 正是右栏画的东西（`pages/agent::spec` 从它借）。
//!
//! 覆盖任务书点名的四件事：
//!
//! | 断言 | 抓得住什么 |
//! |---|---|
//! | 提交后 rail 出现「你 + 助手」两条、正文来自事件的流式增量 | 提交没接到 presenter、或右栏还在画样本会话 |
//! | `busy` 翻真 → 收敛后翻假 | 事件没回流、或流式标记没收敛（输入框会永远禁用） |
//! | 标记已读后未读归零 | `agent_mark_read` 没接、或未读口径没跟着 rail |
//! | 三种失败都给**具名**横幅 | 失败被吞掉（M104 的「静默无语」）|

use iced::widget::text_editor;

use unlucky_design::color::SchemeFlavor;
use unlucky_i18n::{Catalog, Key, Lang};
use unlucky_presenter::dto::{AgentEvent, AppEvent, ChatRole};
use unlucky_presenter::mock::MockBackend;
use unlucky_presenter::settings_bridge::SettingsBridge;

use iced::keyboard::key::{Code, Named, Physical};
use iced::keyboard::{key, Location, Modifiers};

use crate::app::state::presenter::transport::Script;
use crate::app::state::presenter::AgentHost;
use crate::app::Message;
use crate::cli::Options;
use crate::shell::agent::{Focus, Message as AgentMessage, RailState};
use crate::theme::Families;

use super::App;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

/// 不碰磁盘的 App（mock 后端 + 内存设置桥 + **本脚本的** presenter 句柄）。
fn app_with(script: Script) -> App {
    let mut app = App::with_presenter(
        Options::default(),
        FAMILIES,
        SchemeFlavor::Light,
        Box::new(MockBackend::new()),
        SettingsBridge::memory_only(),
    );
    app.agent_host = AgentHost::with_script(script);
    // 换句柄 = 可见性还没写回去；`refresh_view` 是那条单点同步
    app.refresh_view();
    app
}

/// 走真消息提交（用户路径：填草稿 → 点发送）。
fn submit(app: &mut App, text: &str) {
    app.agent.draft = text_editor::Content::with_text(text);
    let _ = app.update(Message::Agent(AgentMessage::Submit));
}

/// **提交 → 事件回流 → 右栏出现这一轮 → 标记已读归零**（任务书第 2 条）。
///
/// 抓得住：①`Submit` 没接 `agent_submit`（rail 空）；②事件没经 Message 通道回流（rail 只有半截）；
/// ③`expand` 没调 `agent_mark_read`（未读不清）；④右栏读的还是壳自己的副本。
/// 抓不住：气泡在像素上长什么样（那要出图，见 `DevDocs/artifacts/m119/`）。
#[test]
fn a_submitted_turn_lands_in_the_rail_and_reading_it_zeroes_unread() {
    let mut app = app_with(Script::Qa);
    // 出厂是折叠档 ⇒ presenter 认为「用户没在看着」，到达的助手消息应计未读
    assert_eq!(app.agent.rail, RailState::Collapsed);
    assert_eq!(app.agent_host.rail().unread, 0, "空会话没有未读");

    submit(&mut app, "Which instance should I start?");

    let rail = app.agent_host.rail();
    assert_eq!(
        rail.messages.len(),
        2,
        "一条「你」+ 一条助手（事件产生，不是本地回显）"
    );
    assert_eq!(rail.messages[0].role, ChatRole::User);
    assert_eq!(rail.messages[0].text, "Which instance should I start?");
    assert_eq!(rail.messages[1].role, ChatRole::Assistant);
    assert!(
        rail.messages[1].text.contains("Fabric"),
        "正文来自流式增量：{:?}",
        rail.messages[1].text
    );
    assert!(!rail.messages[1].streaming, "Qa 脚本会收敛");
    assert!(!rail.busy, "收敛之后不忙（发送键要能再按）");
    assert_eq!(rail.unread, 1, "折叠态到达的一条助手消息 = 一条未读");

    // 右栏画的正是这份 rail（`pages/agent::spec` 直接借它）——构图不 panic
    let _ = app.view();

    // 打开栏 = 读过：`agent_mark_read`
    let _ = app.update(Message::Agent(AgentMessage::Expand));
    assert_eq!(app.agent_host.rail().unread, 0, "标记已读后未读归零");
    assert_eq!(app.agent_host.rail().messages.len(), 2, "已读不改会话");
}

/// **busy 正确翻转**：流式中忙 → 后端收敛后不忙。
///
/// 抓得住：`busy` 还由壳自己算（那它不会跟 rail 走）；`Events` 通道被当成「只追加消息」而丢掉
/// `MessageFinished`（输入框会永远禁用——这是最要命的坏法之一）。
#[test]
fn busy_flips_with_the_backend_events() {
    let mut app = app_with(Script::Streaming);
    submit(&mut app, "hello");
    assert!(app.agent_host.rail().busy, "流式没收敛前输入要禁用");

    // 后端收敛：一条 `MessageFinished` 经同一条 Message 通道回流
    let index = app.agent_host.rail().messages.len() - 1;
    let _ = app.update(Message::Agent(AgentMessage::Events(vec![AppEvent::Agent(
        AgentEvent::MessageFinished { index },
    )])));
    assert!(!app.agent_host.rail().busy, "收敛之后必须能再发");
}

/// 写操作确认：rail 上有待确认 ⇒ 忙；裁决后收敛并记一条审计。
#[test]
fn a_pending_write_op_is_visible_and_decidable() {
    let mut app = app_with(Script::WriteOp);
    submit(&mut app, "把下载并发改小");

    let rail = app.agent_host.rail();
    let request = rail.confirm.as_ref().expect("要挂一个待确认请求");
    assert_eq!(request.tool, "set_config");
    assert!(
        rail.busy,
        "等确认也算忙（否则用户能在一轮写操作中间再发一句）"
    );
    let _ = app.view();

    let _ = app.update(Message::Agent(AgentMessage::Confirm(true)));
    assert!(app.agent_host.rail().confirm.is_none(), "裁决后清空");
    assert!(!app.agent_host.rail().busy);
    assert_eq!(app.agent_host.audit_len(), 1, "裁决要进审计（YOLO 下也是）");
}

/// 失败态①：**AI 端点不可达** ⇒ 具名横幅（不是静默，也不是把后端原文糊上屏）。
#[test]
fn an_unreachable_endpoint_shows_a_named_error() {
    let mut app = app_with(Script::Unreachable);
    submit(&mut app, "hello");

    let failure = app.agent_failure().expect("提交失败必须给横幅");
    // 文案跟着**生效语言**走（`LangState` 在这台机器上探到 zh-CN，硬编码英文会假红）
    let effective = app.lang().catalog();
    assert_eq!(failure.title, Key::AgentErrorTitle, "标题是 catalog 键");
    assert_eq!(failure.body, effective.text(Key::AgentErrorUnavailable));
    assert!(!failure.body.trim().is_empty());
    assert!(
        app.agent_host.rail().messages.is_empty(),
        "失败不产生消息：不许假装有回复"
    );
    let _ = app.view();
}

/// 失败态②：**未实现路由** ⇒ 另一句具名错误（两种失败不能糊成一句）。
#[test]
fn an_unimplemented_route_shows_its_own_named_error() {
    let mut app = app_with(Script::NotImplemented);
    submit(&mut app, "hello");

    let failure = app.agent_failure().expect("必须给横幅");
    let effective = app.lang().catalog();
    assert_eq!(failure.body, effective.text(Key::AgentErrorNotImplemented));
    // 两条原因码要两句话（中英两套都得分开），不是一句万能错误
    for lang in [Lang::En, Lang::ZhCn] {
        let catalog = Catalog::of(lang);
        assert_ne!(
            catalog.text(Key::AgentErrorNotImplemented),
            catalog.text(Key::AgentErrorUnavailable),
            "{} 下两条原因码糊成一句了",
            lang.tag()
        );
    }
}

/// 失败态③：**这一轮回复中途断流** ⇒ 横幅用 presenter 已脱敏的那句原因，已到的增量保留。
#[test]
fn a_turn_level_failure_keeps_the_partial_answer_and_the_reason() {
    let mut app = app_with(Script::FailsMidStream);
    submit(&mut app, "读一下日志");

    let rail = app.agent_host.rail();
    // 用户 + 助手（流式占位）+ 系统条目（`Failed` 会额外追加一条，错误也要能回翻）
    assert_eq!(rail.messages.len(), 3, "断流不撤销已到的增量");
    assert!(
        rail.messages[1].text.contains("日志"),
        "已到的增量要留着：{:?}",
        rail.messages[1].text
    );
    assert!(!rail.busy, "断流要收敛掉流式标记");
    let failure = app.agent_failure().expect("`Failed` 事件要立横幅");
    assert!(
        failure.body.contains("read timeout"),
        "横幅正文是 presenter 脱敏后的原因：{}",
        failure.body
    );
}

/// 输入框的草稿会同步给 presenter（切形态不丢的语义在 presenter 那边）。
/// **P2-1**：默认配置下后端是脚本化 rig ⇒ 视图必须知道「这是模拟」（界面据此常驻标记）。
///
/// 抓得住：`is_simulated` 被写死成 `false`、或者视图拿不到这个事实（用户就会以为模型接上了）。
/// 抓不住：标记在像素上长什么样（那要出图，见 `DevDocs/artifacts/m119/08-*`）。
#[test]
fn the_default_backend_reports_itself_as_simulated() {
    let app = app_with(Script::Qa);
    assert!(
        app.agent_host.is_simulated(),
        "脚本化 rig 必须自报「模拟」，否则界面就能假装模型接上了"
    );
    // 空会话（最容易被误读成一屏）也要能拿到：视图入参 `Session.simulated` 就是它
    let session = crate::shell::agent::Session {
        agent: &app.agent,
        rail: app.agent_host.rail(),
        failure: app.agent_failure(),
        simulated: app.agent_host.is_simulated(),
    };
    assert!(session.simulated);
}

#[test]
fn typing_syncs_the_draft_into_the_presenter() {
    let mut app = app_with(Script::Qa);
    app.agent.draft = text_editor::Content::with_text("half typed");
    let _ = app.update(Message::Agent(AgentMessage::Edit(
        text_editor::Action::Edit(text_editor::Edit::Insert('!')),
    )));
    assert_eq!(
        app.agent_host.rail().draft,
        app.agent.draft.text(),
        "编辑器里是什么，rail 上就该是什么（同一个串，不是它自己的副本）"
    );
    assert!(app.agent_host.rail().draft.contains('!'), "改动真的进去了");
    // 发送成功后草稿两边都清空
    let _ = app.update(Message::Agent(AgentMessage::Submit));
    assert!(app.agent.draft_is_blank(), "编辑器清空");
    assert!(app.agent_host.rail().draft.is_empty(), "presenter 侧也清空");
}

/// 手搓一条按键事件（订阅层搬过来的就是它；`on_agent_key` 只看 `KeyPressed`）。
fn press(named: key::Named) -> iced::keyboard::Event {
    let key = key::Key::Named(named);
    iced::keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: Physical::Code(Code::Enter),
        location: Location::Standard,
        modifiers: Modifiers::default(),
        text: None,
        repeat: false,
    }
}

/// **P2-2 的回归闸**：不碰鼠标、只用键盘消息，能完成一次写操作确认。
///
/// 为什么这条重要：确认条原本只有鼠标路径，而待确认时 `busy` 锁着发送、`Stop` 又走主后端
/// （清不掉 presenter 的 pending）——键盘用户会在这一步**永久卡死**。
///
/// 抓得住：①焦点没主动落到确认条（`Enter` 够不着）；②`Enter` / `Space` 没映射成 `Activate`；
/// ③`Tab` 环里没有确认条那两格；④裁决完焦点没收回输入框（下一句话打不进去）。
/// 抓不住：焦点环在像素上的样子（那要出图，见 `DevDocs/artifacts/m119/09-*`）。
#[test]
fn a_blocking_confirm_can_be_decided_with_the_keyboard_alone() {
    let mut app = app_with(Script::WriteOp);
    submit(&mut app, "把下载并发改小");
    assert!(
        app.agent_host.rail().confirm.is_some(),
        "先要有一个待确认请求"
    );
    // ① 焦点主动落在「允许」（阻塞态），但**键盘闸不替用户打开**（复审 r2 / P2-2）
    assert_eq!(
        app.agent.focus,
        Focus::ConfirmAllow,
        "焦点要落在可操作元素上"
    );
    assert!(
        !app.agent.keyboard_mode,
        "塔台 r2：不许自动 arm——自动 arm 会让用户随手敲的空格变成「批准」"
    );

    // ② 第一次 Tab = 「我要用键盘」的表态：只点亮焦点环，**不移动**焦点
    let _ = app.update(Message::Agent(AgentMessage::Tab { shift: false }));
    assert!(app.agent.keyboard_mode, "Tab 之后才算 armed");
    assert_eq!(
        app.agent.focus,
        Focus::ConfirmAllow,
        "arming 的那一次不移动焦点（否则 Enter 会反着裁决）"
    );

    // ③ Enter = 允许
    let _ = app.update(Message::Agent(AgentMessage::Key(press(Named::Enter))));
    assert!(
        app.agent_host.rail().confirm.is_none(),
        "Enter 必须真的裁决掉"
    );
    assert_eq!(app.agent_host.audit_len(), 1, "裁决进审计");
    assert_eq!(app.agent.focus, Focus::Composer, "裁决完焦点收回输入框");
    assert!(!app.agent.keyboard_mode, "阻塞态结束就不再亮焦点环");

    // ④ 第二轮走「Tab 点环 → Tab 走到拒绝 → Space 触发」：键盘用户不必背 Enter 是哪一个
    submit(&mut app, "那把并发改回来");
    assert_eq!(app.agent.focus, Focus::ConfirmAllow);
    let _ = app.update(Message::Agent(AgentMessage::Tab { shift: false }));
    let _ = app.update(Message::Agent(AgentMessage::Tab { shift: false }));
    assert_eq!(app.agent.focus, Focus::ConfirmDeny, "Tab 要能走到「拒绝」");
    let _ = app.update(Message::Agent(AgentMessage::Key(press(Named::Space))));
    assert!(app.agent_host.rail().confirm.is_none(), "Space 也要能裁决");
    assert_eq!(app.agent_host.audit_len(), 2);
}

/// **复审 r2 / P2-2 的反向回归**：没 armed 的时候，空格与回车都不许裁决写操作。
///
/// 复审用临时用例复现的路径：WriteOp 提交后（r1 无条件 arm），用户接着打下一句，第一个空格就成了
/// 「允许」。真实场景里编辑器仍持 iced 焦点，所以这条不是理论问题。
///
/// 抓得住：谁把 `sync_confirm_focus` 里的 `keyboard_mode = true` 加回来；谁把 `Space`/`Enter` 的
/// `Activate` 绕到 `keyboard_mode` 闸之外。
/// 抓不住：iced 组件树里空格到底有没有进输入框（那要真窗口，出图另算）——这里只断言「没裁决」。
#[test]
fn a_stray_space_or_enter_never_approves_a_pending_write() {
    let mut app = app_with(Script::WriteOp);
    submit(&mut app, "把下载并发改小");
    assert!(app.agent_host.rail().confirm.is_some());
    assert!(!app.agent.keyboard_mode, "待确认不该自动 arm");

    // 用户接着打下一句：先敲一个字符（编辑器自己消化），再敲空格
    let _ = app.update(Message::Agent(AgentMessage::Edit(
        text_editor::Action::Edit(text_editor::Edit::Insert('a')),
    )));
    let _ = app.update(Message::Agent(AgentMessage::Key(press(Named::Space))));
    assert!(
        app.agent_host.rail().confirm.is_some(),
        "未 armed 时空格不许批准写操作"
    );
    assert_eq!(app.agent_host.audit_len(), 0, "也不许留下审计");

    // 回车同理：未 armed 时它归编辑器（发送键），不归确认条
    let _ = app.update(Message::Agent(AgentMessage::Key(press(Named::Enter))));
    assert!(
        app.agent_host.rail().confirm.is_some(),
        "未 armed 时回车不许批准写操作"
    );
    assert_eq!(app.agent_host.audit_len(), 0);
}
