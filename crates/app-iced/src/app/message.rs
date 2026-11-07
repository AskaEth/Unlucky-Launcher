//! 应用消息：单一大枚举。
//!
//! 纪律：本枚举是**唯一**的消息类型，`view` 产出的所有回调都必须落到这里；组件库与页面层
//! 各自用自己的 `Message`（组件契约 §2 的泛型纪律），由 `view` 做一次 `.map` 接上。
//!
//! ## 三类输入（M90）
//!
//! | 来源 | 变体 | 落点 |
//! |---|---|---|
//! | 窗口/框架 | `Boot` / `WindowReady` / `Window` / `Frame` / `Captured` / 三个 caption | [`super::update`] 直接处理 |
//! | **时间** | `Frame`（忙碌，33 ms 闸门）+ `Heartbeat`（空闲，1 Hz） | presenter 的 TICK 节拍：`backend.tick` + `drain_events` + `apply` |
//! | **页面** | `Page(pages::Message)` | 导航 → `AppState::navigate`；启动/停止 → `AppCommand` |

use iced::{window, window::Direction};

use unlucky_presenter::router::Route;

use crate::pages;
use crate::shell::NavItem;

use super::components;

/// 消息（单一大枚举）。
#[derive(Debug, Clone)]
pub enum Message {
    /// 启动即发一次（`iced::application` 的 boot 闭包）。
    Boot,
    /// `window::latest()` 的结果。
    WindowReady(Option<window::Id>),
    /// 系统主题探测结果（只可能在**交互模式**出现，截图模式必有主题）。
    SystemTheme(iced::theme::Mode),
    /// 一帧画完。**只在需要 33 ms 节拍时订阅**：截图通道 / 有 Busy-Running 实例 / 有未过期
    /// toast / 派发后的 3 s 观察窗（见 `subscription::needs_frames`）。
    Frame,
    /// **空闲态**的 1 Hz 心跳：`Task::perform(sleep(1s))` 的自续链，用来替代空闲时的帧循环
    /// （M90 r3 / P2-2）。它保证 `theme::config_poll_due` 仍按「30 × 33 ms ≈ 1 秒」的
    /// presenter 语义跑，外部 CLI/AI 改配置后主题 1 秒内生效。
    Heartbeat,
    /// 窗口事件（M90：`Resized` 决定布局档位，600 / 840 两条断点靠它生效）。
    Window(window::Event),
    /// 窗口句柄（M93：`window::run` 探测出的 raw handle；`None` = 拿不到，整窗 α 无法下发）。
    WindowHandle(Option<isize>),
    /// 页面消息（导航 / 启动 / 停止 / 占位动作）。
    Page(pages::Message),
    /// **AI 助手栏 / 独立页**的消息（M104：三档状态、输入框、拖拽、键盘）。
    ///
    /// 为什么不走 `pages::Message`：助手栏是**外壳级视图**（它挂在任何页面右边），不是某一页的
    /// 消息；塞进页面枚举还要动 `app/update/page_messages.rs` 的穷尽 match（那个文件正被未合的
    /// M101 改），绕开它就少一处 rebase 面。落点是 `app/update/agent.rs`。
    Agent(crate::shell::agent::Message),
    /// 外壳导航轨点选（`src/shell/nav.rs` 的回调仍然发这一条；收到就翻成同一个路由跳转）。
    ///
    /// M90 起产品界面用 `pages` 里的真 `NavigationRail`，这条变体是给**外壳回退路径**
    /// （`Route::Agent`）留的——`shell` 不在本 mission 的 scope 里，不能删它的消息。
    Select(NavItem),
    /// 按下标题栏 → 拖窗。
    DragWindow,
    /// 边缘热区：开始缩放（方向由坐标决定）。
    ResizeStart(Direction),
    /// 最小化。
    Minimize,
    /// 最大化/还原。
    ToggleMaximize,
    /// 关窗退出。
    Close,
    /// 截图回来了。
    Captured(window::Screenshot),
    /// gallery（组件库 harness）里的演示交互。
    Demo(components::gallery::DemoMessage),
}

/// 外壳导航项 → 路由（导航项只在一级页之间有对应；AI 项对应 [`Route::Agent`]）。
///
/// 这是 `NavItem → Route` 的**唯一**转换点：外壳（`src/shell/nav.rs`）与页面导航轨
/// （`pages::NAV`）最终都落到同一个 `Route`，所以两条入口不会各走一套页面语义。
pub(super) fn route_of(nav: NavItem) -> Route {
    match nav {
        NavItem::Instances => Route::Instances,
        NavItem::Download => Route::Download,
        NavItem::Logs => Route::Logs,
        NavItem::Settings => Route::Settings,
        NavItem::Ai => Route::Agent,
        // 轨底头像位（M114）：账户是一级页，但**不在主序列**里
        NavItem::Account => Route::Accounts,
    }
}

/// 路由 → 外壳导航项（[`route_of`] 的逆；外壳 rail 与 caption 键都按它高亮）。
///
/// `Route::Detail(_)` 高亮「实例」（与 `Route::nav()` 同源）。
pub(super) fn nav_item_of(route: &Route) -> NavItem {
    match route {
        Route::Instances | Route::Detail(_) => NavItem::Instances,
        // M109：内容库与版本管理共用「下载」那一个导航项（B1 裁定）
        Route::Download | Route::Content => NavItem::Download,
        Route::Logs => NavItem::Logs,
        Route::Settings => NavItem::Settings,
        Route::Agent => NavItem::Ai,
        Route::Accounts => NavItem::Account,
    }
}
