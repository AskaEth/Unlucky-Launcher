//! 动效的**起跑点**（M192 r1 复审 P1-1 修的正是这里）。
//!
//! ## 为什么这条不能在 `view` 里
//!
//! iced 0.14 的处理顺序是：
//!
//! ```text
//! 消息 ──► update()            ← program.subscription() 在这里求值（iced_winit src/lib.rs:1338）
//!       ──► build_user_interfaces()   ← 这时才调 program.view（src/lib.rs:1233）
//! ```
//!
//! 也就是说**订阅表的求值早于视图**。第一版把「内容标识变了 → 起跑」写在 `pages::render`
//! （= `view`）里，于是同一拍订阅表看不见轨道，`frames()` 要等下一条消息（最快鼠标抬起、
//! 最慢 1 Hz 心跳）才打开；那一帧的 dt 又是整段空闲时长 ⇒ 轨道被当成「早已到期」清掉，
//! 用户看到的是一次 12/8px 的偏移闪跳，比硬切更难看。截图通道因为 `has_screenshot()` 恒真，
//! **结构上**不可能暴露这条。
//!
//! 所以起跑搬到这里：`App::update` 处理完每一条消息后调 [`App::sync_motion`]，
//! 于是「轨道存在」对**同一拍**的 `subscription()` 可见，帧订阅当拍打开。
//!
//! ## 标识从哪来
//!
//! 从**路由**取（`AppState::route()` 是唯一真源），不从视图模型取：账户页 / AI 独立页不走
//! `pages::render`，用路由才能表达「这一屏不归 pages 画」（`None`）——它们之间的切换不起跑，
//! 而「从它们回到某一屏」照旧跑换页那一档（复审 r1 结论 4 的硬切顺手收口）。

use unlucky_presenter::router::Route;

use crate::app::components::motion;
use crate::pages;

use super::boot_env::MotionFixture;

use super::*;

impl App {
    /// 每条消息处理完之后同步一次内容标识（**必须在 `update` 里**，理由见模块文档）。
    pub(in crate::app) fn sync_motion(&mut self) {
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
        motion::observe(screen, detail);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 路由 → 屏键：五个一级页各自唯一，`Detail` 归「实例」，账户/AI 不归 pages。
    #[test]
    fn only_the_five_screens_get_a_key() {
        let keys: Vec<Option<u64>> = [
            Route::Instances,
            Route::Download,
            Route::Content,
            Route::Settings,
            Route::Logs,
            Route::Detail("survival".to_string()),
            Route::Accounts,
            Route::Agent,
        ]
        .iter()
        .map(screen_key)
        .collect();

        assert_eq!(&keys[..5], &[Some(1), Some(2), Some(3), Some(4), Some(5)]);
        assert_eq!(
            keys[5],
            Some(1),
            "详情页归「实例」那一屏（与 Screen::of 同源）"
        );
        assert_eq!(keys[6], None, "账户页不归 pages 画");
        assert_eq!(keys[7], None, "AI 独立页不归 pages 画");
    }
}
