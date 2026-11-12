//! `update`：消息分支的**唯一**落点（含「用户动作 → 后端指令」的派发与事件回流）。
//!
//! ## 数据流的三个入口（presenter crate 文档 §2）
//!
//! ```text
//! 用户动作 ──dispatch(AppCommand)──► backend ──► drain_events() ──► AppState::apply ──► 视图模型
//!      ▲                                                    ▲
//!      └── pages::Message（导航/启动/停止）──────────────────┘  忙碌 33 ms / 空闲 1 Hz
//! ```
//!
//! 三条纪律：
//!
//! 1. **不做乐观回显**：点了「Play」不改本地状态，等 `InstanceRuntimeChanged` 事件回来才变
//!    （否则事件与本地状态打架时，磁贴与进度条会各说一套）；
//! 2. **状态只有一个写入口**：`AppState::apply`（事件）与 `AppState::navigate`（路由）；
//! 3. **只在状态变了才重建视图模型**：`refresh_view()` 是快照 → 页面的唯一转换点，
//!    由 `view_dirty` 门控（M90 r2/P2-1 起；此前是每拍无条件重建）。
//!
//! ## 两个时钟（M90 r3 / P2-2）
//!
//! | 场景 | 时钟 | 周期 | 谁触发 |
//! |---|---|---|---|
//! | 截图 / **有动画在跑（M192）** / 有 Busy-Running 实例 / 有 toast / 派发后 3 s 观察窗 | `window::frames()` + 闸门 | 33 ms | [`App::needs_frames`] |
//! | 其余（空闲） | `Heartbeat` 自续链 | 1 s | [`App::heartbeat`] |
//!
//! 两条时钟不会同时干活：心跳在 `needs_frames()` 为真时**只续排、不推进**，忙碌结束自动接回。
//! 动效那条**不新开循环**：它只是给同一个布尔加一个分支（有轨道才为真），轨道到期当场清空，
//! 下一拍就退回心跳（见 `components::motion`）。

use crate::app::components::motion;
use crate::app::diag::diag;
use std::time::Instant;

use iced::{window, Task};

use unlucky_design::color::SchemeFlavor;
use unlucky_presenter::dto::AppCommand;
use unlucky_presenter::prefs::ThemeMode;
use unlucky_presenter::router::Route;

use crate::pages;
use crate::renderer::Kind;

use super::message::{route_of, Message};
use super::state::{App, IDLE_TICK, TICK, WATCH_WINDOW};
use super::CAPTURE_AT_FRAME;

mod account;
// M223：导入已有版本向导（扫描 / 预检 / 确认 / 关闭 / 退回）
mod adopt;
mod agent;
mod appearance;
pub(super) mod boot_env;
mod clock;
mod content;
mod content_pack;
mod dialog;
mod dialog_fixture;
mod download;
mod instances;
mod motion_sync;
mod page_messages;
mod settings;
mod update_action;

/// 这一条键盘事件是不是 `Esc` 按下。
fn is_escape(event: &iced::keyboard::Event) -> bool {
    matches!(
        event,
        iced::keyboard::Event::KeyPressed {
            key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            ..
        }
    )
}

impl App {
    /// 一条消息进来，改状态并返回后续任务。
    ///
    /// **分两段**：先派发（[`App::route_message`]），再同步一次动效的内容标识。第二段必须在
    /// `update` 里而不是 `view` 里：iced 求值订阅表**早于**建视图（`app/update/motion.rs` 的模块
    /// 文档有源码行号），在 `view` 里起跑的轨道当拍订阅不上，动画会退化成「一帧偏移 + 闪跳」。
    pub(super) fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.route_message(message);
        self.sync_motion();
        task
    }

    /// 把一条消息派发到它的落点（`update` 的正文；拆出来只为让 [`App::sync_motion`] 收尾）。
    fn route_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Boot => self.on_boot(),
            Message::WindowReady(id) => {
                self.window = id;
                // M93：整窗 α 要 Win32 句柄，而 iced 只给 `window::Id`。`window::run` 是唯一能把
                // `&dyn Window`（实现了 `HasWindowHandle`）拿出来的入口。
                match id {
                    Some(id) => {
                        window::run(id, super::window_alpha::probe_hwnd).map(Message::WindowHandle)
                    }
                    None => Task::none(),
                }
            }
            Message::WindowHandle(hwnd) => {
                self.hwnd = hwnd;
                match hwnd {
                    Some(_) => self.apply_window_alpha("boot"),
                    None => {
                        diag!("[appearance] hwnd=none（拿不到窗口句柄：整窗 α 无法下发）");
                        self.log_appearance("boot", None);
                    }
                }
                // M116：圆角放在 α **之后**——α < 1 时那一步才补上 `WS_EX_LAYERED`，
                // 先打圆角日志会把 `layered=` 记成 `off`（那正是我们要分辨的那一位）。
                self.apply_window_corner("boot");
                Task::none()
            }
            Message::SystemTheme(mode) => {
                let flavor = match mode {
                    iced::theme::Mode::Dark => SchemeFlavor::Dark,
                    iced::theme::Mode::None | iced::theme::Mode::Light => SchemeFlavor::Light,
                };
                diag!("[theme] 系统主题 = {mode:?}");
                // 优先级：`--theme` > 配置文件里的 `appearance.theme_mode` > 系统。
                // 早先这里无条件盖掉，于是「配置文件写 dark、系统是浅色」会出现
                // `light -> dark -> light` 的一次抖动，配置永远不生效（M90 r3 顺手修）。
                if self.options.theme.is_none() && self.theme_mode == ThemeMode::Auto {
                    self.set_flavor(flavor);
                }
                Task::none()
            }
            Message::Frame => self.on_frame(),
            Message::Heartbeat => self.on_heartbeat(),
            Message::Window(event) => {
                match event {
                    window::Event::Resized(size) => {
                        self.width = size.width;
                        // 高度是背景平铺网格的视口判据之一（宽度早就有），以前只记宽是因为没有背景层。
                        self.height = size.height;
                        self.view_dirty = true;
                        self.refresh_view();
                    }
                    // M113: import/export —— 拖入整合包（本版唯一的选文件通道；窗口级事件，
                    // 所以入口收起时也照样接得住。细节在 `on_pack_dropped` / `on_pack_hover`）
                    window::Event::FileDropped(path) => return self.on_pack_dropped(path),
                    window::Event::FileHovered(_) => return self.on_pack_hover(true),
                    window::Event::FilesHoveredLeft => return self.on_pack_hover(false),
                    _ => {}
                }
                Task::none()
            }
            Message::Page(message) => self.on_page(message),
            Message::Dialog(message) => dialog::on_dialog(self, message),
            // M223：导入已有版本向导（打开 / 选版本 / 选目标 / 预检 / 确认 / 退回 / 关闭）
            Message::Adopt(message) => self.on_adopt(message),
            Message::Agent(message) => {
                // `Esc` 先给弹窗：有弹窗时它属于「关掉这一层」，不属于 AI 栏的收起。
                if let crate::shell::agent::Message::Key(event) = &message {
                    if is_escape(event) && dialog::escape(self) {
                        return Task::none();
                    }
                }
                self.on_agent(message)
            }
            // 外壳导航轨（`src/shell/nav.rs`）发来的点选：翻成同一次路由跳转
            Message::Select(nav) => {
                self.state.navigate(route_of(nav));
                self.view_dirty = true;
                self.refresh_view();
                Task::none()
            }
            Message::DragWindow => match self.window {
                Some(id) => window::drag(id),
                None => Task::none(),
            },
            Message::ResizeStart(direction) => match self.window {
                Some(id) => window::drag_resize(id, direction),
                None => Task::none(),
            },
            Message::Minimize => match self.window {
                Some(id) => window::minimize(id, true),
                None => Task::none(),
            },
            Message::ToggleMaximize => match self.window {
                Some(id) => window::toggle_maximize(id),
                None => Task::none(),
            },
            Message::Close => iced::exit(),
            Message::Captured(screenshot) => self.on_captured(&screenshot),
            Message::Demo(message) => {
                // gallery 只是 harness：按下只打日志，不接业务（组件库要能脱离业务验收）
                diag!("[gallery] {} pressed", message.id());
                Task::none()
            }
        }
    }

    /// 启动：问窗口句柄 → 让后端发布首屏快照 → 决定主题。
    ///
    /// **首屏快照在 `Boot` 就抽干**（不是等第一拍）：截图通道在第 3 帧落盘，33 ms 的节拍
    /// 未必来得及到达，等节拍会把空页面拍进图里。
    fn on_boot(&mut self) -> Task<Message> {
        let mut tasks = vec![window::latest().map(Message::WindowReady)];

        // M192：给动效帧泵定档（截图档关、CPU 档半速、GPU 档正常；`ULL_MOTION*` 可覆盖）。
        // 必须在第一次 `view()` 之前：页面主体的入场由 `pages::render` 采样，配置晚了首屏就没有动效。
        self.configure_motion();

        // M117：启动日志一行 —— `[auth] msa client_id=configured|missing source=env|build|none`。
        // 判据（配没配 + 从哪来）在 `core::auth::microsoft::client_id_status()`，零网络；
        // 本 crate 不直接依赖 core，所以经 `unlucky_cli`（已是本 crate 的依赖）出面，
        // 免得 UI 侧把判据重算一遍、两份口径漂。日后看日志能一眼分辨
        // 「没配置」与「配了但登录失败」。
        diag!("{}", unlucky_cli::commands::msa_client_id_log_line());

        // 主题优先级：显式 `--theme` / `--screenshot-theme` > 设置文件 > 系统。
        // 截图通道**不读设置文件**（D30 可复现：同一份代码在两台机器上要出同一张图）。
        self.apply_config_theme();

        // M93 外观取证开关（`ULL_APPEARANCE_*`）+ 启动那一行数值日志。
        // 必须在 `refresh_view` 之前：设置页要画的就是这份状态。
        self.appearance.apply_env();
        self.rebuild_ui();
        self.log_appearance("boot", None);
        // M187（r1 复审 P2-4）：启动这一拍的事实进启动器日志的缓冲（第一次启动时落盘）。
        boot_env::record_boot_log(self);

        // 交互模式且没显式指定主题 → 探系统；截图模式永不等（见 cli 模块文档）
        if self.options.wants_system_theme() {
            tasks.push(iced::system::theme().map(Message::SystemTheme));
        }

        // M104：助手栏的启动尾巴（窗口尺寸 adopt + 「拆出档直接落到独立页」）在 update/agent.rs。
        // **必须在 RefreshAll 之前**：那次 dispatch 会 `pump → auto_select`，把路由拉到实例详情，
        // 而 `sync_agent_route` 看到「不在 Agent 页却还是 Detached」就会把档位收回来——顺序反了
        // 就会出现「`ULL_AGENT_RAIL=detached` 启动后停在实例页」（r2 实测踩到）。
        self.boot_agent();

        // 「重新扫描实例目录 + 全部页面快照」：真 host 在这里发首批事件，mock 在构造时已预置
        self.dispatch(AppCommand::RefreshAll);

        // 取证开关 `ULL_START_ROUTE`：截图/冒烟直接落到某一页（`cli.rs` 不在本 mission 的 scope，
        // 而「外观」分组只在设置页上，出矩阵必须能直达）。未知值忽略。
        if let Some(route) = boot_env::env_start_route() {
            diag!("[boot] ULL_START_ROUTE → {route:?}");
            self.state.navigate(route);
            // 直达某一页时不要再被「自动选中第一条实例」拉回详情页。
            self.auto_selected = true;
            // M109：直达内容库时补一次检索（与页面内切换同一个入口）
            self.ensure_content_loaded();
        }
        account::force_account_state(self);
        // M187：启动失败弹窗的取证夹具（`ULL_DIALOG_FIXTURE`），必须在 `refresh_view` 之前。
        if let Some(mode) = dialog_fixture::apply(self) {
            diag!("[boot] ULL_DIALOG_FIXTURE → {mode}");
        }

        self.auto_select();
        self.view_dirty = true;
        self.refresh_view();

        // 空闲心跳从这里开始自续（它是唯一能在「没有帧事件」时戳醒 app 的时钟）
        tasks.push(heartbeat());

        Task::batch(tasks)
    }

    /// M192：给帧泵定档（策略全在 [`boot_env::motion_scale`]，这里只搬输入）。
    fn configure_motion(&mut self) {
        let pin = boot_env::env_motion_pin();
        let gpu = matches!(
            super::backend::effective_kind(),
            Some(crate::renderer::Kind::Wgpu)
        );
        let scale = boot_env::motion_scale(self.options.screenshot.is_some(), gpu, pin.is_some());
        motion::configure(scale, pin);
        // 打**帧泵实际拿到的**档位（不是刚算出的那个）：配置没生效时这一行会露馅。
        diag!(
            "[motion] scale={:.2} pin={:?} gpu={} screenshot={}",
            motion::scale().factor(),
            pin,
            gpu,
            self.options.screenshot.is_some()
        );
    }

    /// 空闲心跳：只有「不需要帧节拍」时才推进一拍，然后**无条件续排**下一拍。
    ///
    /// 为什么无条件续排：忙碌结束（游戏退出、toast 过期、观察窗关闭）之后没有别的东西会叫醒
    /// app，心跳链必须一直在，才能把低频时钟接回去。忙碌期间它只是每秒醒一次、什么都不做。
    fn on_heartbeat(&mut self) -> Task<Message> {
        // 动效时间轴也吃这一拍：窗口被遮挡 / 最小化 / 合成器不送绘制时，帧一个都不来，而
        // 「有轨道」又让下面的 1 Hz 推进整段停摆——那会让 `needs_frames()` 永远为真。心跳把
        // 轨道按单次上限（[`components::motion::pump::MAX_STEP_MS`]）推几步收尾，最坏的「没有帧」
        // 情况下 5 拍内自然退订；有帧时它只多推 64 ms/秒，看不出来。
        motion::advance();
        if !self.needs_frames() {
            let now = Instant::now();
            let elapsed = now.saturating_duration_since(self.last_tick).max(IDLE_TICK);
            self.last_tick = now;
            self.advance(elapsed);
        }
        // M192 真机取证（`ULL_MOTION_FIXTURE_AT`）：到点就**真的**导航一次。放在 advance 之后 ⇒
        // 这一拍是货真价实的空闲态；跳转后由 `update` 收尾的 `sync_motion` 起跑（与点击同一条路）。
        self.motion_fixture_after_idle();
        heartbeat()
    }

    /// 一帧画完：①动效时间轴推进 ②够 33 ms 就推进一拍 presenter ③截图模式在第
    /// [`CAPTURE_AT_FRAME`] 帧出图。
    ///
    /// 时间闸门按**真实流逝时间**判（不是数帧）：掉帧不会把游戏启动进度拖慢，
    /// 也不会在慢机器上把 1.5 s 的下载阶段拉成 4 s。
    fn on_frame(&mut self) -> Task<Message> {
        self.frames += 1;

        // M192：动效时间轴。**每帧都调**，帧率上限在帧泵内部（16 ms 一次结算），所以忙碌态的
        // 近千 fps 不会变成近千次重排；动画结束后帧泵自动清空，下一拍订阅表就不再有 `frames()`。
        motion::advance();
        // 出图取证（`ULL_MOTION_FIXTURE`）：前两帧各做一次**真的**状态变化。
        // 没有这步，截图通道每一帧画的都是同一屏，页面切换 / 列表↔详情的入场就没有可拍的相位。
        // `ULL_MOTION_FIXTURE_AT`（真机空闲取证）在的时候**不跑**这一支：两条取证路互斥，
        // 否则第 1 帧就把页面换成目标页，等 `_AT` 的心跳再跳同一个目标 = 标识没变 = 没有动效。
        if self.frames <= 2 && boot_env::env_motion_fixture_at().is_none() {
            self.motion_fixture();
        }

        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last_tick);
        if elapsed >= TICK {
            self.last_tick = now;
            self.advance(elapsed);
        }

        // 主题在启动时就已确定（截图模式必有主题，见 cli 模块文档），这里只剩帧数闸门
        if !self.wants_frames() || self.frames < CAPTURE_AT_FRAME {
            return Task::none();
        }

        match self.window {
            Some(id) => window::screenshot(id).map(Message::Captured),
            None => Task::none(),
        }
    }

    /// 截图回来了：落盘 + 打一行取证日志，然后退出。
    fn on_captured(&mut self, screenshot: &window::Screenshot) -> Task<Message> {
        let Some(path) = self.options.screenshot.clone() else {
            return Task::none();
        };
        match crate::shot::save(screenshot, &path) {
            Ok(summary) => {
                diag!("{}", summary.describe());
                // M97：打**实际**后端名（`wgpu`/`tiny-skia`），不是请求值——`renderer=auto` 曾经出现在每张验收图的日志里。
                diag!(
                    "[shot] screen={} theme={} renderer={} route={:?}",
                    self.options.screen.name(),
                    crate::shot::flavor_slug(&self.ui),
                    super::backend::effective_kind().map_or("?", Kind::name),
                    self.state.route()
                );
            }
            Err(error) => diag!("[shot] 失败：{error}"),
        }

        iced::exit()
    }
}

/// 空闲心跳的一拍：睡 1 秒再发 [`Message::Heartbeat`]。
///
/// 用 `Task::perform` + `std::thread::sleep` 而不是 `iced::time::every`：后者只在 tokio/smol
/// 后端提供（本 crate 的执行器是 thread-pool，Cargo.toml 不在 scope）。代价是每拍让线程池里
/// 一个线程阻塞 1 秒——按 1 Hz 算，对线程池（默认 = CPU 数）是可接受的占用。
fn heartbeat() -> Task<Message> {
    Task::perform(async { std::thread::sleep(IDLE_TICK) }, |()| {
        Message::Heartbeat
    })
}

#[cfg(test)]
mod download_wiring;
