//! `App` 的两个构造器（从 `state.rs` 搬出来守 400 行硬线；M119 拆，纯搬不改）。
//!
//! | 构造器 | 谁用 | 碰不碰磁盘 |
//! |---|---|---|
//! | [`App::new`](super::App::new) | 生产（`main` → `boot`） | 按启动参数选后端 + 打开设置桥 |
//! | [`App::with_presenter`](super::App::with_presenter) | 单测（后端与桥都由调用方给） | **不碰** |
//!
//! 两个都走 `with_presenter`：生产路径只是先造好后端与桥。**字段初始化表**也在这一个文件里
//! （构造 = 字段表 + 三件外部依赖），加字段时只改这一处。

use iced::window;

use unlucky_design::color::SchemeFlavor;
use unlucky_presenter::backend::{build_backend, config_path};
use unlucky_presenter::dto::Backend;
use unlucky_presenter::settings_bridge::SettingsBridge;
use unlucky_presenter::state::AppState;

use crate::cli::Options;
use crate::pages;
use crate::theme::{Families, Ui};
use crate::ui::lang::LangState;

use super::super::WINDOW_SIZE;
use super::boot_probe::app_options;
use super::App;

impl App {
    /// 生产构造：按启动参数选后端、打开设置桥。
    ///
    /// **截图通道强制 mock**：出图要可复现（D30），真 host 会按本机盘上的实例清单画面，
    /// 同一份代码在两台机器上出不了同一张图。交互模式走真 host（数据根解析不出来时
    /// [`build_backend`] 自己退回 mock）。
    pub(in crate::app) fn new(options: Options, families: Families, flavor: SchemeFlavor) -> Self {
        let backend = build_backend(&app_options(&options));
        // P2-2：设置桥必须与后端指向**同一个数据根**。早先这里无条件 `SettingsBridge::open()`
        // （按便携配置解析），于是 `--data-dir X` 会出现「实例来自 X、主题/设置读写默认根」的一进程
        // 两个数据根。Slint 版是同一条口径：`app/src/ui_bind.rs` 的
        // `Some(root) => SettingsBridge::with_path(config_path(root))`。
        let bridge = match &options.data_dir {
            Some(root) => SettingsBridge::with_path(config_path(root)),
            None => SettingsBridge::open(),
        };
        Self::with_presenter(options, families, flavor, backend, bridge)
    }

    /// 注入式构造（单测用：后端与桥都由调用方给，**不碰磁盘**）。
    pub(in crate::app) fn with_presenter(
        options: Options,
        families: Families,
        flavor: SchemeFlavor,
        backend: Box<dyn Backend>,
        bridge: SettingsBridge,
    ) -> Self {
        let width = WINDOW_SIZE.width;
        let height = WINDOW_SIZE.height;
        let theme_mode = bridge.load().1;
        // 外观两半分开读：`load()` 给 8 个 UI 写入字段，`load_base()` 给 4 个只读下层键
        // （整窗 α / 标题栏三态 / 配色来源 / 窗口圆角）。M93 之前这些键在 app 侧**根本读不到**。
        //
        // 截图通道**不读用户配置**（D30：同一份代码在两台机器上要出同一张图）：外观一律取出厂值，
        // 矩阵需要的值由 `ULL_APPEARANCE_*` 显式灌进来。
        let (appearance_prefs, appearance_base) = if options.screenshot.is_some() {
            (
                unlucky_presenter::prefs::AppearancePrefs::factory(),
                unlucky_presenter::prefs::AppearanceBase::factory(),
            )
        } else {
            (bridge.load().0, bridge.load_base())
        };
        // 面板 α / 玻璃的降级判据要的是**实际**后端，不是 `--renderer` 的请求值（复审 r1 P2-2）：
        // 默认 `Auto` + 无独显时请求是 Auto、实际是 tiny-skia，只看请求值就会漏掉 D36 的强制项。
        let backend_tier = super::super::backend::effective_backend(options.backend);
        let mut appearance = pages::appearance::State::new(
            &appearance_prefs,
            &appearance_base,
            backend_tier,
            flavor,
        );
        // 取证开关（D30）：`ULL_SETTINGS_GROUP=<下标>` 直达设置页的某一组（出图要看见语言行）。
        if let Some(index) =
            crate::ui::lang::screenshot_settings_group(options.screenshot.is_some())
        {
            appearance.selected_group = index;
        }
        // 界面语言（M106）：**M92 已经探测对了，问题是一直没人用**——`pages::Model.lang` 此前在
        // `view::model` 里被写死成 `Lang::En`，于是中文系统上开出来的是全英文界面。
        let lang = LangState::boot(&bridge, options.screenshot.is_some());
        let mut view = pages::Model::new(
            pages::Screen::Instances,
            width,
            height,
            Some(pages::Screen::Instances.nav()),
        );
        view.lang = lang.lang();

        let mut app = Self {
            ui: Ui::new(flavor, families),
            families,
            options,
            backend,
            bridge,
            state: AppState::new(),
            ticks: 0,
            config_due_at: unlucky_presenter::theme::CONFIG_POLL_TICKS,
            theme_mode,
            lang,
            last_tick: std::time::Instant::now(),
            width,
            view,
            view_dirty: false,
            watch_until: None,
            window: None::<window::Id>,
            frames: 0,
            refreshes: 0,
            auto_selected: false,
            height,
            appearance,
            appearance_prefs,
            hwnd: None,
            settings: pages::settings::Groups::default(),
            // 出厂是**空会话**（M119 删了 M104 的样本会话）；取证开关 `ULL_AGENT_*` 的档位
            // 与夹具在 `boot_agent` 里落地（它要先让 presenter 知道栏可见性，见那里）。
            agent: crate::shell::agent::AgentUi::new(),
            agent_host: super::presenter::AgentHost::from_env(),
            // 下载页安装草稿（M105）：出厂态 = 没选中、原版、两个空串
            download_draft: pages::InstallDraft::default(),
            launch_form: pages::LaunchForm::default(),
            install_cancelled_seen: 0,
            launch_ends_seen: 0,
            content: pages::content::PageState::default(),
            account_form: pages::account::Form::new(),
            account: pages::account::Page::build(
                &AppState::new(),
                &pages::account::Form::new(),
                &mut pages::account::avatar::Cache::default(),
                unlucky_i18n::Lang::En,
                width,
            ),
            avatars: pages::account::avatar::Cache::default(),
            corner_applied: None,
        };
        // 配色来源档可能已经指向一张背景图：那时 `Ui` 必须由解析出的方案构造，而不是 baseline。
        app.rebuild_ui();
        // 两行日志：①档位 + 探测结论 + 生效语言 ②中文策略（D28 缺字体不许静默）。
        // 下一个人据此分得清「探测错了」还是「探测对了但没用上」。
        app.lang.log();
        app.lang.log_cjk();
        app
    }
}
