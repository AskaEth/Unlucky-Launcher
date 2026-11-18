use super::*;
use crate::app::components::motion::FRAME_MS;
use crate::app::message::Message;
use crate::cli::Options;
use crate::theme::Families;
use unlucky_design::color::SchemeFlavor;
use unlucky_design::motion::MotionScale;
use unlucky_presenter::mock::MockBackend;
use unlucky_presenter::settings_bridge::SettingsBridge;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

/// 不碰磁盘、后端是 mock 的 App（与 `app/tests.rs` 的 `headless` 同构）。
fn headless() -> App {
    App::with_presenter(
        Options::default(),
        FAMILIES,
        SchemeFlavor::Light,
        Box::new(MockBackend::new()),
        SettingsBridge::memory_only(),
    )
}

/// 推时间轴到没有轨道（每步 16 ms，上限 20 s）。
fn run_until_idle() {
    for _ in 0..1250 {
        if !motion::needs_frames() {
            return;
        }
        motion::advance_by(FRAME_MS);
    }
}

/// 有窗口、首屏动效已跑完、旧页模型已清空的 App。定档走 `configure_for_test`（P2-5，见
/// `a_pinned_scale_survives_boot_with_motion_off`：它把档**钉住**，`on_boot` 的重定档赢不了）。
fn idle_app() -> App {
    motion::reset();
    motion::configure_for_test(MotionScale::STANDARD, None);
    let mut app = headless();
    app.window = Some(iced::window::Id::unique());
    let _ = app.update(Message::Boot);
    app.watch_until = None;
    run_until_idle();
    let _ = app.update(Message::Frame); // 让 `sync_motion` 把启动期那次换页的旧页烧掉
    assert!(!motion::needs_frames(), "前提：首屏动效已经跑完");
    app
}

/// 屏键 / 轨序两张表：**方向只听轨序**（复审 P2-1 的守卫）。
///
/// 抓得住：①`Screen::of` 与 `Screen::ALL` 的对应被改坏；②把 `nav_rank` 改成返回屏键（或退回
/// 比较 `Screen::ALL` 下标）——下面那条「轨序里 Settings 在 Logs 之后」会红，因为屏键顺序恰好
/// 相反；③谁调了 `Screen::ALL` 的声明顺序（本用例把这对**相反的不等关系**两侧都钉住）。
/// 抓不住：导航轨的视觉几何（`shell/nav.rs` 的间距/药丸位置归它自己的单测）。
#[test]
fn the_rail_order_and_the_screen_table_disagree_on_purpose() {
    let keyed = |route: &Route| (screen_key(route), nav_rank(route));
    assert_eq!(
        keyed(&Route::Detail("survival".to_string())),
        (Some(1), Some(0)),
        "详情归「实例」"
    );
    assert_eq!(
        keyed(&Route::Accounts),
        (None, None),
        "账户不归 pages 画、也不在轨上"
    );
    assert_eq!(
        keyed(&Route::Agent),
        (None, Some(4)),
        "AI 不归 pages 画，但在轨最下面"
    );
    assert_eq!(
        nav_rank(&Route::Content),
        Some(1),
        "内容库与 Download 共用导航项"
    );

    // 两条**相反**的不等关系：这就是 P2-1 修的镜像
    assert!(
        screen_key(&Route::Settings) < screen_key(&Route::Logs),
        "屏键里 Settings 在 Logs 之前"
    );
    assert_eq!(
        nav_rank(&Route::Logs),
        Some(2),
        "轨上 Logs 在 Settings 之前"
    );
    assert_eq!(nav_rank(&Route::Settings), Some(3));
}

/// **旧页模型用完即焚，且快速连点也不累积**（门槛 B）。
///
/// 抓得住：删掉 `sync_motion` 末尾的 `if !needs_frames() { previous_view = None }` ⇒ 两条
/// `is_none()` 断言红。**抓不住、也不假装抓**「中途不会攒到 2 份」：字段是
/// `Option<pages::Model>`，结构上装不下第二份（`refresh.rs` 用 `mem::replace` **移动**）；
/// 旧版 `usize::from(is_some()) <= 1` 恒真。真要证明释放得靠析构计数插桩——**没做**。
#[test]
fn the_previous_page_model_is_dropped_and_never_accumulates() {
    let mut app = idle_app();
    assert!(app.previous_view.is_none(), "前提：静息态没有旧页");

    let _ = app.update(Message::Page(pages::Message::Navigate(Route::Download)));
    assert_eq!(
        app.previous_view.as_ref().map(|model| model.screen),
        Some(pages::Screen::Instances),
        "换页那一拍必须留下**切换前**那一屏"
    );
    run_until_idle();
    let _ = app.update(Message::Frame);
    assert!(app.previous_view.is_none(), "过渡结束必须当拍丢掉旧页模型");

    // 快速连点：中间只走 4 帧就点下一次
    for route in [Route::Content, Route::Settings, Route::Instances] {
        let _ = app.update(Message::Page(pages::Message::Navigate(route)));
        for _ in 0..4 {
            motion::advance_by(FRAME_MS);
        }
        assert!(app.previous_view.is_some(), "过渡中途必须留着旧页");
    }
    run_until_idle();
    let _ = app.update(Message::Frame);
    assert!(app.previous_view.is_none(), "连点 4 次之后仍应是 None");
}

/// 旧页是**冻结快照**（门槛 C）：过渡期间的刷新不许把它换成新内容。
///
/// 抓得住：把 `refresh.rs` 的「只在屏/详情变了才换」改成每次刷新都换 ⇒ 旧页跟着 presenter 变，
/// 画面上两个页面同时在动内容。抓不住：`States`（外观/组状态/设置快照）不随模型冻结（偏离 #4）。
#[test]
fn the_previous_page_is_a_frozen_snapshot() {
    let mut app = idle_app();
    let _ = app.update(Message::Page(pages::Message::Navigate(Route::Download)));
    let before = app
        .previous_view
        .as_ref()
        .map(|model| (model.screen, model.window_width));
    assert!(before.is_some());

    app.view_dirty = true;
    app.refresh_view(); // 同屏刷新（主题 / 下载进度那一类的形状）
    motion::advance_by(FRAME_MS);

    assert_eq!(
        app.previous_view
            .as_ref()
            .map(|model| (model.screen, model.window_width)),
        before,
        "同屏刷新不该换掉旧页快照"
    );
}

/// **退场窗口内那个框已经关掉、且不可再交互**（tower 的硬要求）。四件事：①逻辑状态当拍就空；
/// ②画面还在退场（`dialog_model` 仍 `Some`）；③150ms 内再点一次**什么都不发生**（`refreshes`
/// 不涨 = `on_dialog` 的闸门）；④**Esc 那条路径**也挡住（`escape` 的闸门）。
/// **抓得住**：删掉任一道闸门（`on_dialog` / `escape`）或把逻辑关闭改成延后，①②③④ 各自红。
/// **抓不住**：几何级命中（单测没有 iced 布局树，`zoom.rs` 也不改命中矩形）——那一层由闸门保证。
#[test]
fn the_exiting_dialog_is_already_closed_and_swallows_clicks() {
    use crate::app::dialog::Message as DialogMessage;
    use crate::app::update::dialog::{escape, on_dialog};
    use unlucky_presenter::dialogs::{DialogKind, DialogSource};

    let mut app = idle_app();
    let _ = app.state.dialogs.push(
        DialogSource::UserAction,
        "删除世界",
        "不可撤销",
        DialogKind::Confirm {
            primary: "删除".to_string(),
            secondary: "取消".to_string(),
        },
    );
    app.view_dirty = true;
    app.refresh_view();
    // 一条消息让 `sync_motion` 看见「队列里有框」⇒ 起跑进场（与真机顺序一致）
    let _ = app.update(Message::Frame);
    assert!(motion::needs_frames(), "开框必须在同一拍起跑进场");
    run_until_idle();
    assert!(app.dialog_model.is_some(), "前提：框已经画出来了");

    let _ = on_dialog(&mut app, DialogMessage::Secondary);
    assert!(app.state.dialogs.current().is_none(), "①逻辑上已经关掉");
    assert!(motion::modal_closing(), "②画面还在退场");
    assert!(app.dialog_model.is_some(), "②仍画得到旧框（不是硬跳）");

    let refreshes = app.refreshes;
    let _ = on_dialog(&mut app, DialogMessage::Primary);
    assert_eq!(app.refreshes, refreshes, "③退场期点击必须被吞掉");
    assert!(
        app.state.dialogs.current().is_none(),
        "③幽灵点击不许复活那个框"
    );
    assert!(
        escape(&mut app),
        "④退场期 Esc 也必须被吃掉（不许落到 AI 栏）"
    );

    run_until_idle();
    let _ = app.update(Message::Frame);
    assert!(app.dialog_model.is_none(), "退场结束后不许留残影");
    assert!(!motion::needs_frames(), "退场结束后帧泵必须停");
}

/// **钉住的档必须压过产品启动路径**（复审 r2 的根因守卫）：走真 `Message::Boot` ——
/// `on_boot` → `configure_motion` → `boot_env::motion_scale(..)`（本机 CPU 档 ⇒ HALF）→
/// `configure`，全程都不许把测试钉住的 STANDARD 顶掉。
///
/// 为什么不去 `set_var("ULL_MOTION","off")`：那是**进程级写**，并行用例里别的线程正在
/// `getenv`（`boot_env` 每次 Boot 都读），Linux 上这是真竞态。等价做法：把该 env 下
/// `boot_env` 会产出的**取值**（`NONE`）直接喂给 `configure`——见
/// `components::motion::tests::modal::a_pinned_scale_survives_every_later_configure`。
///
/// 抓得住：删掉 `configure` 里「`FORCED_SCALE` 优先」那段 ⇒ 本条红（r2 之前那 7 条红之一
/// 的同一条路）。抓不住：`boot_env` 自己的分支表（env / 后端 tier 的规则，不在本 scope）。
#[test]
fn a_pinned_scale_survives_boot() {
    motion::reset();
    motion::configure_for_test(MotionScale::STANDARD, None);
    let mut app = headless();
    app.window = Some(iced::window::Id::unique());
    let _ = app.update(Message::Boot);
    assert_eq!(
        motion::scale(),
        MotionScale::STANDARD,
        "on_boot 的重定档不许顶掉测试钉住的档（r2 的根因）"
    );
    motion::reset();
}

/// **首屏（`Kind::Boot`）没有旧页**（M277 / 复审 M257-r1 P1-1 的修）：boot 期间 `previous_view`
/// 不许留在那儿等 `pages::render` 去画。
///
/// 幽灵是怎么来的（不必重跑也能读出来）：`pages::render` 把 `previous_view` 整棵画在当前页**下面**
/// （`stack![opaque(ghost), current]`），当前页不透明处盖住它，只在空隙里露出文字与图标磁贴；
/// 而 boot 那份快照是 `app/state/construct.rs` 的**零实例初始模型**，被 `refresh.rs` 在第一次
/// refresh 时换进 `previous_view` ⇒ 每次启动闪一张「这里还没有实例…」的空态页直到退场腿走完
/// （`Kind::Boot` 的退场腿 = `PEER_EXIT` 250ms，`transition.rs` 的表）。
///
/// 抓得住：把 `sync_motion` 的判据退回 `if !motion::needs_frames()`（即删掉 `keeps_old_page`
/// 那半句）⇒ boot 那一拍结束时的第一条 `is_none()` 立刻红（实测：`前提/首屏` 那条断言位置报
/// 「首屏（Kind::Boot）不许留旧页 …」），后面「每拍都跑」那条也红。`frame.exiting` 那条断言是
/// 配套的：它证明**那一帧本来会画**，所以这不是「没什么可画」的假绿。
/// 抓不住：`pages::render` 的调用点本身（单测没有 iced 布局树）；那一处只认
/// 「`previous_view.is_some()` 且 `frame.exiting`」，本用例把前一半钉成 `None`。
#[test]
fn the_first_screen_never_keeps_a_previous_page_to_draw() {
    motion::reset();
    motion::configure_for_test(MotionScale::STANDARD, None);
    let mut app = headless();
    app.window = Some(iced::window::Id::unique());
    let _ = app.update(Message::Boot);
    assert_eq!(
        motion::kind(),
        Some(motion::Kind::Boot),
        "前提：首屏起的是 Boot 档（这是「没有旧页」的那一档）"
    );
    assert!(
        motion::frame().exiting,
        "前提：Boot 的退场腿还在跑 ⇒ 没有守卫时这一帧就会把旧页画出来"
    );
    assert!(motion::needs_frames(), "前提：Boot 轨道还在跑");
    assert!(
        app.previous_view.is_none(),
        "首屏（Kind::Boot）不许留旧页：boot 那一拍的 refresh 已经把零实例初始模型换进 \
         previous_view，删掉 `keeps_old_page` 那半句即红"
    );

    // 再钉一次「不是只清了第一拍」：boot 期间又一份快照落地（换页判据会在屏幕上冒出一次
    // 「屏或详情变了」）⇒ 守卫必须每一拍都生效，而不是只在 Boot 头一帧清一次。
    app.previous_view = Some(pages::Model::new(
        pages::Screen::Instances,
        app.width,
        app.height,
        Some(pages::Screen::Instances.nav()),
    ));
    let _ = app.update(Message::Frame);
    assert_eq!(
        motion::kind(),
        Some(motion::Kind::Boot),
        "一帧之后仍在 Boot 档（`exiting` 为真的窗口里）"
    );
    assert!(
        app.previous_view.is_none(),
        "首屏每一拍都不许留旧页（守卫要每拍都跑，不是只在 Boot 头一帧清一次）"
    );
}

/// **正常换页仍然看得见旧页退场**（M277 的反向守卫：不许为首屏把这条一起关掉）。
///
/// Peer 换页（Instances → Settings 这一档）的旧页退场是产品要的；误关掉比幽灵更显眼。
/// 抓得住：把 `keeps_old_page` 写死成假、或把 `sync_motion` 改成无条件清 `previous_view` ⇒ 红。
#[test]
fn a_peer_page_change_still_keeps_the_old_page_for_the_exit_leg() {
    let mut app = idle_app();
    let _ = app.update(Message::Page(pages::Message::Navigate(Route::Settings)));
    motion::advance_by(FRAME_MS);
    assert!(
        matches!(motion::kind(), Some(motion::Kind::Peer { .. })),
        "前提：同级换页走 Peer 档（有旧页）"
    );
    assert!(motion::frame().exiting, "前提：退场腿还没走完");
    assert_eq!(
        app.previous_view.as_ref().map(|model| model.screen),
        Some(pages::Screen::Instances),
        "Instances → Settings 的旧页必须还留着（删掉旧页退场即红）"
    );
}

/// `MotionScale::NONE`：既不起页面轨道，也不留旧页（两条一起钉住，M277 回归项之一）。
///
/// 抓得住：把 `sync_motion` 的 `!needs_frames()` 那半句删掉后若又给 NONE 档插了轨道；
/// 或把守卫写成「只有 Boot 才丢」却让 NONE 档留下快照（`kind()` 为 `None` 时 `is_none_or` 为真，
/// 靠 `!needs_frames()` 兜住——这一条就是那道兜底的守卫）。
#[test]
fn motion_off_leaves_neither_a_page_track_nor_a_previous_page() {
    motion::reset();
    motion::configure_for_test(MotionScale::NONE, None);
    let mut app = headless();
    app.window = Some(iced::window::Id::unique());
    let _ = app.update(Message::Boot);
    assert_eq!(motion::kind(), None, "关掉动效连轨道都不插");
    assert!(!motion::needs_frames());
    let _ = app.update(Message::Page(pages::Message::Navigate(Route::Settings)));
    assert!(app.previous_view.is_none(), "没有过渡就没有旧页可留");
    motion::reset();
}
