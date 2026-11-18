//! Unlucky-Launcher 的 **UI 框架无关宿主层**（presenter）。
//!
//! # 1. 这层为什么存在
//!
//! D26 的技术路线是「用 iced 重写 UI」。重写只应该换掉**渲染**，不该重写业务装配——
//! 本 crate 就是被两个 UI 版共用的那一半：Slint 版（`crates/app`）与 iced 版
//! （`crates/app-iced`）都从这里取 DTO、状态 reducer 与 host。Slint 退役后只剩 iced
//! 依赖它，本 crate 不受影响。
//!
//! 判据（M85 已核）：这套代码原本住在 `crates/app/src`，但 `host/`（25 个文件）、
//! `state.rs`、`settings_bridge.rs`、`dto/` 里**没有一处** UI 框架符号；它们依赖的只有
//! 标准库、serde 与 `unlucky-core`。把它们留在 UI crate 里等于让 iced 重写跟着重写一遍
//! 约 5400 行，白担行为漂移的风险。
//!
//! # 2. 对外表面（UI 侧的唯一入口）
//!
//! ```text
//! host::CoreHost ──impl Backend──► dto::AppEvent ──► state::AppState::apply ──► dto 快照
//!        ▲                                              │
//!        └──────────── dto::AppCommand ◄────────────────┘
//! ```
//!
//! | 模块 | 入口 | UI 侧怎么用 |
//! | ---- | ---- | ----------- |
//! | [`dto`] | [`dto::Backend`] / [`dto::AppCommand`] / [`dto::AppEvent`] / 各 `*View` 快照 | 派发指令、消费事件、把快照字段映射成视图模型 |
//! | [`host`] | [`host::CoreHost`]（实现 [`dto::Backend`]）、[`host::HostEnv`] | 进程启动时构造一次，之后只当 `Box<dyn Backend>` 用 |
//! | [`state`] | [`state::AppState`]（`apply` / `navigate` / 各页面字段） | **唯一**状态入口：事件进来先 `apply`，再从字段取值画 |
//! | [`agent`] | `Presenter`（`agent_rail` / `agent_submit` / `agent_mark_read` / `agent_confirm`）、`AgentRail` | AI 助手的唯一状态入口：读 rail 画、把点击翻成这四个方法 |
//! | [`prefs`] | `UiPrefs` / `ThemeMode` / `TileSize` / `AppearancePrefs` / `keys` | 读字段做展示；不自己写盘 |
//! | [`settings_bridge`] | `SettingsBridge`（`open` / `load` / `persist` / `reload_if_changed`） | 启动读一次、变更时写回、每拍问一次「变了吗」 |
//! | [`theme`] | `CONFIG_POLL_TICKS` / `BACKGROUND_KEY` 一类纯判据、`tile_viewport` / `system_theme_value` | 「这一拍该不该重读/重建」由它回答，UI 只执行 |
//! | [`router`] | `Route` / `Router` / `AiPanelMode` | 导航状态 |
//! | [`dialogs`] | `DialogQueue` / `DialogRequest` | 模态队列 |
//! | [`toast`] | `ToastCenter` / `Toast` | 非阻塞提示 |
//! | [`wizard`] | `WizardState` | 新建实例向导的纯状态机 |
//! | [`mock`] / [`mock_data`] | `MockBackend` / 样例数据 | 无数据根时的演示后端、UI 回归 |
//! | [`backend`] | `AppOptions` / `build_backend` / `config_path` | 启动时选真实 host 还是 mock |
//!
//! # 3. UI 侧允许做什么、禁止做什么
//!
//! 允许（**只做展示与转发**）：
//!
//! - 把本 crate 的值翻译成框架类型（视图模型、颜色、图标、字符串）并灌进属性/`Element`；
//! - 回调里把用户动作翻成 [`dto::AppCommand`] 交给 [`dto::Backend::dispatch`]，再 `drain_events()` 喂
//!   [`state::AppState::apply`]——**不要**在本地做乐观回显（会话气泡/磁贴状态都以事件为准）；
//! - 框架能力：窗口、定时器/订阅、图片解码与模糊层、截图落盘、字体加载。
//!
//! 禁止：
//!
//! - 在本 crate 里出现 Slint / iced / winit 类型（守门命令见 `crates/app/Cargo.toml` 的注释：
//!   自指问题让那条命令不能写在本 crate 里，否则它自己就被抓出来了）；
//! - UI 侧绕过 [`state::AppState::apply`] 直接改状态字段（会与事件流打架，磁贴/日志会漂）；
//! - UI 侧直接 `use unlucky_core::…` 组装业务（读盘、起进程、拼路径、写设置一律走 host/bridge）；
//! - UI 侧自己造 `<data root>` 路径：路径只从 [`host::HostEnv`] / [`settings_bridge`] 拿。
//!
//! # 4. iced 接线的推荐用法（骨架，伪代码）
//!
//! ```ignore
//! // 1) 启动：选后端（数据根解析不出来自动退回 mock）
//! let backend: Box<dyn Backend> = build_backend(&AppOptions::default());
//! let bridge = SettingsBridge::open();          // 读盘一次，主题/外观看这里
//! let state = AppState::new();
//!
//! // 2) 订阅：iced 的 30fps 订阅里推进一拍
//! fn subscription() -> Subscription<Message> { iced::time::every(TICK).map(|_| Message::Tick) }
//! fn update(&mut self, Message::Tick) {
//!     backend.tick(TICK);                        // 后台线程的消息在这里汇聚
//!     for event in backend.drain_events() { self.state.apply(event); }   // 唯一的写状态入口
//!     if config_poll_due(self.ticks) {           // presenter 的纯判据
//!         if let Some((appearance, theme)) = self.bridge.reload_if_changed() { … }
//!     }
//!     self.view = ViewModel::from(&self.state);  // 展示转换在 iced 侧做
//! }
//!
//! // 3) 回调：用户动作 → AppCommand → 事件回流（不本地回显）
//! fn update(&mut self, Message::Launch(id)) { self.dispatch(AppCommand::Launch(id)); }
//! ```
//!
//! 要点：`state` / `bridge` / `backend` 三件由 UI 侧持有并驱动，`presenter` 不持有定时器也不
//! 知道帧率；事件顺序以 `drain_events()` 的顺序为准（host 侧保证配对：`Install` 之后必有
//! `InstallDone`、`LaunchStarted` 之后必有 `LaunchEnd`）。
//!
//! # 5. 留在 `crates/app`（Slint 侧）的东西，以及为什么
//!
//! | 留在 app | 为什么不能进 presenter |
//! | -------- | ---------------------- |
//! | `conv/` | 把 DTO 转成 **Slint 结构体**（UI 侧生成的视图模型类型），是 Slint 专属转换层 |
//! | `ui_bind.rs` / `page_sync.rs` / `callbacks*.rs` | 持有 `MainWindow`、`ModelRc`、回调闭包，全是框架绑定 |
//! | `theme_runtime.rs` / `appearance_bind.rs` | `impl Inner` 的方法，读写窗口属性与图片属性；其中的**纯判据**已抽到 [`theme`] |
//! | `bg_image.rs` / `png.rs` | 解码/模糊/编码产出框架图片类型与像素缓冲，属渲染侧 |
//! | `screenshot.rs` | Slint 快照通道（`Window::take_snapshot`） |
//! | `main.rs` | `include_modules!()` 与窗口拉起 |
//!
//! # 6. 零 UI 依赖的证据命令
//!
//! 两条命令（`grep` 与 `cargo tree`）的原文与实测输出见 `crates/app/Cargo.toml` 的注释与 M85
//! 的完成报告：本 crate 的依赖树里只有 serde / serde_json / unlucky-core（及其自身依赖）。

// 关于 dead_code：host 里有一批「已定义、由单测覆盖、等 iced 页面接线才调用」的辅助函数
// （`host::logs::session_finished`、`host::settings::clear_vendor_priority`
// 等）。它们原本住在 `crates/app` 里，由那边的 crate 级 `#![allow(dead_code)]` 一并放开；
// 搬进 lib 之后「pub 但模块私有」会被判成死码。逐条 `#[allow]` 会把噪音藏进实现细节，
// 这里沿用同一条策略统一放开，接线完成后再删掉本行暴露真死码。
#![allow(dead_code)]

pub mod agent;
pub mod backend;
pub mod dialogs;
pub mod dto;
pub mod host;
pub mod mock;
pub mod mock_data;
pub mod prefs;
pub mod router;
pub mod settings_bridge;
mod settings_bridge_lang;
pub mod state;
pub mod theme;
pub mod toast;
pub mod wizard;
