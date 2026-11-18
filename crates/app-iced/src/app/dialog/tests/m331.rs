//! M331：Java 不符的**启动前**对话框（说什么 / 只弹一次 / 两个出口各去哪）。
//!
//! | 用例 | 抓得住 |
//! |---|---|
//! | [`the_dialog_names_both_sides_in_both_languages`] | 正文退回一句「Java 版本不对」（两个数字必须都在）、按钮文案退回字面量、或有人把内部 code（`java_too_new`）搬上屏 |
//! | [`the_same_block_pops_only_once`] | `refresh_view` 每条事件都跑 ⇒ 不比对次数就会「关掉之后又自己弹回来」 |
//! | [`primary_goes_to_settings_and_secondary_launches_anyway`] | 主按钮不走设置页 / 次按钮不派 `LaunchInstanceAnyway`（退化成「只是关掉框」= 用户点了没反应） |
//!
//! 抓不住：像素排版（由 `DevDocs/artifacts/javaguard/` 的出图守）。

use std::cell::RefCell;
use std::rc::Rc;

use unlucky_i18n::catalog::Key;
use unlucky_i18n::Lang;
use unlucky_presenter::dto::{
    AppCommand, AppEvent, Backend, CoreEvent, JavaGuardDirection, LaunchBlockedView,
};
use unlucky_presenter::router::Route;
use unlucky_presenter::settings_bridge::SettingsBridge;
use unlucky_presenter::toast::ToastKind;

use crate::app::dialog::{current, DialogRequestView, JavaGuardCard};
use crate::app::state::App;
use crate::cli::Options;
use crate::theme::Families;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

/// 「1.12.2 只能用 Java 8，托管里只有 17」这一次拦下的事实。
fn blocked() -> LaunchBlockedView {
    LaunchBlockedView {
        id: "1.12.2".to_string(),
        name: "better-mc-1122".to_string(),
        version_id: "1.12.2".to_string(),
        direction: JavaGuardDirection::TooNew,
        required_min: 8,
        required_max: Some(8),
        actual: 17,
        install_major: Some(8),
        // 硬拦没有「实测证据」：证据只属于建议那一支（M349 r2）。
        evidence: None,
    }
}

/// 记下 `dispatch` 的后端（只观察，不回事件）。
#[derive(Default)]
struct RecorderBackend {
    dispatched: Rc<RefCell<Vec<AppCommand>>>,
}

impl Backend for RecorderBackend {
    fn drain_events(&mut self) -> Vec<AppEvent> {
        Vec::new()
    }

    fn dispatch(&mut self, command: AppCommand) {
        self.dispatched.borrow_mut().push(command);
    }
}

/// 用 recorder 造一个 App（返回它，方便断言「点了哪个按钮 ⇒ 派了哪条命令」）。
fn recording() -> (App, Rc<RefCell<Vec<AppCommand>>>) {
    let dispatched = Rc::new(RefCell::new(Vec::new()));
    let backend = RecorderBackend {
        dispatched: Rc::clone(&dispatched),
    };
    let app = App::with_presenter(
        Options::default(),
        FAMILIES,
        unlucky_design::color::SchemeFlavor::Light,
        Box::new(backend),
        SettingsBridge::memory_only(),
    );
    (app, dispatched)
}

/// 把一次拦下事件喂进产品路径（`AppState::apply` + `refresh_view`，与真 host 同一条）。
fn deliver(app: &mut App, view: &LaunchBlockedView) {
    app.state
        .apply(AppEvent::Core(CoreEvent::LaunchBlocked(Box::new(
            view.clone(),
        ))));
    app.refresh_view();
}

/// 正文里必须有**两个数字**（需要什么 / 现在是什么）+ 后果；按钮文案必须来自目录。
#[test]
fn the_dialog_names_both_sides_in_both_languages() {
    for lang in [Lang::En, Lang::ZhCn] {
        let catalog = lang.catalog();
        let view = blocked();
        let request = unlucky_presenter::dialogs::DialogRequest {
            id: 7,
            source: unlucky_presenter::dialogs::DialogSource::UserAction,
            title: String::new(),
            body: String::new(),
            kind: unlucky_presenter::dialogs::DialogKind::Confirm {
                primary: String::new(),
                secondary: String::new(),
            },
        };
        let card = JavaGuardCard {
            view: view.clone(),
            dialog: 7,
            install: crate::app::dialog::GuardInstall::Idle,
            consumed: None,
        };
        let modal = current(lang, &DialogRequestView::of(&request), None, Some(&card));

        let shown = modal.body_text();
        assert_eq!(modal.title, catalog.text(Key::JavaGuardTitle));
        assert!(
            shown.contains("8"),
            "{}：正文必须说清「需要 Java 8」：{shown}",
            lang.tag()
        );
        assert!(
            shown.contains("17"),
            "{}：正文必须说清「现在用的是 17」：{shown}",
            lang.tag()
        );
        assert!(
            shown.contains(catalog.text(Key::JavaGuardConsequence)),
            "{}：必须说清「仍要启动大概率会崩」：{shown}",
            lang.tag()
        );
        assert_eq!(modal.primary, catalog.text(Key::JavaGuardOpenSettings));
        assert_eq!(
            modal.secondary.as_deref(),
            Some(catalog.text(Key::JavaGuardLaunchAnyway))
        );
        // 内部 code 不许上屏（用户看不懂 `java_too_new`）
        assert!(!shown.contains("java_too"), "{shown}");
        assert!(!shown.contains("TooNew"), "{shown}");
        // 崩溃卡片的证据区不该出现在这里（这是**启动前**，没有现场可给）
        assert!(modal.card.is_none(), "拦下没有崩溃现场");
        // 中英两套译文必须真的不同（漏译会被这条抓住）
        assert_ne!(
            Lang::En.catalog().text(Key::JavaGuardTitle),
            Lang::ZhCn.catalog().text(Key::JavaGuardTitle)
        );
    }
}

/// 同一条拦下事件被重投两次 ⇒ 只弹一个框（不排队、不叠第二个）。
#[test]
fn the_same_block_pops_only_once() {
    let (mut app, _recorded) = recording();
    deliver(&mut app, &blocked());
    let first = app.state.dialogs.current().map(|request| request.id);
    assert!(first.is_some(), "拦下必须弹框");
    assert_eq!(app.state.dialogs.pending_len(), 0);

    // 同一条事件再来一遍（用户又点了一次启动 / 事件被重投）
    deliver(&mut app, &blocked());
    assert_eq!(app.state.dialogs.current().map(|request| request.id), first);
    assert_eq!(
        app.state.dialogs.pending_len(),
        0,
        "同一件事不许再压一条进队列"
    );
}

/// 主按钮 = 去设置改 Java；次按钮 = 仍要启动（派 `LaunchInstanceAnyway`）。
#[test]
fn primary_goes_to_settings_and_secondary_launches_anyway() {
    let (mut app, recorded) = recording();

    // 主按钮：去设置页，框关掉，卡片一起清掉
    deliver(&mut app, &blocked());
    let _ = app.update(crate::app::Message::Dialog(
        crate::app::dialog::Message::Primary,
    ));
    assert_eq!(app.state.route(), &Route::Settings, "主按钮要跳到设置页");
    assert!(app.state.dialogs.is_empty(), "点了主按钮框要关掉");
    assert!(app.guard_for_current().is_none(), "卡片要跟着框一起清");
    assert!(
        recorded.borrow().is_empty(),
        "去设置这条路不该顺带启动：{:?}",
        recorded.borrow()
    );

    // 次按钮：仍要启动 ⇒ 一条 LaunchInstanceAnyway（**只放过这一次**，不写设置）
    deliver(&mut app, &blocked());
    let _ = app.update(crate::app::Message::Dialog(
        crate::app::dialog::Message::Secondary,
    ));
    assert_eq!(
        *recorded.borrow(),
        vec![AppCommand::LaunchInstanceAnyway("1.12.2".to_string())],
        "次按钮必须派「仍要启动」，实例 id 取自拦下事实（不靠上下文猜）"
    );
    assert!(app.state.dialogs.is_empty(), "点了次按钮框要关掉");

    // Esc / 点遮罩只关框，绝不放行
    deliver(&mut app, &blocked());
    let _ = app.update(crate::app::Message::Dialog(
        crate::app::dialog::Message::Dismiss,
    ));
    assert!(
        recorded.borrow().len() == 1,
        "关掉框（遮罩 / Esc）不许启动：{:?}",
        recorded.borrow()
    );
    assert!(app.state.dialogs.is_empty());
}

/// 拦下**不是失败**：磁贴不许被点亮成「启动失败」，也不许推一条错误 Toast。
///
/// 注意要先**喂一份实例快照**（复审 P2-2 那族的同一课）：快照为空时
/// `all(!Busy && !Running)` 是空集合上的恒真断言，等于什么都没断。
#[test]
fn a_block_does_not_paint_the_tile_as_failed() {
    let (mut app, _recorded) = recording();
    app.state.apply(AppEvent::Core(CoreEvent::InstancesChanged(
        unlucky_presenter::mock_data::mock_instances(),
    )));
    app.refresh_view();
    assert!(!app.state.instances.is_empty(), "前置条件：快照非空");
    deliver(&mut app, &blocked());
    assert!(
        app.state
            .toasts
            .items()
            .iter()
            .all(|toast| toast.kind != ToastKind::Error),
        "拦下的框已经把话说清了，不该再叠一条错误 Toast"
    );
    let survival = app
        .state
        .instances
        .iter()
        .find(|instance| instance.id == "survival")
        .expect("快照里有 survival");
    assert_eq!(
        survival.runtime,
        unlucky_presenter::dto::InstanceRuntime::Idle,
        "拦下不许把磁贴画成启动中/运行中"
    );
    assert!(
        app.state.instances.iter().all(|instance| {
            !matches!(
                instance.runtime,
                unlucky_presenter::dto::InstanceRuntime::Busy { .. }
                    | unlucky_presenter::dto::InstanceRuntime::Running { .. }
            )
        }),
        "拦下不许把任何实例画成启动中/运行中"
    );
}

/// **关掉之后不许自己弹回来**（复审 nit-1：这条钉的是 `refresh.rs` 的**次数差**，
/// 而不是 `open_java_guard_dialog` 里「同 view 不重压」的早退）。
///
/// 抓得住：把 `refresh.rs` 的 `blocks != launch_blocks_seen` 改成「有拦下记录就弹」——
/// 那时关掉框之后，任何一条无关事件（列表刷新、安装进度……）都会把它重新弹出来，本用例红。
/// 抓不住：用户点「仍要启动」之后 host 真的又拦了一次（那是**新的一次拦下**，本来就该弹）。
#[test]
fn a_closed_block_never_pops_back_on_unrelated_events() {
    let (mut app, _recorded) = recording();
    deliver(&mut app, &blocked());
    assert!(app.state.dialogs.current().is_some(), "先要来一个框");
    let _ = app.update(crate::app::Message::Dialog(
        crate::app::dialog::Message::Dismiss,
    ));
    assert!(app.state.dialogs.is_empty(), "关掉");

    // 一条与拦下无关的事件（实例列表刷新）
    app.state.apply(AppEvent::Core(CoreEvent::InstancesChanged(
        unlucky_presenter::mock_data::mock_instances(),
    )));
    app.refresh_view();
    assert!(
        app.state.dialogs.is_empty(),
        "关掉之后不许因为别的事件又弹回来（次数没变）"
    );

    // 而**新的一次**拦下仍然要弹：否则这条判据会退化成「永远不弹」也绿
    deliver(&mut app, &blocked());
    assert!(
        app.state.dialogs.current().is_some(),
        "新的一次拦下必须还能弹出来"
    );
}
