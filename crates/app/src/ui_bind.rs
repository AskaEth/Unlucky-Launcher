//! 绑定层（核心）：窗口/状态/后端的持有与同步；回调接线在 [`crate::callbacks`]。
//!
//! P0 的“后端”是 [`MockBackend`]（脚本化假数据）；把 `Box<dyn Backend>` 换成真实的 core/agent
//! host 时，除本文件、`conv.rs`、`callbacks.rs` 外都不需要改
//! （通道语义见 `DevDocs/SubAgentsTalk/app-DTO通道约定.md`）。
//!
//! 主题与外观（D17 / 09 §101-113）：落盘配置由 [`SettingsBridge`] 读写（`config/launcher.json`，
//! 与 `ull config set` 同一份），tick 里 1s 一次 `mtime` 热切；系统主题由
//! `core::platform::theme_probe` 做启动对齐 + 后端不支持时的兜底轮询。

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::backend::{build_backend, config_path, AppOptions};
use crate::bg_image::BackgroundLayers;
use crate::conv;
use crate::dto::{AppCommand, Backend};
use crate::prefs::BgFill;
use crate::settings_bridge::SettingsBridge;
use crate::state::AppState;
use crate::{MainWindow, PageState, SettingSection};

/// 定时器节拍：33ms ≈ 30fps（磁贴进度环 / 流式文本 / Toast TTL 的刷新下限）。
const TICK_MS: u64 = 33;

/// 配置热切轮询间隔（拍数）：约 1s 一次 `mtime` 比对，AI/CLI 改完下一拍即生效。
pub(crate) const CONFIG_POLL_TICKS: u64 = 30;

/// 系统主题兜底轮询间隔（拍数）：只在 `auto` 且后端不认识系统主题时启动（约 3s）。
pub(crate) const SYSTEM_THEME_POLL_TICKS: u64 = 90;

/// tile 模式重建画布的窗口尺寸粒度（像素）：避免拖动窗口时反复重算平铺画布。
pub(crate) const TILE_REGENERATE_STEP: u32 = 128;

/// 应用外壳：持有窗口、状态与后端，把三者缝在一起。
pub struct AppUi {
    inner: Rc<Inner>,
    timer: slint::Timer,
}

pub(crate) struct Inner {
    pub(crate) ui: MainWindow,
    pub(crate) state: RefCell<AppState>,
    pub(crate) backend: RefCell<Box<dyn Backend>>,
    /// `config/launcher.json` 读写桥（app 不自己写盘：一律走 core::setting 的 schema API）
    pub(crate) bridge: RefCell<SettingsBridge>,
    /// 当前背景图 / 模糊层（避免每次同步都重新解码）
    pub(crate) background: RefCell<BackgroundLayers>,
    /// 生成 `background` 时的输入（路径 / 填充模式 / 窗口粒度尺寸）
    pub(crate) background_key: RefCell<Option<BackgroundKey>>,
    /// 拍数（轮询节拍）
    pub(crate) ticks: Cell<u64>,
    /// 是否已经做过启动时的系统主题对齐
    pub(crate) system_theme_probed: Cell<bool>,
}

/// 背景图加工输入（值相同就不重新解码）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackgroundKey {
    pub(crate) path: String,
    pub(crate) fill: BgFill,
    /// tile 用的窗口尺寸（按 [`TILE_REGENERATE_STEP`] 量化；cover/contain 恒为 (0, 0)）
    pub(crate) viewport: (u32, u32),
}

impl AppUi {
    /// 建状态 + 真实后端、读配置、接全部回调、灌初始事件并做第一次同步。
    pub fn new(ui: MainWindow) -> Self {
        Self::with_options(ui, AppOptions::default())
    }

    /// 带启动选项装配（数据根覆盖 / 强制 mock）。
    pub fn with_options(ui: MainWindow, options: AppOptions) -> Self {
        // 先读一次落盘配置：主题三态 + 外观四件必须在首帧前就位（否则会闪一下出厂外观）
        let bridge = match &options.data_root {
            Some(root) => SettingsBridge::with_path(config_path(root)),
            None => SettingsBridge::open(),
        };
        let (appearance, theme) = bridge.load();
        let mut state = AppState::new();
        state.prefs.appearance = appearance;
        state.prefs.theme = theme;

        let backend = build_backend(&options);
        let inner = Rc::new(Inner {
            ui,
            state: RefCell::new(state),
            backend: RefCell::new(backend),
            bridge: RefCell::new(bridge),
            background: RefCell::new(BackgroundLayers::default()),
            background_key: RefCell::new(None),
            ticks: Cell::new(0),
            system_theme_probed: Cell::new(false),
        });

        crate::callbacks::wire(Rc::downgrade(&inner));
        crate::callbacks_pages::wire_pages(Rc::downgrade(&inner));
        inner.drain_events();
        inner.align_system_theme();
        inner.apply_theme();
        inner.apply_appearance();
        inner.sync();

        Self {
            inner,
            timer: slint::Timer::default(),
        }
    }

    /// 进入 Slint 事件循环（窗口句柄由本结构持有，直到循环退出）。
    pub fn run(&self) -> Result<(), slint::PlatformError> {
        self.inner.ui.run()
    }

    /// 启动 33ms 节拍（后端 tick → 事件 → 状态 → UI）。
    pub fn start_timer(&self) {
        let inner = Rc::clone(&self.inner);
        self.timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(TICK_MS),
            move || inner.tick(),
        );
    }

    /// 推进一拍（截图通道用：没有事件循环时手动把 tick 走起来，让 mock 事件灌进状态）。
    pub fn tick_once(&self) {
        self.inner.tick();
        slint::platform::update_timers_and_animations();
    }

    /// 直接跳到「设置 → 外观」页（截图验收用：证明外观分组的四组控件真的接线了）。
    pub fn open_settings_appearance(&self) {
        self.inner
            .state
            .borrow_mut()
            .navigate(crate::router::Route::Settings);
        self.inner.sync();
        self.inner
            .ui
            .set_settings_section(SettingSection::Appearance);
        slint::platform::update_timers_and_animations();
    }

    /// 直接跳到一个页面（截图验收用：每页一张）。
    pub fn open_page(&self, page: &str) {
        let route = match page {
            "download" | "java" => crate::router::Route::Download,
            "logs" => crate::router::Route::Logs,
            "settings" | "account" | "appearance" => crate::router::Route::Settings,
            "agent" => crate::router::Route::Agent,
            _ => crate::router::Route::Instances,
        };
        {
            let mut state = self.inner.state.borrow_mut();
            state.navigate(route);
            match page {
                "account" => state.settings_tab = "account".to_string(),
                "java" => state.settings_tab = "java".to_string(),
                "settings" => state.settings_tab = "download".to_string(),
                _ => {}
            }
            let download_tab = i32::from(page == "java");
            self.inner
                .ui
                .global::<PageState<'_>>()
                .set_download_tab(download_tab);
        }
        match page {
            "account" => self.inner.ui.set_settings_section(SettingSection::Account),
            "java" => self.inner.ui.set_settings_section(SettingSection::Java),
            "appearance" => self
                .inner
                .ui
                .set_settings_section(SettingSection::Appearance),
            "settings" => self.inner.ui.set_settings_section(SettingSection::Download),
            _ => {}
        }
        self.inner.sync();
        slint::platform::update_timers_and_animations();
    }

    /// 打开新建实例向导（截图验收用）。
    pub fn open_wizard(&self) {
        self.inner.state.borrow_mut().wizard = crate::wizard::WizardState::open();
        self.inner.sync();
        slint::platform::update_timers_and_animations();
    }

    /// 打开一个日志会话（截图验收：日志行 + 诊断卡片 + 头部标题都要有值）。
    pub fn open_session(&self, id: &str) {
        self.inner.state.borrow_mut().selected_session = Some(id.to_string());
        self.inner
            .dispatch(AppCommand::OpenLogSession(id.to_string()));
        self.inner.sync();
        slint::platform::update_timers_and_animations();
    }

    /// 派发一条指令并跑若干毫秒（截图中间态：安装进度 / 启动事件流）。
    pub fn dispatch_and_run(&self, command: AppCommand, millis: u64) {
        self.inner.dispatch(command);
        let steps = (millis / TICK_MS).max(1);
        for _ in 0..steps {
            self.inner.tick();
            slint::platform::update_timers_and_animations();
            std::thread::sleep(Duration::from_millis(TICK_MS));
        }
    }

    /// 渲染当前场景并存成 PNG（`--screenshot` 通道；见 `crate::png`）。
    ///
    /// 为什么要这条通道：验收环境可能没有交互桌面（`CopyFromScreen` 直接失败），
    /// 而 Slint 的软件渲染器可以把场景渲染进内存像素缓冲（`Window::take_snapshot`），
    /// 结果与屏幕上看到的逐像素一致，且不需要 GPU。
    ///
    /// # Errors
    /// 窗口显示失败、快照失败或写文件失败时返回错误。
    pub fn render_screenshot(&self, path: &str) -> Result<(), slint::PlatformError> {
        let ui = &self.inner.ui;
        ui.show()?;
        // 固定逻辑尺寸 = main.slint 的默认窗口口径，保证验收图可比
        ui.window()
            .set_size(slint::WindowSize::Logical(slint::LogicalSize::new(
                1180.0, 760.0,
            )));
        // 先走几拍：把后端事件（实例/版本/日志/账户/设置）与 33ms 节拍的效果灌进去
        for _ in 0..8 {
            self.tick_once();
            std::thread::sleep(Duration::from_millis(TICK_MS));
        }
        let buffer = ui.window().take_snapshot()?;
        let bytes = crate::png::encode_rgba8(&buffer);
        std::fs::write(path, bytes)
            .map_err(|error| slint::PlatformError::Other(format!("写截图失败 {path}: {error}")))?;
        Ok(())
    }
}

/// 发送一条 AI 提问：**只发指令 + 回流事件**，绝不做本地乐观回显。
///
/// 契约 §3 规定 host 先发 `MessageAppended(user)`：本地再 push 一次用户消息就会重复；
/// 且流式气泡的收敛依赖事件里的 `MessageFinished{index}`，本地多建的占位会变成
/// 永不收敛的「幽灵气泡」（review P1-1）。callbacks 与单测都走这一个入口。
pub(crate) fn send_agent_message(state: &mut AppState, backend: &mut dyn Backend, text: &str) {
    backend.dispatch(AppCommand::AskAgent(text.to_string()));
    for event in backend.drain_events() {
        state.apply(event);
    }
}

impl Inner {
    /// 走一次完整的「提问 → 事件回流」。
    pub(crate) fn ask_agent(&self, text: &str) {
        let mut state = self.state.borrow_mut();
        let mut backend = self.backend.borrow_mut();
        send_agent_message(&mut state, backend.as_mut(), text);
    }

    pub(crate) fn dispatch(&self, command: AppCommand) {
        self.backend.borrow_mut().dispatch(command);
        self.drain_events();
    }

    pub(crate) fn drain_events(&self) {
        let events = self.backend.borrow_mut().drain_events();
        if events.is_empty() {
            return;
        }
        let mut state = self.state.borrow_mut();
        for event in events {
            state.apply(event);
        }
    }

    fn tick(&self) {
        self.backend
            .borrow_mut()
            .tick(Duration::from_millis(TICK_MS));
        self.drain_events();
        self.state.borrow_mut().toasts.expire(TICK_MS);
        self.ticks.set(self.ticks.get() + 1);
        self.poll_settings();
        self.poll_system_theme();
        self.sync();
    }

    pub(crate) fn sync(&self) {
        let state = self.state.borrow();
        let ui = &self.ui;

        ui.set_route(conv::route(state.route()));
        ui.set_nav_collapsed(state.prefs.nav_collapsed);
        ui.set_ai_collapsed(state.prefs.ai_collapsed);
        ui.set_tile_size(conv::tile_size(state.prefs.tile_size));
        ui.set_instance_view(conv::instance_view(state.prefs.instance_view));
        ui.set_download_category(conv::category_from(state.prefs.download_category));
        ui.set_download_installed_only(state.installed_only);
        ui.set_theme_mode(conv::theme_mode(state.prefs.theme));

        ui.set_tiles(model(conv::tiles(&state)));
        ui.set_running_tiles(model(conv::running_tiles(&state)));
        ui.set_versions(model(conv::versions(&state)));
        ui.set_log_sessions(model(conv::log_sessions(&state)));
        ui.set_log_lines(model(conv::log_lines(&state)));
        ui.set_toasts(model(conv::toasts(&state)));

        let (diagnosis, has_diagnosis) = conv::diagnosis(&state);
        ui.set_diagnosis(diagnosis);
        ui.set_has_diagnosis(has_diagnosis);
        ui.set_selected_session(state.selected_session.clone().unwrap_or_default().into());

        ui.set_ai_messages(model(conv::chat(&state)));
        ui.set_ai_streaming(state.ai.streaming);
        ui.set_yolo(state.ai.yolo);
        ui.set_ai_endpoint_label(state.ai.endpoint_label.clone().into());

        let (dialog, open, queued) = conv::dialog(&state);
        ui.set_dialog(dialog);
        ui.set_dialog_open(open);
        ui.set_dialog_queued(queued);

        // PageState（账户/Java/安装/设置/通知/向导）在独立模块里灌，守 400 行
        crate::page_sync::apply(ui, &state);
    }
}

/// Slint 列表模型（VecModel → ModelRc）。
pub(crate) fn model<T: Clone + 'static>(items: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(items))
}
