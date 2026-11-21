//! `app/update/instances.rs` 的用例（M455 从那边原样搬出：那边加完 M455 的两个档位分支后
//! 到了 405 行，越过 400 行硬线，而原有的 5 条用例占 205 行）。
//!
//! 纯搬移：用例与注释一字未动，路径靠 `instances.rs` 的 `#[path = "instances/tests.rs"] mod tests;`
//! 保持不变。M455 自己那两条档位用例在 `view_mode.rs`（测的是另一件事：写盘 + 渲染跟随）。

use unlucky_design::color::SchemeFlavor;
use unlucky_presenter::dto::{AppCommand, AppEvent, Backend, CoreEvent, LaunchArgsView};
use unlucky_presenter::settings_bridge::SettingsBridge;

use crate::app::state::App;
use crate::cli::Options;
use crate::pages::{InstanceMessage, LaunchForm};

use crate::theme::Families;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

fn app_with_form() -> App {
    let mut app = App::with_presenter(
        Options::default(),
        FAMILIES,
        SchemeFlavor::Light,
        Box::new(unlucky_presenter::mock::MockBackend::new()),
        SettingsBridge::memory_only(),
    );
    // 进详情页 + 直接放一份「已播种」的表单：本测试只验消息落点，
    // 不验播种（那在 `LaunchForm::sync` 的单测里）。
    app.state.navigate(unlucky_presenter::router::Route::Detail(
        INSTANCE.to_string(),
    ));
    app.launch_form = LaunchForm::from_view(
        Some(INSTANCE),
        &LaunchArgsView {
            instance_id: INSTANCE.to_string(),
            ..LaunchArgsView::default()
        },
    );
    app
}

/// 测试用的实例 id（mock 后端的实例快照里有它）。
const INSTANCE: &str = "survival";

/// 编辑 → 脏；恢复 → 五项清空仍需保存；保存前不发命令。
#[test]
fn editing_marks_dirty_and_reset_waits_for_save() {
    let mut app = app_with_form();
    let _ = app.on_instance_message(InstanceMessage::Memory("6144".to_string()));
    assert_eq!(app.launch_form.memory, "6144");
    assert!(app.launch_form.dirty);

    let _ = app.on_instance_message(InstanceMessage::Backend("vulkan".to_string()));
    assert_eq!(app.launch_form.backend, "vulkan");

    let _ = app.on_instance_message(InstanceMessage::Reset);
    assert_eq!(app.launch_form.memory, "");
    assert_eq!(app.launch_form.backend, "default");
    assert!(app.launch_form.dirty, "恢复之后还得点保存才落盘");
}

/// **MODS 区的启停在表单守卫之前**（M224）：表单还没播种也照样能启停。
///
/// 抓得住：把 `SetModEnabled` 放回 `bound_to` 守卫之后——那时首帧的详情页点了开关
/// 什么都不会发生（用户看到开关纹丝不动，也没有任何提示）。
#[test]
fn mod_toggles_work_before_the_launch_form_is_bound() {
    let mut app = app_with_form();
    // 页面已经打开（真路径上 `refresh_view` 的 `sync_mods` 会做这一步），但表单还没播种。
    app.state.bind_mods(Some(INSTANCE));
    app.launch_form = LaunchForm::from_view(
        Some("other"),
        &LaunchArgsView {
            instance_id: "other".to_string(),
            ..LaunchArgsView::default()
        },
    );

    let _ = app.on_instance_message(InstanceMessage::SetModEnabled {
        file: "sodium.jar".to_string(),
        enabled: false,
    });

    assert!(
        app.state.mods.is_pending("sodium.jar"),
        "启停被表单守卫挡住了（开关点了没反应）"
    );
}

/// **M302：刷新在飞时列表不清空，快照落地后时钟不被钉住**。
///
/// 现场：扫盘搬进后台线程之后，「派发刷新 → 结果回来」之间有一小段真空。这段真空里界面
/// 只有两条路：**留着旧列表**（用户看到的是「还在，但还没变」）或者**清空再重填**（用户
/// 看到的是一屏白/空态）。M302 要的是前者；后者等于把卡顿换成闪屏。
///
/// 第二件事是 M298 的型态：`view_dirty` 一旦永久为真，`advance` 就每拍无条件 `refresh_view`
/// （整页视图模型重建），而「快照落地」这条路最容易被写成「只置脏不重建」。
///
/// 抓得住：① **没有事件的一拍** `pump` 不把 `state.instances` 清空（用例走的是
/// `backend.dispatch + pump`，所以要防的是「事件落地路径顺手清表」这一类错）；
/// ② 谁把 `InstancesChanged` 的落地路径写成「置脏不落地」或者让 `refresh_view` 收尾不再复位
/// `view_dirty`。
/// 抓不住：**`App::dispatch` 那 5 行**（它置 `watch_until`、再 pump）本身——本用例故意绕过它，
/// 因为观察窗会让 `needs_frames()` 在 3 s 内为真，与「刷新有没有多订阅帧」这件事混在一起。
/// 换句话说：这里守的是「一拍无事件 = 列表不动 + 不烧帧」，不是「派发那一刻」。
/// 抓不住：真线程的时序与竞争（那在 presenter 的 `host::game::refresh::tests` 交错用例里）；
/// 也抓不住「扫盘真要多久」（那是真实测量，不是单测）。
#[test]
fn a_refresh_in_flight_keeps_the_list_and_does_not_pin_the_clock() {
    /// 只喂首屏快照、派发命令一律不回答的后端（= 「刷新还在飞」）。
    #[derive(Debug)]
    struct RecorderBackend {
        seeded: Vec<AppEvent>,
    }

    impl Backend for RecorderBackend {
        fn drain_events(&mut self) -> Vec<AppEvent> {
            std::mem::take(&mut self.seeded)
        }

        fn dispatch(&mut self, _command: AppCommand) {}
    }

    let mut app = App::with_presenter(
        Options::default(),
        FAMILIES,
        SchemeFlavor::Light,
        Box::new(RecorderBackend {
            seeded: vec![AppEvent::Core(CoreEvent::InstancesChanged(
                unlucky_presenter::mock_data::mock_instances(),
            ))],
        }),
        SettingsBridge::memory_only(),
    );
    // 前提：首屏快照已落地、入场动效已经跑完（否则 `needs_frames` 为真是动效的锅，不是刷新的）
    app.pump();
    while crate::app::components::motion::needs_frames() {
        crate::app::components::motion::advance_by(crate::app::components::motion::FRAME_MS);
    }
    let tiles = app.state.tiles().len();
    assert!(tiles > 0, "前提：首屏该有实例");

    // 刷新在飞：命令出去了，后端这一拍一个字都不回
    app.backend.dispatch(AppCommand::RefreshAll);
    app.pump();
    assert_eq!(
        app.state.tiles().len(),
        tiles,
        "刷新在飞时列表被清空了（清空再重填 = 把卡顿换成闪屏）"
    );
    assert!(
        !app.view_dirty,
        "刷新在飞时 `view_dirty` 不许留在真上（M298：每拍重建整页视图模型）"
    );
    assert!(
        !app.needs_frames(),
        "刷新在飞时不该多出帧节拍（真要等回程，走派发后的 3 s 观察窗，不是永久订阅）"
    );

    // 回程到了：整份替换（不是追加），并且这一拍就把脏标收回去
    app.backend = Box::new(RecorderBackend {
        seeded: vec![AppEvent::Core(CoreEvent::InstancesChanged(
            unlucky_presenter::mock_data::mock_instances(),
        ))],
    });
    app.pump();
    assert_eq!(app.state.tiles().len(), tiles, "快照是整体替换");
    assert!(
        !app.view_dirty,
        "快照落地之后 `view_dirty` 必须被 `refresh_view` 收回去（M298）"
    );
}

#[test]
fn edits_are_ignored_when_the_form_is_bound_elsewhere() {
    let mut app = app_with_form();
    app.launch_form = LaunchForm::from_view(
        Some("other"),
        &LaunchArgsView {
            instance_id: "other".to_string(),
            ..LaunchArgsView::default()
        },
    );
    let _ = app.on_instance_message(InstanceMessage::Memory("9999".to_string()));
    assert_eq!(app.launch_form.memory, "");
    assert!(!app.launch_form.dirty);
}

/// **保存只派发、不清脏**（M157 r3 复审 P2-2）：落盘成败要等回程事件，
/// 派发前清掉脏标记会让失败后的重播覆盖用户草稿。
///
/// 抓得住：Save 分支里又写回 `dirty = false`（那条路径在失败时丢草稿）。
/// 抓不住：回程事件的真实时序——清脏判据在 `LaunchForm::sync` 的
/// `confirmed_save_clears_dirty_and_reseeds` / `failed_save_keeps_the_draft` 里。
#[test]
fn save_dispatch_keeps_the_dirty_flag() {
    let mut app = app_with_form();
    let _ = app.on_instance_message(InstanceMessage::Memory("4g".to_string()));
    assert!(app.launch_form.dirty);
    let _ = app.on_instance_message(InstanceMessage::Save);
    assert!(
        app.launch_form.dirty,
        "派发后仍要保留脏标记，等回程快照确认"
    );
    assert_eq!(app.launch_form.memory, "4g", "派发不改表单内容");
}
