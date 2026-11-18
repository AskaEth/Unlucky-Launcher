//! M302 的**首屏三态接线**：presenter 的 `InstancePhase` → 页面三态，以及「重新扫描」的落点。
//!
//! 从 `app/update/instances.rs` 拆出来只为守 400 行硬线（那边加了这一组会到 573 行）。
//!
//! | 用例 | 抓得住 | 抓不住 |
//! |---|---|---|
//! | [`a_fresh_app_is_scanning_until_the_snapshot_lands`] | 首帧（还没有任何快照）被画成「已就绪 + 零实例」（= 复审 P2-1 的假空态） | 真的像素（出图） |
//! | [`an_empty_snapshot_still_counts_as_ready`] | 空快照被当成「未就绪」（用户永远看着「正在扫描」） | 空快照从哪来（presenter 的 `refresh_tests` 用真空盘钉） |
//! | [`a_stalled_scan_goes_back_to_scanning_on_retry`] | 超时之后没有出路（`Stalled` 不会被新一轮扫描收回 `Scanning`） | 15 s 的阈值本身（`host::game::SCAN_STALL`，单测不等真实时间） |
//! | [`retry_scan_dispatches_a_scan_only_refresh`] | 重试按钮派错命令 / 被「必须有选中实例」的守卫挡住（冷启动时根本没有选中项） | 命令真的到达 host（`host::dispatch` 的 `RefreshInstances` 分支由 presenter 的穷尽 match 守） |
//! | [`a_real_host_flips_the_detail_button_in_the_same_beat`] | 真 host + 真 app：点启动后**同一拍**（`advance` 之前）没进运行态（把 `Busy` 拖到回程才发） | 像素；受理之后那条后台线程的成败（不在本条判据里） |

//! M302 的**首屏三态接线**：presenter 的 `InstancePhase` → 页面三态，以及「重新扫描」的落点。
//!
//! | 用例 | 抓得住 | 抓不住 |
//! |---|---|---|
//! | [`a_fresh_app_is_scanning_until_the_snapshot_lands`] | 首帧（还没有任何快照）被画成「已就绪 + 零实例」（= 复审 P2-1 的假空态） | 真的像素（出图） |
//! | [`an_empty_snapshot_still_counts_as_ready`] | 空快照被当成「未就绪」（用户永远看着「正在扫描」） | 空快照从哪来（presenter 的 `refresh_tests` 用真空盘钉） |
//! | [`a_stalled_scan_goes_back_to_scanning_on_retry`] | 超时之后没有出路（`Stalled` 不会被新一轮扫描收回 `Scanning`） | 15 s 的阈值本身（`host::game::SCAN_STALL`，单测不等待真实时间） |
//! | [`retry_scan_dispatches_a_scan_only_refresh`] | 重试按钮派错命令 / 被「必须有选中实例」的守卫挡住（冷启动时根本没有选中项） | 命令真的到达 host（`host::dispatch` 的 `RefreshInstances` 分支由 presenter 编译期穷尽 match 守） |

use std::cell::RefCell;
use std::rc::Rc;

use unlucky_design::color::SchemeFlavor;
use unlucky_presenter::dto::{AppCommand, AppEvent, Backend, CoreEvent, InstancePhase};
use unlucky_presenter::settings_bridge::SettingsBridge;

use crate::app::state::App;
use crate::cli::Options;
use crate::pages::model::ScanPhase;
use crate::pages::InstanceMessage;

use crate::theme::Families;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

/// 只记命令、只在被喂料时回事件的后端（首帧测试要的是「什么都还没到」）。
#[derive(Debug, Default)]
struct Recorder {
    dispatched: Rc<RefCell<Vec<AppCommand>>>,
    queued: Vec<AppEvent>,
}

impl Backend for Recorder {
    fn drain_events(&mut self) -> Vec<AppEvent> {
        std::mem::take(&mut self.queued)
    }

    fn dispatch(&mut self, command: AppCommand) {
        self.dispatched.borrow_mut().push(command);
    }
}

/// 造一个「首帧还没收到任何快照」的 App（路由停在实例页，**没有**选中实例）。
fn app() -> (App, Rc<RefCell<Vec<AppCommand>>>) {
    let recorded = Rc::new(RefCell::new(Vec::new()));
    let mut app = App::with_presenter(
        Options::default(),
        FAMILIES,
        SchemeFlavor::Light,
        Box::new(Recorder {
            dispatched: Rc::clone(&recorded),
            queued: Vec::new(),
        }),
        SettingsBridge::memory_only(),
    );
    app.state
        .navigate(unlucky_presenter::router::Route::Instances);
    app.refresh_view();
    (app, recorded)
}

/// 首帧：**没有快照 ⇒ 画占位**，不是「已就绪 + 零实例」。
///
/// 抓得住：谁把页面三态接成「`rows.is_empty()` ⇒ 空态」（复审 P2-1 的原样），这条立刻红。
/// 抓不住：占位长什么样（`pages/instances/tests/cold_boot.rs` 的控件树 + 出图）。
#[test]
fn a_fresh_app_is_scanning_until_the_snapshot_lands() {
    let (mut app, _recorded) = app();

    assert_eq!(
        app.view.instances.scan,
        ScanPhase::Scanning,
        "首帧（还没有任何实例快照）必须画占位，不许画「还没有实例」"
    );
    assert!(app.view.instances.rows.is_empty(), "首帧本来就没有行");

    // 快照到了：三态翻到 Ready，行数跟着来（占位消失）
    app.state.apply(AppEvent::Core(CoreEvent::InstancesChanged(
        unlucky_presenter::mock_data::mock_instances(),
    )));
    app.refresh_view();
    assert_eq!(app.view.instances.scan, ScanPhase::Ready);
    assert!(
        !app.view.instances.rows.is_empty(),
        "快照里的实例必须出现在列表里"
    );
}

/// **空快照也算就绪**：真·空数据根扫完就是「零实例 + Ready」，不是「一直在扫描」。
///
/// 抓得住：把「收到快照」判成「没收到」（例如拿空列表当未就绪）——那会让空数据根永远转圈。
/// 抓不住：真空盘真的产出空快照这件事（presenter 的 `refresh_tests` 用真临时根钉）。
#[test]
fn an_empty_snapshot_still_counts_as_ready() {
    let (mut app, _recorded) = app();

    app.state
        .apply(AppEvent::Core(CoreEvent::InstancesChanged(Vec::new())));
    app.refresh_view();

    assert_eq!(
        app.view.instances.scan,
        ScanPhase::Ready,
        "扫完且零实例 = 就绪：这时才轮到「还没有实例」+ 导入 CTA"
    );
    assert!(app.view.instances.rows.is_empty());
}

/// 超时那屏：`Stalled` 落地 ⇒ 页面翻到「重新扫描」；点了重试 ⇒ 回到 `Scanning`。
///
/// 抓得住：`InstancePhaseChanged` 没接进页面（超时对用户不可见）；重试之后没有回到扫描态
/// （按钮点完还停在错误态）。
/// 抓不住：15 s 的阈值（`SCAN_STALL`，等真实时间不是单测该做的事）。
#[test]
fn a_stalled_scan_goes_back_to_scanning_on_retry() {
    let (mut app, recorded) = app();
    app.state
        .apply(AppEvent::Core(CoreEvent::InstancePhaseChanged(
            InstancePhase::Stalled,
        )));
    app.refresh_view();
    assert_eq!(app.view.instances.scan, ScanPhase::Stalled);

    // 用户点「重新扫描」⇒ 派一条只重扫实例的命令
    let _ = app.on_instance_message(InstanceMessage::RetryScan);
    assert!(
        recorded.borrow().contains(&AppCommand::RefreshInstances),
        "重试必须派 `AppCommand::RefreshInstances`（实际 {:?}）",
        recorded.borrow()
    );

    // host 受理后会把三态打回 `Scanning`（新一轮扫描起跑）
    app.state
        .apply(AppEvent::Core(CoreEvent::InstancePhaseChanged(
            InstancePhase::Scanning,
        )));
    app.refresh_view();
    assert_eq!(app.view.instances.scan, ScanPhase::Scanning);
}

/// 「重新扫描」派的是**只重扫实例**那条命令，而且冷启动（没有选中实例）也能派出去。
///
/// 抓得住：① 派成 `RefreshAll`（会顺带重拉版本清单网络请求）；② 那一支被 `selected_instance_id()`
/// 守卫挡住——冷启动时列表还是空的、根本没有选中项，按钮会点了没反应（`on_instance_message`
/// 的第一条守卫就是这个形态）。
/// 抓不住：host 收到之后真的重扫（`host::dispatch` 的 `RefreshInstances` 分支）。
#[test]
fn retry_scan_dispatches_a_scan_only_refresh() {
    let (mut app, recorded) = app();
    assert!(
        app.selected_instance_id().is_none(),
        "前提：冷启动没有选中实例（这正是守卫会挡住的情形）"
    );

    let _ = app.on_instance_message(InstanceMessage::RetryScan);

    let sent = recorded.borrow().clone();
    assert_eq!(
        sent,
        vec![AppCommand::RefreshInstances],
        "只许派一条 `RefreshInstances`（不许 `RefreshAll`，也不许什么都不派）"
    );
    assert!(
        !sent.contains(&AppCommand::RefreshAll),
        "重试不该重拉版本清单（那是一条网络请求）"
    );
}

/// **已就绪之后再刷新：相位仍是 `Ready`，真控件树里不出现扫描占位**（复审 r2 P2-4）。
///
/// 现场：列表已经有数据，用户手动刷新 / 别处触发刷新。若那一轮把相位打回 `Scanning`，界面就把
/// 有数据的一屏换成「正在扫描实例…」——把卡顿换成闪屏（任务 ⑤ 禁止）。
///
/// 抓得住：① 相位被打回 `Scanning`/`Stalled`；② 控件树变了（占位替换掉列表：文字/按钮节点数
/// 与刷新前不再相等）。前半条的直接来源是 host 的守卫（presenter 的
/// `a_refresh_after_ready_emits_no_scanning_phase` 用真 host 钉它），这一条验的是「守卫成立时
/// 界面确实不动」。
/// 抓不住：真 host 的那一轮扫描（这一条用只记账的后端：命令出去了、事件一个都没回）。
#[test]
fn a_refresh_after_ready_keeps_the_list_and_the_tree() {
    use iced::advanced::widget::tree::Tag;
    use iced::advanced::widget::Tree;
    use iced::widget::text;
    use iced::Element;

    fn tree_of(el: &Element<'_, crate::pages::Message>) -> Tree {
        Tree::new(el.as_widget())
    }
    fn count(tree: &Tree, tag: Tag) -> usize {
        usize::from(tree.tag == tag)
            + tree
                .children
                .iter()
                .map(|child| count(child, tag))
                .sum::<usize>()
    }
    /// 一屏的 `(文字节点数, 按钮数)`。
    fn counts(app: &App) -> (usize, usize) {
        let el = crate::pages::app(&app.ui, &app.view, &app.launch_form);
        let tree = tree_of(&el);
        let text_tag = tree_of(&{
            let probe: Element<'static, crate::pages::Message> = text("probe").into();
            probe
        })
        .tag;
        let button_tag = tree_of(&crate::app::components::buttons::filled(
            &app.ui,
            crate::app::components::buttons::Button::new(
                "probe",
                crate::pages::Message::Action(crate::pages::Action::Back),
            ),
        ))
        .tag;
        (count(&tree, text_tag), count(&tree, button_tag))
    }

    let (mut app, recorded) = app();
    // 首份快照落地 ⇒ Ready + 有行
    app.state.apply(AppEvent::Core(CoreEvent::InstancesChanged(
        unlucky_presenter::mock_data::mock_instances(),
    )));
    app.refresh_view();
    assert_eq!(app.view.instances.scan, ScanPhase::Ready);
    assert!(!app.view.instances.rows.is_empty(), "前提：列表里有数据");
    let before = counts(&app);

    // 再触发一次刷新：命令出去了，后端这一拍**一个事件都不回**（= 扫描在飞）
    let _ = app.on_instance_message(InstanceMessage::RetryScan);
    app.pump();
    app.refresh_view();

    assert!(
        recorded.borrow().contains(&AppCommand::RefreshInstances),
        "前提：刷新命令真的发出去了（{:?}）",
        recorded.borrow()
    );
    assert_eq!(
        app.view.instances.scan,
        ScanPhase::Ready,
        "已就绪之后再刷新，相位不许被打回占位态"
    );
    assert_eq!(
        counts(&app),
        before,
        "控件树必须与刷新前同构（占位/空态会多出文字与按钮节点）"
    );
    assert!(!app.view.instances.rows.is_empty(), "列表也不许被清空");
}

/// **M304 的端到端判据**：真 `CoreHost` + 真数据根 + 真 app —— 点启动 ⇒ **同一拍**（`advance` 之前）
/// 详情页的按钮已经翻成「可停止」。
///
/// 判据落在**状态**上：`app.dispatch(LaunchInstance)` 只做「派发 + 抽干**当下**已有的事件」，之后
/// 一次 `advance`（= `backend.tick()`）都不调。所以这条绿的意思是「`Busy` 在 host 的 `dispatch`
/// 返回前就发出来了」，而不是「等某条后台线程回报」。
///
/// 抓得住：把 host 那条 `InstanceRuntimeChanged{Busy}` 挪进启动线程（= 只有回程才发）——那时抽干
/// 到的是空队列，`can_stop` 仍是 false，这条红（presenter 侧的同款判据在
/// `host::launch::target_tests::clicking_launch_shows_busy_in_the_same_beat`）。
/// 抓不住：真鼠标点击（从 `AppCommand` 起跑）；受理之后装配/启动的成败（这一条只判「同一拍」）。
#[test]
fn a_real_host_flips_the_detail_button_in_the_same_beat() {
    use std::time::Duration;

    use unlucky_core::game::install::instance::write_profiles;
    use unlucky_core::game::{InstanceProfile, ProfileId, ProfileList};
    use unlucky_presenter::host::{CoreHost, HostEnv};
    use unlucky_presenter::router::Route;

    let dir = std::env::temp_dir().join(format!("ull-m304-launch-beat-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".ULL/config")).expect("config 目录");

    let env = HostEnv::at_root(&dir);
    // 一条实例档案。**不造版本本体**：装配会失败——但那是**受理之后**的事，本条判的是受理那一拍。
    let profile = InstanceProfile {
        id: ProfileId::generate(),
        name: "demo".to_string(),
        version_id: "1.20.1".to_string(),
        game_dir: None,
        running_dir: None,
        java_path: None,
        created_at: None,
        last_played_at: None,
        extra: std::collections::BTreeMap::new(),
    };
    let id = profile.id.as_str().to_string();
    write_profiles(
        &env.layout,
        &ProfileList {
            profiles: vec![profile],
            ..ProfileList::default()
        },
    )
    .expect("写 profiles.json");

    let mut app = App::with_presenter(
        Options {
            data_dir: Some(dir.clone()),
            ..Options::default()
        },
        FAMILIES,
        SchemeFlavor::Light,
        Box::new(CoreHost::new(env)),
        SettingsBridge::with_path(unlucky_presenter::backend::config_path(&dir)),
    );
    app.dispatch(AppCommand::RefreshAll);
    // 先 `advance` 到**第一份快照落地**（M302 之后扫盘在后台线程，这一拍必须等）
    for _ in 0..200 {
        if app.state.tiles().iter().any(|cell| cell.instance.id == id) {
            break;
        }
        app.advance(Duration::from_millis(33));
    }
    assert!(
        app.state.tiles().iter().any(|cell| cell.instance.id == id),
        "前提：快照里必须已经有这条实例（等了 200 拍）"
    );
    app.state.navigate(Route::Detail(id.clone()));
    app.refresh_view();
    let before = app.view.instances.detail.clone().expect("有选中项");
    assert!(
        before.can_launch && !before.can_stop,
        "前提：空闲态可启动不可停止（{before:?}）"
    );

    // 点 Play：只派发 + 抽干**当下**的事件，不 tick、不 advance
    app.dispatch(AppCommand::LaunchInstance(id.clone()));

    let after = app.view.instances.detail.clone().expect("有选中项");
    assert!(
        after.can_stop && !after.can_launch,
        "点下去必须**同一拍**进运行态（advance 之前）：{after:?}"
    );

    // 收尾：让那条后台线程把失败交回来（不造版本本体 ⇒ 装配失败），再把临时根删干净
    for _ in 0..100 {
        app.advance(Duration::from_millis(33));
        if app
            .view
            .instances
            .detail
            .as_ref()
            .is_some_and(|detail| !detail.can_stop)
        {
            break;
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
