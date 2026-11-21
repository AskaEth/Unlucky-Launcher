//! 「启动参数保存失败」的**整环**回归（M157 r4 复审 P3-a）。
//!
//! 仓里原来只有这条环的两半：`LaunchForm::sync` 的单测（本 crate 的 `pages/instances/state.rs`）
//! 与「Save 只派发不清脏」（`app/update/instances.rs`）。两半各自绿，但**接线**（消息 →
//! `dispatch` → 后端回事件 → `pump` → `refresh_view` → `sync_launch_form`）没有任何活着的断言。
//! 把 `dirty = false` 提前到派发前，只有这条整环测试会红（单测两半都不会）。
//!
//! 造的后端与真 host 的失败路径同形：收到 `SetInstanceLaunchArgs` 只回一条 Toast（错误的用户
//! 可见反馈），**不回** `InstancesChanged`——盘上没改，快照还是旧的。

use std::cell::RefCell;
use std::rc::Rc;

use unlucky_design::color::SchemeFlavor;
use unlucky_presenter::dto::{
    AppCommand, AppEvent, Backend, CoreEvent, InstanceRuntime, InstanceSummary, LaunchArgsView,
    LoaderInfo, LoaderKind,
};
use unlucky_presenter::router::Route;
use unlucky_presenter::settings_bridge::SettingsBridge;
use unlucky_presenter::toast::{Toast, ToastKind};

use crate::app::message::Message;
use crate::app::state::{App, TICK};
use crate::cli::Options;
use crate::pages;
use crate::theme::Families;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

/// 失败路径的后端：命令进台账；`SetInstanceLaunchArgs` 只回一条错误 Toast。
#[derive(Debug, Default)]
struct FailingSaveBackend {
    dispatched: Rc<RefCell<Vec<AppCommand>>>,
    queued: Vec<AppEvent>,
}

impl Backend for FailingSaveBackend {
    fn drain_events(&mut self) -> Vec<AppEvent> {
        std::mem::take(&mut self.queued)
    }

    fn dispatch(&mut self, command: AppCommand) {
        self.dispatched.borrow_mut().push(command.clone());
        if matches!(command, AppCommand::SetInstanceLaunchArgs { .. }) {
            // 真 host 在 `dispatch_inner` 的 Err 分支只发 toast（不发 InstancesChanged）。
            self.queued.push(AppEvent::Toast(Toast {
                id: 0,
                kind: ToastKind::Error,
                text: "内存超出范围".to_string(),
                remaining_ms: 0,
            }));
        }
    }
}

/// 盘上的实例快照：内存 4096、其余留空（= 保存失败时 pump 会带回来的那份）。
fn snapshot() -> Vec<InstanceSummary> {
    vec![InstanceSummary {
        id: "survival".to_string(),
        name: "生存".to_string(),
        mc_version: "1.20.4".to_string(),
        loader: LoaderInfo {
            kind: LoaderKind::Vanilla,
            version: String::new(),
        },
        last_played: None,
        runtime: InstanceRuntime::Idle,
        icon: None,
        isolation: "isolated".to_string(),
        version_id: "1.20.4".to_string(),
        version_present: true,
        launch: LaunchArgsView {
            instance_id: "survival".to_string(),
            memory_mb: Some(4096),
            memory_effective_mb: 4096,
            ..LaunchArgsView::default()
        },
    }]
}

fn app_with_snapshot() -> (App, Rc<RefCell<Vec<AppCommand>>>) {
    let dispatched = Rc::new(RefCell::new(Vec::new()));
    let backend = FailingSaveBackend {
        dispatched: Rc::clone(&dispatched),
        queued: vec![AppEvent::Core(CoreEvent::InstancesChanged(snapshot()))],
    };
    let app = App::with_presenter(
        Options::default(),
        FAMILIES,
        SchemeFlavor::Light,
        Box::new(backend),
        SettingsBridge::memory_only(),
    );
    (app, dispatched)
}

/// **保存失败不许吃掉草稿**（整环）：输入 `4g` → 点保存 → 后端回错误事件 → pump 重建视图。
///
/// 抓得住：①`Save` 在派发前清脏（草稿被旧快照覆盖成 4096）；②`LaunchForm::sync` 去掉
/// `matches` 判据（同样覆盖）；③`pump` 没把事件喂进 `apply`/`refresh_view`（表单不会被播种，
/// 下面的前置断言会红）。
/// 抓不住：真 host 的落盘失败（那是 `host::launch_args` 的单测射程）。
#[test]
fn failed_save_keeps_the_draft_through_the_whole_loop() {
    let (mut app, dispatched) = app_with_snapshot();

    // 首屏快照到位 → 自动选中 → 表单按盘上的值播种。
    let _ = app.update(Message::Boot);
    let _ = app.update(Message::Page(pages::Message::Navigate(Route::Detail(
        "survival".to_string(),
    ))));
    app.advance(TICK);
    assert_eq!(app.launch_form.memory, "4096", "首帧要按快照播种");
    assert!(!app.launch_form.dirty);

    // 用户把内存改成非法值（host 会拒），然后点保存。
    let _ = app.update(Message::Page(pages::Message::Instances(
        pages::InstanceMessage::Memory("4g".to_string()),
    )));
    assert_eq!(app.launch_form.memory, "4g");
    let _ = app.update(Message::Page(pages::Message::Instances(
        pages::InstanceMessage::Save,
    )));
    assert!(
        dispatched
            .borrow()
            .iter()
            .any(|command| matches!(command, AppCommand::SetInstanceLaunchArgs { .. })),
        "保存要把命令派发出去"
    );

    // 失败回程：一条错误 Toast → pump → refresh_view → sync_launch_form（旧快照）。
    app.advance(TICK);

    assert_eq!(
        app.launch_form.memory, "4g",
        "保存失败后，pump 用旧快照把用户刚敲的内容冲掉了"
    );
    assert!(app.launch_form.dirty, "草稿还在，脏标记也不能掉");
}
