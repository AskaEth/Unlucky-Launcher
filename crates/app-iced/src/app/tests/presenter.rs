//! presenter 闭环与节拍的断言（M90 r2 从 `app/tests.rs` 外置：那边 416 行会被 file-size 判红）。
//!
//! 这里的每条都依赖一个可注入的 `Backend`：
//! - `MockBackend`（presenter 自带）验「命令 → 事件 → 状态」的闭环；
//! - `RecorderBackend`（本文件，只记账不回事件）验「UI 侧不做乐观回显」——用 MockBackend 验这条
//!   会得到**假绿**，因为它是同步发事件的，本地回显与事件回流终态相同（P3-2 的教训）。

use std::cell::RefCell;
use std::rc::Rc;

use unlucky_i18n::Key;
use unlucky_presenter::dto::{AppCommand, AppEvent, Backend, CoreEvent};
use unlucky_presenter::mock_data::mock_instances;

use super::*;

// --------------------------------------------------------------- presenter 闭环

/// **首屏快照在 Boot 就到位**（截图在第 3 帧落盘，等 30 fps 的 tick 会拍进空页面）。
#[test]
fn boot_drains_the_first_snapshot_before_the_first_frame() {
    let mut app = headless(screenshot_options(), SchemeFlavor::Light);
    assert!(app.view.instances.rows.is_empty(), "还没 Boot");

    let _ = app.update(Message::Boot);

    assert_eq!(
        app.view.instances.rows.len(),
        4,
        "mock 的四个实例已进视图模型"
    );
    assert_eq!(app.view.screen, pages::Screen::Instances);
}

/// 首屏自动选中第一行（右栏不该开局空白），且**只做一次**：返回列表后再刷新不弹回详情。
#[test]
fn the_first_snapshot_selects_the_first_instance_once() {
    let mut app = headless(screenshot_options(), SchemeFlavor::Light);
    let _ = app.update(Message::Boot);
    assert_eq!(
        app.state.route(),
        &Route::Detail("survival".to_string()),
        "首屏快照到位后自动选中第一行"
    );

    let _ = app.update(Message::Page(pages::Message::Navigate(Route::Instances)));
    assert_eq!(app.state.route(), &Route::Instances);

    // 再推进一拍（会 drain_events + auto_select）：不该把他弹回详情页
    app.advance(super::super::state::TICK);
    assert_eq!(app.state.route(), &Route::Instances, "自动选中只做一次");
}

/// 只记账、**不回任何事件**的后端（P3-2）。
///
/// 为什么需要它：`MockBackend::dispatch` 是**同步**发事件的，于是「UI 本地回显」与「事件回流」
/// 在 dispatch 之后终态相同——拿 MockBackend 测「不乐观回显」是假绿（r1 就是栽在这条上）。
/// 这个后端把两件事分开：命令只进 `dispatched`，`drain_events` 永远空 ⇒ 状态**不可能**变；
/// 一旦有人在 `update` 里偷偷改本地状态，下面那条断言立刻红（实测见报告「P3-2 取证」）。
#[derive(Debug, Default)]
struct RecorderBackend {
    dispatched: Rc<RefCell<Vec<AppCommand>>>,
    seeded: Vec<AppEvent>,
}

impl Backend for RecorderBackend {
    fn drain_events(&mut self) -> Vec<AppEvent> {
        std::mem::take(&mut self.seeded)
    }

    fn dispatch(&mut self, command: AppCommand) {
        self.dispatched.borrow_mut().push(command);
    }
}

/// 用 recorder 后端造一个 App（首屏快照靠 `seeded` 一次性喂进去）。
fn recording(options: Options, flavor: SchemeFlavor) -> (App, Rc<RefCell<Vec<AppCommand>>>) {
    let recorded = Rc::new(RefCell::new(Vec::new()));
    // 注意：`InstancesChanged` 是**整体替换**，所以四条必须合成一条事件；
    // 分开发的话最后只会剩一个实例（r1 在这条上踩过）。
    let backend = RecorderBackend {
        dispatched: Rc::clone(&recorded),
        seeded: vec![AppEvent::Core(
            CoreEvent::InstancesChanged(mock_instances()),
        )],
    };
    let app = App::with_presenter(
        options,
        FAMILIES,
        flavor,
        Box::new(backend),
        SettingsBridge::memory_only(),
    );
    (app, recorded)
}

/// **P3-2：不做乐观回显**——命令必须真的出 UI 层，而状态在后端回事件之前**一个字节都不许变**。
///
/// 抓得住：`update` 里出现「本地先改状态」的写法（recorder 永远不回事件，本地一改这里就红）。
/// 抓不住：「命令有没有真的到达 host」——那由 `MockBackend` 的闭环测试与真机验证负责。
#[test]
fn launching_only_dispatches_and_never_echoes_locally() {
    let mut options = screenshot_options();
    options.screenshot = None; // recorder 不依赖截图通道
    let (mut app, recorded) = recording(options, SchemeFlavor::Light);

    let _ = app.update(Message::Boot);
    let _ = app.update(Message::Page(pages::Message::Navigate(Route::Detail(
        "survival".to_string(),
    ))));

    // Boot 自己发过一条 RefreshAll，先把账本清掉，下面只看「点 Play」这一个动作
    recorded.borrow_mut().clear();

    let before = app.view.instances.detail.clone().expect("有选中项");
    assert!(before.can_launch && !before.can_stop && before.status.is_none());

    let _ = app.update(Message::Page(pages::Message::Launch(
        "survival".to_string(),
    )));

    // ① 命令真的出 UI 层了
    assert_eq!(
        *recorded.borrow(),
        vec![AppCommand::LaunchInstance("survival".to_string())],
        "Launch 必须翻成一条 AppCommand::LaunchInstance 并交给 Backend::dispatch"
    );

    // ② 后端一条事件都没回 ⇒ 视图模型必须**原样不动**（任何本地回显都会让这条红）
    let after = app.view.instances.detail.clone().expect("有选中项");
    assert_eq!(
        after, before,
        "没有事件回流时页面不许自己变（乐观回显）；现在的差异是 {after:?}"
    );
    assert!(!after.can_stop, "不许因为点了 Play 就本地切成「可停止」");
    assert!(after.status.is_none(), "不许本地造一条进度文案");
}

/// **点 Play → 命令真的派发出去 → 事件回流 → 页面真的变**（真后端的闭环）。
#[test]
fn launching_goes_through_the_backend_and_comes_back_as_events() {
    let mut app = headless(screenshot_options(), SchemeFlavor::Light);
    let _ = app.update(Message::Boot);
    let _ = app.update(Message::Page(pages::Message::Navigate(Route::Detail(
        "survival".to_string(),
    ))));

    let detail = app.view.instances.detail.as_ref().expect("有选中项");
    assert!(
        detail.can_launch && !detail.can_stop,
        "空闲：可启动不可停止"
    );

    let _ = app.update(Message::Page(pages::Message::Launch(
        "survival".to_string(),
    )));

    let detail = app.view.instances.detail.as_ref().expect("有选中项");
    assert!(
        !detail.can_launch && detail.can_stop,
        "派发 LaunchInstance 后后端发回了 Busy 事件：按钮组合必须已经翻转"
    );
    // 阶段名跟界面语言走（M120）：断言对着**当前生效语言**的目录算，别在中文机器上钉死英文
    let stage = app.lang().catalog().text(Key::InstancesStageDownload);
    assert_eq!(
        detail.status.as_deref(),
        Some(format!("{stage} · 0%").as_str()),
        "状态行来自事件里的进度，不是本地假造"
    );
}

/// 时间推进真的把状态往前推（后端的脚本按 tick 的累计时长走）。
#[test]
fn ticks_advance_the_backend_script() {
    let mut app = headless(screenshot_options(), SchemeFlavor::Light);
    let _ = app.update(Message::Boot);
    let _ = app.update(Message::Page(pages::Message::Navigate(Route::Detail(
        "survival".to_string(),
    ))));
    let _ = app.update(Message::Page(pages::Message::Launch(
        "survival".to_string(),
    )));

    // 33 ms × 50 ≈ 1.65 s：mock 的脚本在 1.5 s 从 Download 切到 Verify
    for _ in 0..50 {
        app.advance(super::super::state::TICK);
    }

    let detail = app.view.instances.detail.as_ref().expect("有选中项");
    let status = detail.status.as_deref().unwrap_or_default();
    let catalog = app.lang().catalog();
    let (verify, launch) = (
        catalog.text(Key::InstancesStageVerify),
        catalog.text(Key::InstancesStageLaunch),
    );
    assert!(
        status.starts_with(verify) || status.starts_with(launch),
        "50 拍之后应已过下载阶段，实际 {status:?}"
    );
}

/// 停止也是真命令：`StopInstance` 让 mock 直接回一条 Idle 事件。
#[test]
fn stopping_returns_to_idle_through_an_event() {
    let mut app = headless(screenshot_options(), SchemeFlavor::Light);
    let _ = app.update(Message::Boot);
    let _ = app.update(Message::Page(pages::Message::Navigate(Route::Detail(
        "survival".to_string(),
    ))));
    let _ = app.update(Message::Page(pages::Message::Launch(
        "survival".to_string(),
    )));
    let _ = app.update(Message::Page(pages::Message::Stop("survival".to_string())));

    let detail = app.view.instances.detail.as_ref().expect("有选中项");
    assert!(detail.can_launch && !detail.can_stop, "停下来回到空闲");
    assert!(detail.status.is_none(), "空闲没有状态行");
}

/// 导航：一级页替换、详情压栈、返回出栈——页面换的是 `Route`，不是页面自己的选中项。
#[test]
fn navigation_follows_the_presenter_router() {
    let mut app = headless(screenshot_options(), SchemeFlavor::Light);
    let _ = app.update(Message::Boot);

    let _ = app.update(Message::Page(pages::Message::Navigate(Route::Detail(
        "survival".to_string(),
    ))));
    assert_eq!(app.view.screen, pages::Screen::Instances, "详情仍是实例页");
    assert!(app.state.router.can_go_back());

    let _ = app.update(Message::Page(pages::Message::Navigate(Route::Settings)));
    assert_eq!(app.view.screen, pages::Screen::Settings);
    assert_eq!(
        app.view.active,
        Some(pages::Screen::Settings.nav()),
        "导航轨高亮跟着路由走"
    );
    assert!(
        !app.state.router.can_go_back(),
        "一级页互相替换，不进栈（presenter::router 的语义）"
    );

    let _ = app.update(Message::Page(pages::Message::Navigate(Route::Instances)));
    assert_eq!(app.view.screen, pages::Screen::Instances);
}
