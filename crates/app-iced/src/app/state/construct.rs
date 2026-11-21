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
use unlucky_presenter::prefs::ThemeMode;
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
        // P2-2：设置桥必须与后端指向**同一个数据根**。早先这里无条件 `SettingsBridge::open()`
        // （按便携配置解析），于是 `--data-dir X` 会出现「实例来自 X、主题/设置读写默认根」的一进程
        // 两个数据根。Slint 版是同一条口径：`app/src/ui_bind.rs` 的
        // `Some(root) => SettingsBridge::with_path(config_path(root))`。
        let bridge = match &options.data_dir {
            Some(root) => SettingsBridge::with_path(config_path(root)),
            None => SettingsBridge::open(),
        };
        // M309：自更新的「升级尝试」落点跟着同一份设置桥走（`<home>/cache/update/`）。出图通道
        // 不参与（D30 要可复现，而且它不读用户数据根）。
        let rollback_config = match options.screenshot {
            Some(_) => None,
            None => bridge.path(),
        };
        super::super::update::rollback::arm(rollback_config);
        // M309：对账必须在 `build_backend` **之前**跑完——不是「早点更好」，而是判据可达性：
        // `trial.started_ms` 只在 `reconcile` 里落地，一个在「建后端 / 装配 App」里就 panic 的
        // 坏版本如果对账排在后面，每次启动都到不了对账 ⇒ `trial` 恒 `None` ⇒ 超窗口判定永远
        // 不成立 ⇒ 无限 crash loop 且一次自动回滚都不会发生。结论先攥着，App 建好后再说。
        let rollback_notice = super::super::update::rollback::reconcile_before_boot();
        let backend = build_backend(&app_options(&options));
        // M188：日志目录也在这一拍登记——它是从**同一份**设置桥反推的（`--data-dir` 同样生效），
        // 所以设置页显示的路径与 `launcher.log` 的真实落点不会各说各话。
        crate::app::logs_folder::register(&bridge);
        let mut app = Self::with_presenter(options, families, flavor, backend, bridge);
        // 对账的结论（回滚 / 停手 / 搬不动）在这里进 toast；没有标记时一句废话都不说。
        super::super::update::rollback::announce(&mut app, rollback_notice);
        app
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
        // 主题档：截图通道**不读机器配置**（D30：同一份代码在两台机器上要出同一张图）——
        // 由 `--screenshot-theme` 钉死（浅 → Silver / `light`，深 → Ink / `dark`）。
        // 产品路径才读 `appearance.theme_mode`。复审 P2：此前这里两种通道都读文件，
        // 设置页「主题」那一行的选中态会随出图机器的配置漂移。
        let theme_mode = match options.screenshot.is_some() {
            true => match options.shot_flavor().unwrap_or(SchemeFlavor::Light) {
                SchemeFlavor::Light => ThemeMode::Light,
                SchemeFlavor::Dark => ThemeMode::Dark,
            },
            false => bridge.load().1,
        };
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
        // M168（复审 r2 P2-2）：出图通道**不许联网**——`log_cjk` 那一拍会走 `after_probe`，
        // 一台 zh-locale 且没有中文字体的机器上会真的下 17 MiB，并可能在出图过程中注册新字体
        // （D30：同一份代码要出同一张图）。产品路径不设这个开关。
        crate::fonts::install::set_offline(options.screenshot.is_some());
        // M457：出图通道（D30）要能把实例页的搜索关键词钉死；产品路径出厂是空串。
        // 与 `ULL_INSTANCE_VIEW` / `ULL_INSTANCE_TILE` 同一族开关，落点在 `boot_env`。
        let instance_query = if options.screenshot.is_some() {
            crate::app::update::boot_env::env_instance_query()
        } else {
            String::new()
        };
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
            // M256：换页过渡期的「旧页」快照。启动时没有旧页。
            previous_view: None,
            view_dirty: false,
            watch_until: None,
            window: None::<window::Id>,
            frames: 0,
            refreshes: 0,
            auto_selected: false,
            // M168：中文字体下载状态。构造这一拍 `lang.log_cjk()` 与启动编排的 `fonts::resolve()`
            // 已经可能发起下载，所以初值就取**当前快照**（不能假定是 Unknown，否则第一拍会白重建）。
            font_seen: crate::fonts::install::status(),
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
            adopt_form: super::super::adopt::Form::default(),
            install_cancelled_seen: 0,
            launch_ends_seen: 0,
            mods_failed_seen: None,
            dialog_card: None,
            launch_blocks_seen: 0,
            java_guard: None,
            dialog_model: None,
            toast_tracks: super::toast::ToastTracks::default(),
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
            // M457：实例页搜索框。产品路径出厂是空关键词（不过滤）；出图通道要能钉一个词
            // （`ULL_INSTANCE_QUERY`），否则「有结果 / 空结果」那四张图没法复现（D30）。
            instance_query,
        };
        // M229：设置页「主题」行的选中态来自设置组自有状态（`language_appearance::State::theme`），
        // 唯一写者是 `App::set_theme_mode`；这里把启动时读到的档位灌进去，首帧才不会画错档。
        app.settings.language_appearance.theme = theme_mode;
        // M300：导航轨的折叠档（`ui.nav.collapsed`）。**截图通道不读用户配置**（D30：同一份代码
        // 在两台机器上要出同一张图）：出图一律展开态，折叠态那两张图由 `ULL_NAV_COLLAPSED` 显式灌。
        app.state.prefs.nav_collapsed = if app.options.screenshot.is_some() {
            crate::app::update::boot_env::env_nav_collapsed()
        } else {
            app.bridge.load_nav_collapsed()
        };
        // M452：另外四个 UI 档位键的启动读回（AI 栏折叠档 / 磁贴尺寸 / 实例页视图 / 下载分类）。
        // 截图通道同样不读用户配置（D30，与上面那条同一口径）。
        if app.options.screenshot.is_none() {
            app.adopt_ui_prefs();
        } else {
            // M455：出图通道的两个档位由环境变量钉住（`ULL_INSTANCE_VIEW` / `ULL_INSTANCE_TILE`），
            // 词表与落盘一致；没给就是出厂（`tiles` / `medium`）。这一句同时打一行身份日志——
            // 每张图配一行「这一屏是哪个组合」的证据就在它后面。
            app.state.prefs.instance_view = crate::app::update::boot_env::env_instance_view();
            app.state.prefs.tile_size = crate::app::update::boot_env::env_tile_size();
            // M457：这一行还带上搜索关键词——四张交付图（列表 / 磁贴 × 有结果 / 空结果）
            // 靠它证明「这张图是哪个组合」，不用去猜图里那个药丸写的是什么。
            crate::app::diag::diag!(
                "[shot] instances view={} tile={} columns={} query={:?}",
                app.state.prefs.instance_view.as_str(),
                app.state.prefs.tile_size.as_str(),
                app.state.prefs.tile_size.columns(),
                app.instance_query
            );
        }
        // 配色来源档可能已经指向一张背景图：那时 `Ui` 必须由解析出的方案构造，而不是色板。
        app.rebuild_ui();
        // M174：助手配置的首屏值（真值来自 `config/launcher.json`；截图通道走 `ULL_AI_STATE`）。
        app.hydrate_ai_settings();
        // M175：Java 分组的首屏值 + **首屏自动扫一次盘**（截图通道走 `ULL_JAVA_STATE`）。
        // M223: adopt wizard screenshot seed (evidence channel only; `ULL_ADOPT_STATE`).
        app.hydrate_adopt_state();
        app.hydrate_java_settings();
        // 两行日志：①档位 + 探测结论 + 生效语言 ②中文策略（D28 缺字体不许静默）。
        // 下一个人据此分得清「探测错了」还是「探测对了但没用上」。
        app.lang.log();
        app.lang.log_cjk();
        // M298：出图取证用的 Toast 夹具（`ULL_TOAST_FIXTURE`；产品路径读不到这个变量）。
        super::toast_shot::seed(&mut app);
        // M331：出图取证用的「Java 不符 · 拦下」夹具（`ULL_JAVA_GUARD_SHOT`；同上，只影响这一次进程）。
        super::super::dialog::shot::seed(&mut app);
        app
    }

    /// 把盘上的四个 UI 档位键灌进首屏（M452；只被 [`App::with_presenter`] 的产品路径调）。
    ///
    /// 三件事分开做，因为它们对「**缺键**」的处置不一样：
    ///
    /// | 落点 | 缺键时 | 为什么 |
    /// |---|---|---|
    /// | presenter 的 `UiPrefs` 四格 | 照抄 `UiKeys` 的出厂值 | 它的出厂值与 schema 逐项相同，没有歧义 |
    /// | AI 右栏档位 | **不动**（保持 `AgentUi::new` 的折叠） | schema 出厂 `false` = 展开，而首启版面要折叠 |
    /// | 下载页 chip | **不动**（保持 `Category::All`） | schema 出厂 `release` 是个会过滤的选中值，「没选过」不是它 |
    ///
    /// 所以后两格先问 [`unlucky_presenter::settings_bridge::SettingsBridge::ui_key_present`]：
    /// 「键真的出现过」才盖界面。缺键不报错、旧配置（只有 `appearance.*` 那种）也走同一条路。
    pub(in crate::app) fn adopt_ui_prefs(&mut self) {
        use unlucky_presenter::prefs::keys;

        let ui = self.bridge.load_ui_keys();
        self.state.prefs.adopt_ui_keys(ui);
        if self.bridge.ui_key_present(keys::AI_COLLAPSED) {
            self.agent.boot_rail(ui.ai_collapsed);
        }
        if self.bridge.ui_key_present(keys::DOWNLOAD_CATEGORY) {
            // `modpack` / `mod` 在侧栏没有 chip：那两档按「全部」画（不编第五个 chip）。
            if let Some(category) = pages::Category::from_version_category(ui.download_category) {
                self.view.download.category = category;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! **M452 的启动读回**（`adopt_ui_prefs`）：四个档位键里那些「界面上真的看得见」的值
    //! 在首帧就得对。AI 栏与下载分类各自的用例在 `app/update/agent/rail_persist.rs` 与
    //! `app/update/download_wiring.rs`（它们要在拨动之后再重启才说得清）；这里钉的是
    //! **磁贴尺寸 / 实例页视图**——这两格自 M455（`07ed9dc`）起页头有真控件（实例页的两组
    //! chip，见 `pages/instances/chips.rs`），所以「点一下会不会写盘」在
    //! `app/update/instances/view_mode.rs` 里另有用例；这里钉的是**另一半**：盘上写了值、
    //! 重启之后 `state.prefs` 里就是它们（`adopt_ui_prefs` 这一条链）。

    use unlucky_design::color::SchemeFlavor;
    use unlucky_presenter::mock::MockBackend;
    use unlucky_presenter::prefs::{InstanceView, TileSize, UiKeys};
    use unlucky_presenter::settings_bridge::SettingsBridge;

    use crate::cli::Options;
    use crate::theme::Families;

    use super::*;

    const FAMILIES: Families = Families {
        text: "Segoe UI",
        display: "Segoe UI",
        mono: "Consolas",
    };

    fn app_with(bridge: SettingsBridge) -> App {
        crate::app::components::motion::reset();
        App::with_presenter(
            Options::default(),
            FAMILIES,
            SchemeFlavor::Light,
            Box::new(MockBackend::new()),
            bridge,
        )
    }

    /// 一份临时 `launcher.json`（父目录建好、文件删掉 = 首次启动）。
    fn temp_config(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ull-construct-prefs-{label}-{}-{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(dir.join("config")).expect("临时目录");
        let path = dir.join("config/launcher.json");
        let _ = std::fs::remove_file(&path);
        path
    }

    /// 盘上写了磁贴尺寸 / 视图 ⇒ 启动后 presenter 的两个字段就是它们。
    ///
    /// 抓得住：`adopt_ui_prefs` 没调 `UiPrefs::adopt_ui_keys`（`state.prefs` 停在出厂
    /// `Medium` / `Tiles`）。接口那一头（页头 chip 的写盘）由 `view_mode.rs` 的往返用例守着，
    /// 这条守的是读回：**盘上的值真的进了内存**，且没顺手改掉邻格。
    #[test]
    fn a_restart_reads_the_tile_size_and_view_into_the_prefs() {
        let path = temp_config("shelf");
        let mut writer = SettingsBridge::with_path(&path);
        let wanted = UiKeys {
            tile_size: TileSize::Large,
            instance_view: InstanceView::List,
            ..UiKeys::default()
        };
        assert!(writer.persist_ui_keys(&wanted));

        let app = app_with(SettingsBridge::with_path(&path));
        assert_eq!(app.state.prefs.tile_size, TileSize::Large);
        assert_eq!(app.state.prefs.instance_view, InstanceView::List);

        // 另外两族字段没被顺手改掉（`adopt_ui_keys` 只碰四格）
        assert!(!app.state.prefs.nav_collapsed);
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    /// **没有配置文件也能启动**：四格全是出厂值，不 panic、不写盘。
    #[test]
    fn a_first_boot_without_any_config_starts_on_the_factory_values() {
        let path = temp_config("empty");
        let app = app_with(SettingsBridge::with_path(&path));
        assert_eq!(app.state.prefs.tile_size, TileSize::Medium);
        assert_eq!(app.state.prefs.instance_view, InstanceView::Tiles);
        assert_eq!(app.view.download.category, pages::Category::All);
        assert!(
            !SettingsBridge::with_path(&path)
                .ui_key_present(unlucky_presenter::prefs::keys::TILE_SIZE),
            "首次启动不该把出厂值写成一条显式配置"
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }
}
