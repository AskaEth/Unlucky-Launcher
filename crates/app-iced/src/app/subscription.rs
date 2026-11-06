//! 订阅：**按需**帧节拍（presenter 的 TICK）+ 窗口尺寸事件。
//!
//! ## 为什么要「按需」
//!
//! iced_winit 在处理完任何一条消息后都会 `request_redraw()`，而 `window::frames()` 监听的是
//! `RedrawRequested` —— 于是「订阅帧 → 收 Frame → update → 再触发重绘」是一条**自持循环**：
//! 只要订阅着，窗口就永远在重绘（M90 r1 就是这样，CPU 空转）。
//!
//! 现在只在**真的需要 33 ms 节拍**的时候订阅（[`App::needs_frames`]）：
//!
//! | 条件 | 为什么需要 33 ms 节拍 |
//! |---|---|
//! | 截图通道 | 第 3 帧要落盘 |
//! | 有 `Busy` / `Running` 实例 | 启动进度、日志尾随都靠 `backend.tick` 推进 |
//! | 有未过期的 toast | toast 按毫秒倒计时 |
//! | 刚派发过指令（3 s 观察窗） | 真 host 的异步事件要靠节拍抽出来 |
//!
//! 空闲态**一条帧消息都不来** ⇒ 窗口不重绘、`refresh_view` 不跑（配合 `view_dirty`）。
//!
//! ## 空闲态的时钟从哪来（M90 r3 / P2-2）
//!
//! r2 把空闲的帧订阅掐掉之后，`config_poll_due` 就再也没被调用过 —— 外部 CLI / AI 工具改
//! `launcher.json` 后主题**永不生效**（不是「等下次交互」，是「等下一次 dispatch」）。这与
//! D4「AI 能读改 ULL 配置」直接冲突。
//!
//! 所以空闲态换成一个 **1 Hz 的自续心跳**（[`crate::app::message::Message::Heartbeat`]，
//! 由 `Task::perform(sleep(1s))` 驱动，见 `update::heartbeat`）：每秒醒一次、推进一拍
//! presenter（内部按 33 ms 折算 30 拍，于是 `config_poll_due` 仍是「约 1 秒比对一次 mtime」）。
//!
//! 为什么不用 `window::frames()` + 1 s 闸门来实现这个低频时钟：帧订阅**本身就是**自持重绘循环
//! （iced_winit 处理完任何消息都 `request_redraw()`），闸门只能省掉 `advance()` 里的活，省不掉
//! 整窗重绘——实测软件渲染约 **500 帧/秒**。心跳是「每秒一帧」，差三个数量级。
//!
//! ## 已知代价（都登记，不藏）
//!
//! 1. **忙碌态仍是自持帧循环**：`Running`/`Busy` 一天不结束，就一天按帧刷（软件渲染实测约
//!    970 帧/秒，refreshes 不增长 = 纯像素空转）。功能上需要 tick 来发现游戏退出，但「整段
//!    运行期近千帧/秒重绘」是真实开销；要收口得让 `Running` 走更慢的时钟（follow-up）。
//! 2. 心跳用 `thread::sleep` 占线程池一个线程 1 秒；1 Hz 下可接受，但不是优雅解法（正解仍是
//!    `iced::time::every` + tokio/smol，Cargo.toml 在 scope 外，已报 finding）。

use std::time::Instant;

use iced::{window, Subscription};

use super::message::Message;
use super::state::App;

impl App {
    /// 本次进程的订阅表。
    pub(super) fn subscription(&self) -> Subscription<Message> {
        let frames = if self.needs_frames() {
            window::frames().map(|_| Message::Frame)
        } else {
            Subscription::none()
        };

        // 断点 600 / 840 要跟着真窗口宽度走；截图模式的窗口尺寸由参数钉死，不吃 resize。
        let events = if self.has_screenshot() {
            Subscription::none()
        } else {
            window::events().map(|(_, event)| Message::Window(event))
        };

        // 键盘（M104）：`Esc` 收栗 / `Tab` 走焦点环 / `Enter` 激活焦点键。
        //
        // 为什么订阅整条键盘流而不是自己挑键：`iced::keyboard::listen()` 是唯一的全窗键盘入口，
        // 挑键与「什么状态下该做什么」的解释权放在 `app/update/agent.rs`（一眼看完），
        // 订阅表只负责搬运。截图通道不开它——D30 要可复现，键盘事件一个都不该进图。
        let keys = if self.has_screenshot() {
            Subscription::none()
        } else {
            iced::keyboard::listen()
                .map(|event| Message::Agent(crate::shell::agent::Message::Key(event)))
        };

        Subscription::batch([frames, events, keys])
    }

    /// 是否订阅帧事件（= 截图模式要帧数闸门）。
    pub(super) fn wants_frames(&self) -> bool {
        self.has_screenshot()
    }

    /// 现在要不要**时钟**（见模块文档的表）。
    pub(super) fn needs_frames(&self) -> bool {
        self.has_screenshot()
            || !self.state.toasts.is_empty()
            || !self.state.running().is_empty()
            || self
                .watch_until
                .is_some_and(|deadline| Instant::now() < deadline)
    }
}
