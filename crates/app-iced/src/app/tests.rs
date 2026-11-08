//! `app` 的单测：主题切换、窗口尺寸、截图闸门、gallery 能构图，以及**presenter 接线的闭环**。
//!
//! 闭环测试用的是 `MockBackend`（presenter 自带的假后端）：它走的是与真 host 完全相同的
//! `dispatch → drain_events → AppState::apply → 视图模型` 通道，所以「点 Play → 状态真的变」
//! 这件事在这里就能判死，不必等图形环境。

use unlucky_design::color::SchemeFlavor;
use unlucky_presenter::mock::MockBackend;
use unlucky_presenter::router::Route;
use unlucky_presenter::settings_bridge::SettingsBridge;

#[cfg(test)]
mod appearance;
#[cfg(test)]
mod instances_save_loop;
#[cfg(test)]
mod presenter;

use super::message::Message;
use super::state::App;
use super::{APP_TITLE, WINDOW_SIZE};
use crate::cli::{Options, Screen};
use crate::pages;
use crate::shell::agent::RailState;
use crate::shell::NavItem;
use crate::theme::Families;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

/// 不碰磁盘的 App（mock 后端 + 内存设置桥）。
fn headless(options: Options, flavor: SchemeFlavor) -> App {
    App::with_presenter(
        options,
        FAMILIES,
        flavor,
        Box::new(MockBackend::new()),
        SettingsBridge::memory_only(),
    )
}

fn screenshot_options() -> Options {
    Options {
        screenshot: Some(std::path::PathBuf::from("x.png")),
        ..Options::default()
    }
}

#[test]
fn switching_flavor_rebuilds_styles_but_keeps_ui_state() {
    let mut app = headless(Options::default(), SchemeFlavor::Light);
    app.state.navigate(Route::Logs);
    app.refresh_view();
    assert_eq!(app.ui().name(), "ull-m3-light");

    app.set_flavor(SchemeFlavor::Dark);

    assert_eq!(app.ui().name(), "ull-m3-dark", "切主题要换主题名");
    assert_eq!(app.state.route(), &Route::Logs, "切主题不许重置 UI 状态");
    assert_eq!(app.view.screen, pages::Screen::Logs, "视图模型跟着路由");
    assert!(app.options.screenshot.is_none(), "切主题不许改启动参数");
}

#[test]
fn window_size_is_the_acceptance_size() {
    assert_eq!((WINDOW_SIZE.width, WINDOW_SIZE.height), (1180.0, 760.0));
    assert_eq!(APP_TITLE, "Unlucky Launcher");
}

#[test]
fn screenshot_mode_never_waits_for_the_system_theme() {
    // 门禁依赖：CI 的 runtime-smoke 不带主题，xvfb 里也没有主题会话。
    let options = screenshot_options();
    assert!(!options.wants_system_theme(), "截图模式不许等系统主题");
    assert_eq!(options.capture_flavor(), Some(SchemeFlavor::Light));

    let app = headless(options, SchemeFlavor::Light);
    assert!(app.has_screenshot() && app.ui().name() == "ull-m3-light");
}

#[test]
fn gallery_screen_builds_a_tree() {
    assert_eq!(Options::default().screen, Screen::Shell, "默认仍是产品骨架");
    let options = Options {
        screen: Screen::Gallery,
        ..Options::default()
    };
    let app = headless(options, SchemeFlavor::Light);
    // 真实像素由 `--screen=gallery --screenshot` 出图 + `gallery::probe` 量
    let _ = app.view();
}

#[test]
fn frames_are_only_requested_in_screenshot_mode() {
    let mut app = headless(Options::default(), SchemeFlavor::Light);
    assert!(!app.wants_frames(), "普通运行不订阅帧，避免每帧重绘");

    app.options.screenshot = Some(std::path::PathBuf::from("x.png"));
    assert!(
        app.wants_frames(),
        "截图模式必须订阅帧事件才能在第 3 帧出图"
    );
}

// --------------------------------------------------------------- P2-1 按需节拍

/// 空闲态**不要每帧节拍**（但要有 1 Hz 心跳）。
///
/// 意图与 r2 那条 `an_idle_app_asks_for_no_clock` 一致：空闲时不许按帧循环跑（那会以每秒几百帧
/// 的速度重绘整窗），改成「低频心跳」。断言拆成两半：
///
/// 1. `needs_frames()` 为假 ⇒ **不订阅** `window::frames()`；
/// 2. 空闲时钟的周期 = [`IDLE_TICK`] = 1 s，且心跳每拍推进的 tick 预算 = 30（= 1 s / 33 ms），
///    这样 `theme::config_poll_due`（30 拍）仍是「约 1 秒比对一次 mtime」。
///
/// 抓得住：有人把「无条件订阅帧」改回来（第 1 半红）；有人把心跳周期改快/改慢（第 2 半红）。
/// 抓不住：真实窗口的重绘次数（那要 `ULL_TICK_TRACE=1` 跑一次交互模式看日志，见报告）。
#[test]
fn an_idle_app_uses_a_slow_heartbeat_instead_of_frames() {
    let app = headless(screenshot_options(), SchemeFlavor::Light);
    assert!(app.needs_frames(), "截图通道永远要帧（第 3 帧落盘）");

    let mut idle = headless(Options::default(), SchemeFlavor::Light);
    let _ = idle.update(Message::Boot);
    assert!(
        idle.needs_frames(),
        "Boot 刚派发过 RefreshAll：3 秒观察窗内要节拍（等真 host 的首批事件）"
    );

    // 观察窗过期、也没有忙碌实例 ⇒ 空闲态不订阅帧事件（改走 1 Hz 心跳）
    idle.watch_until = None;
    assert!(!idle.needs_frames(), "空闲交互态：不订阅帧事件（只有心跳）");

    // 派发之后观察窗重新打开
    let _ = idle.update(Message::Page(pages::Message::Navigate(Route::Detail(
        "survival".to_string(),
    ))));
    let _ = idle.update(Message::Page(pages::Message::Launch(
        "survival".to_string(),
    )));
    assert!(idle.needs_frames(), "派发后观察窗内要有节拍");

    // 进入 Busy 之后由「忙碌实例」这个条件自己续订（不靠观察窗）
    idle.watch_until = None;
    assert!(
        idle.needs_frames(),
        "有 Busy 实例时即便观察窗过期也要继续 tick"
    );
}

/// 心跳的**节奏**：1 Hz，且每拍折算 30 个 33 ms 单位 ⇒ `config_poll_due` ≈ 每秒一次。
///
/// 抓得住：心跳周期常量、以及 `advance()` 里「按流逝时间折算 ticks」这条记账（改回
/// `ticks += 1` 会让空闲态的配置轮询变成 30 秒一次，这条就红）。
/// 抓不住：`Task::perform` 那条自续链真的每秒触发一次（那是运行期 trace 的事，见报告）。
#[test]
fn the_idle_heartbeat_budgets_one_config_poll_per_second() {
    use unlucky_presenter::theme::config_poll_due;

    assert_eq!(
        super::state::IDLE_TICK,
        std::time::Duration::from_secs(1),
        "空闲心跳周期必须是 1 s"
    );

    let mut app = headless(Options::default(), SchemeFlavor::Light);
    let _ = app.update(Message::Boot);
    app.watch_until = None;

    // 一次空闲心跳（1 s）之后：ticks 增加 30，且正好落在 config 轮询点上
    let before = app.ticks;
    app.advance(super::state::IDLE_TICK);
    assert_eq!(app.ticks - before, 30, "1 s 应折算 30 个 33 ms 拍");
    assert!(
        config_poll_due(app.ticks),
        "空闲心跳每拍都要落在 config 轮询点上（ticks={}）",
        app.ticks
    );
}

/// 空闲态每拍**不再重建视图模型**（P2-1 的另一半：`refresh_view` 的 30 Hz × N 实例白克隆）。
///
/// 抓得住：把 `advance()` 里的 `if self.view_dirty` 去掉（改回无条件 `refresh_view()`）时，
/// 这条断言里的 `refreshes` 会从「不增长」变成「每拍 +1」。
/// 抓不住：「重建一次花了多少微秒」（没有基准测试；量级判断见报告）。
#[test]
fn idle_ticks_do_not_rebuild_the_view_model() {
    let mut app = headless(Options::default(), SchemeFlavor::Light);
    let _ = app.update(Message::Boot);

    let before = app.refreshes;
    for _ in 0..100 {
        app.advance(super::state::TICK);
    }
    assert_eq!(
        app.refreshes,
        before,
        "空闲 100 拍：视图模型一次都不该重建（实际 +{}）",
        app.refreshes - before
    );

    // 有事件进来时仍然要重建（否则页面不刷新）
    let _ = app.update(Message::Page(pages::Message::Navigate(Route::Logs)));
    assert!(app.refreshes > before, "导航之后必须重建一次");
}
/// 视图模型跟着窗口宽度走（交互模式下 600 / 840 两条断点靠 resize 事件生效）。
///
/// M104 起 `view.window_width` 装的是**中栗可用宽度**（窗口 − 右栗 − 拖拽缝），所以这里按三档
/// 各钉一个**字面量**：48 的折叠条与 320 的面板各自的减法都得对。
/// （断言里不调 `agent.content_width(..)`——那是同义反复，实现错一处它照样绿。）
#[test]
fn resizing_the_window_rebuilds_the_view_model() {
    let mut app = headless(Options::default(), SchemeFlavor::Light);
    let _ = app.update(Message::Boot);

    // 出厂是 Collapsed(48) 档：中栗 = 窗口 − 48
    let _ = app.update(Message::Window(iced::window::Event::Resized(
        iced::Size::new(1180.0, 760.0),
    )));
    assert!(
        (app.view.window_width - 1132.0).abs() < f32::EPSILON,
        "1180 − 48（折叠条）= 1132，实际 {}",
        app.view.window_width
    );

    let _ = app.update(Message::Window(iced::window::Event::Resized(
        iced::Size::new(700.0, 600.0),
    )));
    assert!(
        (app.view.window_width - 652.0).abs() < f32::EPSILON,
        "700 − 48（折叠条）= 652，实际 {}",
        app.view.window_width
    );

    // Expanded(320) 档：中栗 = 窗口 − 320（面板）− 8（拖拽缝）= 852
    app.agent.expand();
    app.view_dirty = true;
    app.refresh_view();
    assert!(
        (app.view.window_width - 652.0).abs() < f32::EPSILON,
        "700 上展开档不成立（上限 700 − 88 − 472 − 8 = 132 < 280）⇒ 实际按折叠条 48 算：         700 − 48 = 652，实际 {}",
        app.view.window_width
    );
    let _ = app.update(Message::Window(iced::window::Event::Resized(
        iced::Size::new(1180.0, 760.0),
    )));
    assert!(
        (app.view.window_width - 852.0).abs() < f32::EPSILON,
        "1180 − 320 − 8 = 852，实际 {}",
        app.view.window_width
    );

    // Detached 档：右栗不占宽，中栗 = 窗口 = 1180。
    // **必须走路由**（`Select(Ai)`）：r1 之后「档位 = Detached」与「路由 = Agent」是一条不变式，
    // 直接 `agent.detach()` 再手工 refresh 会被 `sync_agent_route` 立刻收回右栗（那正是要的语义）。
    let _ = app.update(Message::Select(NavItem::Ai));
    app.view_dirty = true;
    app.refresh_view();
    assert!(
        (app.view.window_width - 1180.0).abs() < f32::EPSILON,
        "拆出后右栗不占宽：1180 − 0 = 1180，实际 {}",
        app.view.window_width
    );
    // 页面照旧能构图（窄窗退化到单列，不 panic）
    let _ = app.view();
}

/// **不变式（复审 r1 的 P2-1）**：`Route::Agent` ⟺ `RailState::Detached`。
///
/// 走的是**真实路由入口**（外壳导航第五项 `Assistant` → `Message::Select(NavItem::Ai)`），不是
/// 直接调 `agent.detach()`：r1 复现的 bug 正是「导航进页面时档位没变」，只测 `detach()` 抓不到它。
///
/// 抓得住：①进独立页后档位 = Detached（页头才会是 `Dock back` 而不是 `Open assistant`/`Pop out`）；
/// ②未读在进页时清零；③`Esc` 在独立页真的离开（不是只把档位改成折叠）；④从页上点别的导航，
/// 会话收回右栗（档位不再停在 Detached）。
/// 抓不住：按钮在像素上长什么样（那要出图，见 `13/14` 两张）。
#[test]
fn entering_the_agent_route_detaches_the_rail() {
    let mut app = headless(Options::default(), SchemeFlavor::Light);
    let _ = app.update(Message::Boot);
    assert_eq!(app.agent.rail, RailState::Collapsed, "出厂是折叠档");
    // M119：出厂没有样本会话，未读由 presenter 按真事件累加 ⇒ 出厂就是 0
    assert_eq!(app.agent_host.rail().unread, 0, "出厂 0 条未读");

    // ① 外壳导航第五项（真实入口）
    let _ = app.update(Message::Select(NavItem::Ai));
    assert_eq!(app.state.route(), &Route::Agent);
    assert_eq!(
        app.agent.rail,
        RailState::Detached,
        "进了独立页，档位必须是 Detached——否则页头会画右栗才有的动作"
    );
    assert_eq!(app.agent_host.rail().unread, 0, "进独立页即读");

    // ③ Esc 真的离开这一页
    let _ = app.update(Message::Agent(crate::shell::agent::Message::Escape));
    assert_ne!(app.state.route(), &Route::Agent, "独立页上 Esc 必须离开");
    assert_eq!(app.agent.rail, RailState::Expanded, "收回到右栗展开档");

    // ④ 从独立页点别的导航：会话收回右栗（档位不能停在 Detached）
    let _ = app.update(Message::Select(NavItem::Ai));
    assert_eq!(app.agent.rail, RailState::Detached);
    let _ = app.update(Message::Select(NavItem::Instances));
    assert_eq!(app.state.route(), &Route::Instances);
    assert_eq!(
        app.agent.rail,
        RailState::Expanded,
        "离开独立页 ⇒ 会话收回右栗（否则页走了、栏还是 0 宽）"
    );
}

/// 同一条不变式的另一个入口：页面层的 `Navigate(Route::Agent)`（右栗里的 `pages::Message` 路径）。
#[test]
fn the_page_navigation_entry_also_detaches() {
    let mut app = headless(Options::default(), SchemeFlavor::Light);
    let _ = app.update(Message::Boot);
    // 走真消息（不是直接改字段）：这样「展开即读」的 `agent_mark_read` 也会真的发生
    let _ = app.update(Message::Agent(crate::shell::agent::Message::Expand));
    assert_eq!(app.agent.rail, RailState::Expanded);

    let _ = app.update(Message::Page(pages::Message::Navigate(Route::Agent)));
    assert_eq!(app.state.route(), &Route::Agent);
    assert_eq!(app.agent.rail, RailState::Detached);
    assert_eq!(app.agent_host.rail().unread, 0);

    // 页面层回退（不是 Dock 消息）也要把档位收回来
    let _ = app.update(Message::Page(pages::Message::Navigate(Route::Instances)));
    assert_eq!(app.agent.rail, RailState::Expanded);
}

/// 截图模式不读设置文件里的主题（D30 可复现：两台机器出同一张图）。
#[test]
fn screenshot_mode_ignores_the_settings_file_theme() {
    let mut app = headless(screenshot_options(), SchemeFlavor::Light);
    app.apply_config_theme();
    assert_eq!(app.ui().name(), "ull-m3-light", "截图模式的主题只认命令行");
}

/// 主题优先级：`--theme` > 配置文件 `appearance.theme_mode` > 系统主题。
///
/// r2 之前 `Message::SystemTheme` 无条件 `set_flavor`，于是「配置文件写 dark、系统是浅色」会
/// 出现 `light -> dark -> light` 的一次抖动、配置永远不生效（r3 做 P2-2 的配置热切实测时暴露
/// 出来，一并修）。
///
/// 抓得住：系统主题不许盖掉显式配置/命令行给出的主题（三条分支各一条断言）。
/// 抓不住：真实桌面切换系统主题时的事件时序（只有消息级验证）。
#[test]
fn the_system_theme_never_overrides_an_explicit_one() {
    use unlucky_presenter::prefs::ThemeMode;

    // ① auto（内存桥默认）：系统主题说了算
    let mut auto = headless(Options::default(), SchemeFlavor::Light);
    assert_eq!(auto.theme_mode, ThemeMode::Auto);
    let _ = auto.update(Message::SystemTheme(iced::theme::Mode::Dark));
    assert_eq!(auto.ui().name(), "ull-m3-dark", "auto 模式要跟随系统");

    // ② 配置文件显式 dark：系统给浅色也不许改
    let mut pinned = headless(Options::default(), SchemeFlavor::Dark);
    pinned.theme_mode = ThemeMode::Dark;
    let _ = pinned.update(Message::SystemTheme(iced::theme::Mode::Light));
    assert_eq!(
        pinned.ui().name(),
        "ull-m3-dark",
        "配置文件说 dark，系统说 light：不许改"
    );

    // ③ 命令行 `--theme` 优先级最高
    let mut explicit = headless(
        Options {
            theme: Some(SchemeFlavor::Dark),
            ..Options::default()
        },
        SchemeFlavor::Dark,
    );
    let _ = explicit.update(Message::SystemTheme(iced::theme::Mode::Light));
    assert_eq!(
        explicit.ui().name(),
        "ull-m3-dark",
        "--theme 优先级最高，系统主题不许盖"
    );
}
