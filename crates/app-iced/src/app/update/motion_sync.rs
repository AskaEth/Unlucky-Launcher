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

        // M300：左轨的折叠档**对表**（幂等）。泵的目标出厂是「展开」，而用户可能上次就是折叠着
        // 退出的（`ui.nav.collapsed` 落盘）。不对表的话，启动后第一次**展开**会被
        // `observe_nav` 的「目标没变」分支判成空操作 ⇒ 跳变没有动画（T37：数值对，但没人调它）。
        //
        // 判据是「没有左轨轨道在跑」而不是「只做一次」：动画期间它不许动目标（否则每一拍都会把
        // 过渡掐掉），而轨道结束后它自然变成空操作（目标已经等于真实档位）。
        if !motion::nav_running() {
            motion::snap_nav(!self.state.prefs.nav_collapsed);
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
        //
        // M277：**这一档本来就没有旧页时也必须丢**。判据是 `Kind::has_old_page()`（M256 写好的
        // 设计口径，此前全仓无人调用 ⇒ 首屏照样画幽灵）。为什么它必须在这里：`pages::render` 把
        // `previous_view` 整棵画在当前页**下面**，只在空隙里露出来——而 boot 那份快照是
        // `construct.rs` 的零实例初始模型（被 `refresh.rs` 在第一次 refresh 时换进去），
        // 于是「每次启动闪一张『还没有实例…』的空态页」约 250~300ms。
        // 为什么不在 `refresh.rs` 存之前拦：本 mission 的 scope 不含 `app/state/**`；
        // 而这里比"存之前拦"更靠下游一格，任何来源（含将来新增的）都被一并兜住。
        // 丢在这里的时机是 `update` 的尾巴、`view` 之前 ⇒ 一帧都画不到。
        let keeps_old_page = motion::kind().is_none_or(motion::Kind::has_old_page);
        if !motion::needs_frames() || !keeps_old_page {
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
mod tests;
