//! 主按钮 / Esc 的落点判据（M247，复审 r1 P1-1 / P2-1）：**接线**层，不是 presenter 的状态机。
//! 一个 App + 一个只记账的后端，看"点了主按钮之后下发了什么"——「盘上的世界还在」在这里等价于
//! 「没有下发 `DeleteWorld`」（app 不写盘，唯一写入口是 `host::saves::dispatch`）。
//! 真实 `export()` 写没写 zip 由 `app/dialog/tests.rs` 的夹具覆盖，这里不造崩溃卡片。

use std::cell::RefCell;
use std::rc::Rc;

use unlucky_design::color::SchemeFlavor;
use unlucky_presenter::dialogs::{DialogKind, DialogSource};
use unlucky_presenter::dto::{
    AppCommand, AppEvent, Backend, CoreEvent, SavesCommand, SavesListView, WorldView,
};
use unlucky_presenter::mock_data::mock_instances;
use unlucky_presenter::router::Route;
use unlucky_presenter::settings_bridge::SettingsBridge;

use super::{escape, on_dialog};
use crate::app::dialog::Message as DialogMessage;
use crate::app::message::Message;
use crate::app::state::App;
use crate::cli::Options;
use crate::pages;
use crate::theme::Families;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};
const DIR: &str = "Better World";

/// 只记账、按需喂事件的后端（与 `app/tests/presenter.rs` 的 recorder 同款）。
#[derive(Debug, Default)]
struct SpyBackend {
    dispatched: Rc<RefCell<Vec<AppCommand>>>,
    events: Rc<RefCell<Vec<AppEvent>>>,
}

impl Backend for SpyBackend {
    fn drain_events(&mut self) -> Vec<AppEvent> {
        std::mem::take(&mut *self.events.borrow_mut())
    }

    fn dispatch(&mut self, command: AppCommand) {
        self.dispatched.borrow_mut().push(command);
    }
}

/// 一个进到详情页、可喂事件、可记账的 App。
struct Harness {
    app: App,
    dispatched: Rc<RefCell<Vec<AppCommand>>>,
    events: Rc<RefCell<Vec<AppEvent>>>,
}

impl Harness {
    fn new() -> Self {
        let dispatched = Rc::new(RefCell::new(Vec::new()));
        let events = Rc::new(RefCell::new(vec![AppEvent::Core(
            CoreEvent::InstancesChanged(mock_instances()),
        )]));
        let backend = SpyBackend {
            dispatched: Rc::clone(&dispatched),
            events: Rc::clone(&events),
        };
        let mut harness = Self {
            app: App::with_presenter(
                Options::default(),
                FAMILIES,
                SchemeFlavor::Light,
                Box::new(backend),
                SettingsBridge::memory_only(),
            ),
            dispatched,
            events,
        };
        let _ = harness.app.update(Message::Boot);
        assert_eq!(
            harness.app.state.route(),
            &Route::Detail("survival".to_string()),
            "首屏该落在详情页（后面每一步都以它为前提交付目标）"
        );
        harness
    }

    /// 喂一条事件再抽干（`pump` 是唯一的 apply 入口，它顺带 `refresh_view`）。
    fn feed(&mut self, event: AppEvent) {
        self.events.borrow_mut().push(event);
        self.app.pump();
        self.app.refresh_view();
    }

    /// 点一次「删除 X」（页面消息的真实入口）。
    fn request_delete(&mut self) {
        let _ = self.app.update(Message::Page(pages::Message::Instances(
            pages::InstanceMessage::RequestDeleteWorld {
                dir: DIR.to_string(),
            },
        )));
    }

    /// 有没有下发过 `SavesCommand::DeleteWorld`（= core 会不会去删盘上的世界）。
    fn deleted(&self) -> bool {
        self.dispatched.borrow().iter().any(|command| {
            matches!(
                command,
                AppCommand::Saves(command)
                    if matches!(**command, SavesCommand::DeleteWorld { .. })
            )
        })
    }
}

/// 一份 `saves/` 快照（`worlds` 决定"那个世界还在不在"）。
fn snapshot(worlds: &[&str]) -> AppEvent {
    AppEvent::Core(CoreEvent::Saves(Box::new(Ok(SavesListView {
        instance: "survival".to_string(),
        dir: "C:/mc/saves".to_string(),
        exists: true,
        worlds: worlds
            .iter()
            .map(|dir| WorldView {
                dir: (*dir).to_string(),
                icon: false,
                modified_ms: None,
            })
            .collect(),
        screenshots_dir: "C:/mc/screenshots".to_string(),
        screenshots_exists: false,
        screenshots: Vec::new(),
        skipped: Vec::new(),
        last_op: None,
    }))))
}

/// **P1-1 事故链**：删存档 → `Esc` → 一个不相干的 `Confirm`（崩溃卡片的主按钮是「导出诊断包」）
/// → 点主按钮 ⇒ **没有** `DeleteWorld` 下发，框走它自己那条路（这里没有崩溃卡片 ⇒ 关框）。
#[test]
fn escaping_the_delete_dialog_disarms_it_for_the_next_one() {
    let mut harness = Harness::new();
    harness.request_delete();
    assert!(
        harness.app.state.dialogs.current().is_some(),
        "确认框没弹出来"
    );
    assert!(harness.app.state.saves.confirm.is_some(), "目标没登记");

    assert!(escape(&mut harness.app), "有框时 Esc 该被吃掉");
    assert!(
        harness.app.state.saves.confirm.is_none(),
        "Esc 之后目标还留着 —— 下一个框的主按钮会把它兑现"
    );
    assert!(harness.app.state.dialogs.current().is_none());

    // 不相干的框：崩溃卡片就是这个形态（`Confirm`，主按钮「导出诊断包」）。
    let _ = harness.app.state.dialogs.push(
        DialogSource::UserAction,
        "启动失败",
        "退出码 1",
        DialogKind::Confirm {
            primary: "导出诊断包".to_string(),
            secondary: "关闭".to_string(),
        },
    );
    let _ = on_dialog(&mut harness.app, DialogMessage::Primary);

    assert!(
        !harness.deleted(),
        "Esc 之后那个目标被下一个框兑现了：世界被静默删除"
    );
    assert!(
        harness.app.state.dialogs.current().is_none(),
        "主按钮该走它自己那条路（这里没有崩溃卡片 ⇒ 关框）"
    );
}

/// **纵深防御（P1-1 的第二层）**：万一某条路径只关了框、**漏清**目标，下一个不相干的框
/// 点主按钮也**拿不走**它 —— 因为 `take_confirm` 只有在"登记时那个框 == 当前框"时才交付。
///
/// 这条直接构造"漏清"状态（登记一个不存在的框 id），所以它**不依赖** `Esc` 那条路有没有清：
/// 去掉 id 校验，它立刻红。
#[test]
fn a_leaked_target_cannot_be_redeemed_by_another_dialog() {
    let mut harness = Harness::new();
    // 模拟事故形态：目标还挂着，但那个框早就不在了（登记 999 = 一个不存在的框）。
    harness.app.state.saves.ask_delete(999, DIR);
    let _ = harness.app.state.dialogs.push(
        DialogSource::UserAction,
        "启动失败",
        "退出码 1",
        DialogKind::Confirm {
            primary: "导出诊断包".to_string(),
            secondary: "关闭".to_string(),
        },
    );

    let _ = on_dialog(&mut harness.app, DialogMessage::Primary);

    assert!(
        !harness.deleted(),
        "漏清的目标被别的框兑现了：世界被静默删除（P1-1 的事故形态）"
    );
    assert!(
        harness.app.state.saves.confirm.is_some(),
        "别人的框不该顺手清掉我们的目标（清它的是关闭路径，不是主按钮）"
    );
}

/// 反向：目标属于**当前**这个框时，主按钮仍然真的下发删除（没把功能关死）。
#[test]
fn the_owning_dialog_still_deletes() {
    let mut harness = Harness::new();
    harness.request_delete();
    let _ = on_dialog(&mut harness.app, DialogMessage::Primary);

    assert!(harness.deleted(), "自己的框点主按钮居然没删");
    assert!(harness.app.state.saves.confirm.is_none(), "取走即清");
    assert!(
        harness.app.state.saves.pending.is_some(),
        "该把那一行标成在飞（否则能连点）"
    );
    assert!(harness.app.state.dialogs.current().is_none(), "框该关掉");
}

/// **P2-1**：世界还在 ⇒ 一拍无关快照（备份回程 / 打开文件夹回程）之后，主按钮照样删。
#[test]
fn an_unrelated_snapshot_keeps_the_confirmation_redeemable() {
    let mut harness = Harness::new();
    harness.request_delete();
    harness.feed(snapshot(&[DIR, "Other"]));

    assert!(
        harness.app.state.saves.confirm.is_some(),
        "无关快照把目标吃掉了：主按钮会变成静默空操作"
    );
    assert!(
        harness.app.state.dialogs.current().is_some(),
        "框不该被收掉"
    );
    let _ = on_dialog(&mut harness.app, DialogMessage::Primary);
    assert!(harness.deleted(), "快照之后主按钮点了没反应");
}

/// **P2-1 的另一半**：世界从快照里消失 ⇒ 框被**收起来**（不是留一个点了没反应的按钮），
/// 而且不下发删除（那个世界已经不存在了，下发只会换来一条 `WriteMissing` 噪音）。
#[test]
fn a_vanished_world_closes_its_confirmation() {
    let mut harness = Harness::new();
    harness.request_delete();
    harness.feed(snapshot(&["Other"]));

    assert!(
        harness.app.state.saves.confirm.is_none(),
        "作废的目标还留着"
    );
    assert!(
        harness.app.state.dialogs.current().is_none(),
        "框没被收起：主按钮会是一次静默空操作"
    );
    assert!(!harness.deleted(), "世界都不在了还下发删除");
}
