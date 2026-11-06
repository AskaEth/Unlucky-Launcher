//! 节拍：推进一拍 / 派发指令 / 抽干事件（M119 从 `update.rs` 拆出来守 400 行硬线；纯搬不改）。
//!
//! ## 为什么这三件必须在一起
//!
//! `advance` → `pump` → `apply` → `refresh_view` 是同一条链：时间推进、事件抽干、状态落地、
//! 视图重建。拆开看每一段都无害，合起来才是「presenter 的异步事件在 UI 侧怎么被消费」的答案。
//!
//! ## 两个时钟的相位（M90 r3 / P2-2 的老账，代码里有细节）
//!
//! | 场景 | 时钟 | 周期 |
//! |---|---|---|
//! | 截图 / 有 Busy-Running 实例 / 有 toast / 派发后 3 s 观察窗 | `window::frames()` + 闸门 | 33 ms |
//! | 其余（空闲） | `Heartbeat` 自续链 | 1 s |
//!
//! ## M119 增加了什么
//!
//! [`App::pump`] 里多了一行 [`App::drain_agent`]：AI 助手的事件队列和 core 的事件队列同拍抽干，
//! 于是「右栗的未读/流式状态」和页面快照共享同一个刷新节奏（不会一个 30 Hz、一个 1 Hz）。

use std::time::{Duration, Instant};

use unlucky_presenter::dto::AppCommand;
use unlucky_presenter::theme::CONFIG_POLL_TICKS;

use super::super::state::TICK_MS;
use super::*;

impl App {
    /// 推进一拍：后端消化时间 → 抽干事件 → 过期 toast → 偶发重读配置文件。
    ///
    /// `elapsed` 是真实流逝时间（毫秒级抖动是正常的：`MockBackend` 的脚本按累计时长走）。
    pub(in crate::app) fn advance(&mut self, elapsed: Duration) {
        self.backend.tick(elapsed);
        self.pump();

        self.state
            .toasts
            .expire(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX));

        // 设备码/授权码的倒计时（M114）：等待态每秒都要重画一次，否则「剩 900s」永远不动，
        // 用户会以为卡死了——这正是任务里「不得只转圈」的那一条。
        if self.state.device_login.phase == unlucky_presenter::dto::DeviceLoginPhase::Waiting {
            self.view_dirty = true;
        }
        // `ticks` 的单位是**33 ms 的一拍**，不是「advance 被调了几次」：空闲心跳一次跨 1 s，
        // 要记 30 拍，这样 `theme::config_poll_due`（30 拍 ≈ 1 秒）在两种时钟下语义一致。
        let ticks = elapsed.as_millis().div_euclid(u128::from(TICK_MS));
        let ticks = u64::try_from(ticks).unwrap_or(u64::MAX).max(1);
        let bucket_before = self.ticks / 30;
        self.ticks += ticks;
        let bucket_after = self.ticks / 30;

        // 外部（AI / CLI）改了配置文件：主题跟着换，其余外观项等玻璃/背景 mission。
        //
        // 判据用**跨越**而不是 presenter 的 `config_poll_due(ticks)`（整除）：两个时钟的相位不同
        // （帧节拍按 33 ms 累加、空闲心跳按 30 拍累加），整除判定一旦错过一次就**再也命中不了**
        // ——实测 ticks=137 之后每拍 +30 永远不整除，配置轮询静默死掉。间隔真源仍是
        // `CONFIG_POLL_TICKS`（30 拍 × 33 ms ≈ 1 秒）。截图通道不轮询（D30 可复现）。
        let poll_due = self.ticks >= self.config_due_at;
        if poll_due {
            // 追到「当前 ticks 之后的第一个间隔点」，避免心跳把相位拖飞
            self.config_due_at = self.ticks + CONFIG_POLL_TICKS;
        }
        let reloaded =
            !self.has_screenshot() && poll_due && self.bridge.reload_if_changed().is_some();
        if reloaded {
            eprintln!("[settings] 配置文件变了，重读主题 + 外观");
            self.apply_config_theme();
            // M93：外观也热切（面板 α / 背景图 / 配色来源 / 整窗 α 都在里面）。
            self.reload_appearance();
        }

        if self.view_dirty {
            self.refresh_view();
        }

        // 取证开关（`ULL_TICK_TRACE=1`）：每跨过 30 拍（≈1 秒）打一行。
        // 忙碌态由帧节拍跨桶（≈1 行/秒），空闲态由心跳一次 +30 跨桶（**也是 1 行/秒**）——
        // 两种时钟都用同一把尺子，所以 trace 的密度本身就是「时钟在跑」的证据。
        if bucket_after != bucket_before && std::env::var_os("ULL_TICK_TRACE").is_some() {
            eprintln!(
                "[tick] n={} frames={} refreshes={} clock={}",
                self.ticks,
                self.frames,
                self.refreshes,
                if self.needs_frames() {
                    "frames"
                } else {
                    "idle-1hz"
                }
            );
        }
    }

    /// 派发一条指令并**立刻**抽干它带出来的事件（不本地回显，等价于 Slint 版的 `dispatch`）。
    pub(in crate::app) fn dispatch(&mut self, command: AppCommand) {
        self.backend.dispatch(command);
        // 观察窗：命令发出后 3 秒内保持帧订阅，等后端把第一条事件交回来
        // （真 host 是异步的，派发瞬间状态还没变；不设这个窗口，空闲态的帧订阅永远不会打开）。
        self.watch_until = Some(Instant::now() + WATCH_WINDOW);
        self.pump();
    }

    /// 抽干事件队列并逐条 `apply`（**唯一**的状态写入口）。
    pub(in crate::app) fn pump(&mut self) {
        // M119：AI 助手的事件队列与 core 的事件队列**同拍**抽干（顺序无关：两个队列互不依赖）。
        // 抽出来的这批事件经 `Message::Agent(Events)` 回流，见 `update/agent.rs`。
        self.drain_agent();

        let events = self.backend.drain_events();
        if events.is_empty() {
            return;
        }
        for event in events {
            self.state.apply(event);
        }
        self.view_dirty = true;
        self.auto_select();
        self.refresh_view();
    }

    /// 首屏快照到位后**自动选中第一行**（桌面 list-detail 的惯例：右栏不该开局就是空的）。
    ///
    /// 只做一次：用户点「返回」回到列表后，再刷新实例列表也不会把他弹回详情页。
    /// 选中这件事走的是 `AppState::navigate`（路由栈），不是本地选中字段——于是「当前选中」
    /// 永远等于「路由栈顶」，用户点返回时栈语义自然成立。
    pub(in crate::app) fn auto_select(&mut self) {
        if self.auto_selected || self.state.route() != &Route::Instances {
            return;
        }
        let Some(first) = self
            .state
            .tiles()
            .first()
            .map(|cell| cell.instance.id.clone())
        else {
            return;
        };
        self.auto_selected = true;
        self.state.navigate(Route::Detail(first));
        self.view_dirty = true;
    }
}
