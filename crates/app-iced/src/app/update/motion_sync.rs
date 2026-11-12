//! 动效的**起跑点**（M192 r1 复审 P1-1 修的正是这里）。
//!
//! ## 为什么这条不能在 `view` 里
//!
//! iced 0.14 的顺序：`update()`（**订阅表在这里求值**，iced_winit `src/lib.rs:1338`）→
//! `build_user_interfaces()`（这时才调 `view`，`src/lib.rs:1233`）⇒ **订阅表的求值早于视图**。
//! 第一版把起跑写在 `view` 里，订阅表当拍看不见轨道，`frames()` 要等下一条消息才打开，而那一帧
//! 的 dt 是整段空闲 ⇒ 轨道被当成早已到期清掉，用户看到的是一次 12/8px 的偏移闪跳（截图通道
//! 因为 `has_screenshot()` 恒真，**结构上**不可能暴露这条）。所以起跑搬到这里。
//!
//! ## 标识从哪来
//!
//! 从**路由**取（`AppState::route()` 是唯一真源）：`screen_key` 说「归不归 pages 画」，
//! `nav_rank` 说「在**导航轨**上的左右次序」（方向只看它，不看屏键大小——见 [`nav_rank`]）。

use unlucky_presenter::router::Route;

use crate::app::components::motion;
use crate::pages;

use super::boot_env::MotionFixture;
use crate::app::message::nav_item_of;
use crate::shell::NavItem;

use super::*;

impl App {
    /// 每条消息处理完之后同步一次内容标识（**必须在 `update` 里**，理由见模块文档）。
    pub(in crate::app) fn sync_motion(&mut self) {
        // M256：模态退场走完 ⇒ **这一刻**才把保留的框模型丢掉。
        // 逻辑状态早就在用户点下去的那一拍关了（`state.dialogs` 已清），这里丢的只是"还在缩小的
        // 那一份画面"。同一拍里置 `view_dirty`，下一帧就没有模态层了。
        // M256 r2（P2-2）：退场走完只在**队列里确实没有框**时才丢掉画面模型——退场途中开了新框时
        // `observe_modal(true)` 已把旧退场当场收尾（并留下一次「已关」），照样置空会闪一下。
        if motion::take_modal_closed() && self.state.dialogs.current().is_none() {
            self.dialog_model = None;
            self.view_dirty = true;
        }

        // **还没有窗口就什么都不做**（也不记标识）：动效是画出来的东西，没有窗口时
        // `window::frames()` 一条消息也不会有，起跑只会白占一档。更要紧的是「首屏入场」的语义——
        // 等窗口到了再算「第一眼看到的内容」，否则首屏那一下会被启动期的空树吃掉。
        // 截图通道例外：它的窗口句柄也是异步到的，而序列图必须从第 1 帧就按钉住的相位画。
        if self.window.is_none() && !self.has_screenshot() {
            return;
        }
        let route = self.state.route();
        let screen = screen_key(route);
        let detail = match route {
            Route::Detail(id) => Some(id.as_str()),
            _ => None,
        };
        motion::observe(screen, nav_rank(route), detail);
        // M256：模态的**逻辑**开关（队列里有没有框）。用 presenter 的队列而不是 `dialog_model`：
        // 退场期间框还画着（`dialog_model` 仍是 `Some`），但逻辑上已经在关的路上了。
        motion::observe_modal(self.state.dialogs.current().is_some());

        // M256：过渡结束（没有轨道了）当拍就**丢掉旧页快照**——`previous_view` 是一份完整的页面
        // 模型，留着就是「每换一次页多一份」的线性增长（长时间常驻的启动器上不是"小浪费"）。
        if !motion::needs_frames() {
            self.previous_view = None;
        }
    }

    /// 出图取证：让这一次进程真的发生一次「换页」或「列表 → 详情」（见 [`super::boot_env::MotionFixture`]）。
    pub(in crate::app) fn motion_fixture(&mut self) {
        let Some(fixture) = super::boot_env::env_motion_fixture() else {
            return;
        };
        let route = match (fixture, self.frames) {
            // 列表 → 详情：第 1 帧先回列表（详情收起来），第 2 帧再点进第一条实例。
            (MotionFixture::Detail, 1) => Route::Instances,
            (MotionFixture::Detail, 2) => {
                let Some(first) = self
                    .state
                    .tiles()
                    .first()
                    .map(|cell| cell.instance.id.clone())
                else {
                    return;
                };
                Route::Detail(first)
            }
            (MotionFixture::Route(route), 1) => route,
            _ => return,
        };
        diag!("[motion] ULL_MOTION_FIXTURE → {route:?}");
        self.state.navigate(route);
        self.view_dirty = true;
        self.refresh_view();
    }

    /// 真机空闲取证：`ULL_MOTION_FIXTURE_AT=<秒>` 到点就**真的**导航一次（`on_heartbeat` 调）。
    ///
    /// 为什么不用点击：[`super::boot_env::env_motion_fixture_at`] 的文档写了原因——本机会话没有
    /// 前台窗口，合成鼠标事件递不进 iced 的输入层。心跳这条路与点击在 `update` 之后是同一段代码。
    /// 返回 `true` = 这一拍真的跳了页。
    pub(in crate::app) fn motion_fixture_after_idle(&mut self) -> bool {
        use std::sync::atomic::{AtomicU32, Ordering};
        /// 已经过去的心跳数（进程内一次；只有取证开关打开时才会用到）。
        static FIXTURE_BEATS: AtomicU32 = AtomicU32::new(0);

        let Some(target) = boot_env::env_motion_fixture_at() else {
            return false;
        };
        let beat = FIXTURE_BEATS.fetch_add(1, Ordering::Relaxed) + 1;
        if beat != target {
            return false;
        }
        let route = match boot_env::env_motion_fixture() {
            Some(MotionFixture::Route(route)) => route,
            Some(MotionFixture::Detail) => {
                let Some(id) = self
                    .state
                    .tiles()
                    .first()
                    .map(|cell| cell.instance.id.clone())
                else {
                    return false;
                };
                Route::Detail(id)
            }
            None => Route::Download,
        };
        diag!("[motion] ULL_MOTION_FIXTURE_AT 到点（第 {beat} 次心跳）→ {route:?}");
        self.state.navigate(route);
        self.view_dirty = true;
        self.refresh_view();
        true
    }
}

/// 路由 → 屏的稳定键（`pages::Screen::ALL` 的下标 + 1；`None` = 不归 pages 画）。
///
/// 与 `pages::Screen::of` 同一判据（`Screen::Instances` ↔ `Route::Instances | Route::Detail(_)`），
/// 只是把「有没有这一屏」也表达出来——`pages::Screen::of` 给的是 `Option<Screen>`，这里
/// 再折成 `Option<u64>` 给帧泵用。
fn screen_key(route: &Route) -> Option<u64> {
    let screen = pages::Screen::of(route)?;
    pages::Screen::ALL
        .iter()
        .position(|candidate| *candidate == screen)
        .map(|index| index as u64 + 1)
}

/// 这一屏在**导航轨**上的次序（`NavItem::ALL` 的下标）——左右方向的唯一真源。
///
/// 为什么不能用屏键（`Screen::ALL` 的下标）：两处顺序不同。`Screen::ALL` 是
/// `… Content / Settings / Logs`，而导航轨是 `… Logs / Settings / Ai` ⇒ 用屏键会把
/// **Logs↔Settings 的左右镜像**（复审 P2-1）。`Content` 与 `Download` 共用导航项，`rank` 相等，
/// 方向退回按屏键比大小（`Download ↔ Content` 判成同级水平位移）。
///
/// 账户头像（`NavItem::Account`）与 AI（`NavItem::Ai`）不在这个集合里 / 不归 pages 画，
/// 返回 `None`——它们的切换本来就不起跑页面轨道。
fn nav_rank(route: &Route) -> Option<u8> {
    let item = nav_item_of(route);
    NavItem::ALL
        .iter()
        .position(|candidate| *candidate == item)
        .map(|index| index as u8)
}

#[cfg(test)]
mod tests {
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
}
