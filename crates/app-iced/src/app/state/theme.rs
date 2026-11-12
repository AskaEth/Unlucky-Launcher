//! 主题与 `Ui` 的重算口径（从 `state.rs` 搬出来守 400 行硬线；M119 拆，纯搬不改）。
//!
//! 三件事在这里：①`Ui` 什么时候重建（[`App::rebuild_ui`]）②主题档怎么定
//! （[`App::set_flavor`] / [`App::apply_config_theme`]）③读当前求值器（[`App::ui`]）。
//!
//! 截图通道的判据 [`App::has_screenshot`] 也放在这儿：上面三条里两条都要「截图不读用户配置」。

use crate::app::diag::diag;
use crate::app::message::Message;
use iced::Task;
use unlucky_design::color::SchemeFlavor;
use unlucky_presenter::prefs::ThemeMode;

use crate::theme::Ui;

use super::App;

impl App {
    /// 本次进程是否走截图通道。
    pub(in crate::app) fn has_screenshot(&self) -> bool {
        self.options.screenshot.is_some()
    }

    /// 切主题：重建 [`Ui`]（= 重建 Style），**不动任何 UI 状态**。
    ///
    /// M93 起 `Ui` 有两个来源：配色来源取到了图（①②级）就用解析出的方案，否则用令牌 baseline。
    /// 所以这里改叫「重算 `Ui`」：先让外观状态记住新深浅（配色解析要它），再重建。
    pub(in crate::app) fn set_flavor(&mut self, flavor: SchemeFlavor) {
        if self.ui.flavor() == flavor && self.appearance.flavor() == flavor {
            return;
        }

        self.appearance.set_flavor(flavor);
        let previous = self.ui.name();
        self.rebuild_ui();
        diag!(
            "[theme] {} -> {} (Theme::name 变化触发缓存重算)",
            previous,
            self.ui.name()
        );
    }

    /// 按「强调色 / 配色来源是否接管」重建 [`Ui`]。
    ///
    /// **优先级（M250）：显式强调色 > D38 取色方案 > 出厂品牌板**。手填的色是直接意图，取色是从
    /// 图片派生的，所以强调色只在用户真的填了合法值时压过取色——两条路都只决定「颜色来自哪里」，
    /// `flavor`（明暗 / 跟随系统）仍由 `self.appearance.flavor()` 决定。
    ///
    /// 色卡档（③）刻意继续走 `Scheme::baseline`：`resolve_source` 的色卡档返回的是
    /// `DEFAULT_CARD`（天蓝卡）方案，与令牌 baseline 逐色不同，应用它会把**全部既有验收图的
    /// 像素**改掉。这条偏离登记在 `pages::appearance` 的偏离表 #4。
    pub(in crate::app) fn rebuild_ui(&mut self) {
        // 面板 α（D36）在这里注入 `Ui`：全仓所有表面色都从 `Ui::layer` 出去，所以这是**唯一**
        // 生效点（外壳 / 三页卡片 / 外观分组卡片 / 空态图标块自动同源，调用点没有机会各处自乘）。
        let panel_alpha = self.appearance.effective_panel_alpha();
        // 三选一规则只在 `theme::tokens` 里写一遍：出图 harness 也调它（两头各抄一份迟早
        // 出现「图上有强调色、产品里没有」，而那种偏差在图上看不出来）。
        let (ui, colors) = crate::theme::tokens(
            self.appearance.flavor(),
            self.appearance.accent.color(),
            self.appearance.derived_scheme().copied(),
            self.families,
        );
        match colors {
            crate::theme::Colors::Accent => {
                // M250：一行里同时给「强调色生效了」与「取色方案被谁压住」，两件事都能读出来。
                diag!(
                    "[appearance] accent={} → Ui::with_accent（只换 primary/secondary/tertiary；取色方案 palette={} 被压住）",
                    self.appearance.accent.log_token(),
                    self.appearance.palette_level()
                );
            }
            crate::theme::Colors::Scheme => {
                diag!(
                    "[appearance] palette={} → Ui::from_scheme（取色方案接管 49 个角色）",
                    self.appearance.palette_level()
                );
            }
            crate::theme::Colors::Board => {
                // 非法值：**保持出厂板**，但错行留在设置页（不静默），日志里也留一条。
                if let Some(error) = self.appearance.accent.error() {
                    diag!("[appearance] accent 非法（{error:?}）→ 出厂板，错行见设置页");
                }
            }
        }
        self.ui = ui.with_panel_alpha(panel_alpha);
    }

    /// 按设置文件里的主题模式切一次外观；`--theme` 显式给了就不动。
    ///
    /// **截图通道直接跳过**：`--screenshot` 的主题唯一取值口是 `Options::capture_flavor`
    /// （D30 可复现），读本机 `launcher.json` 会让同一份代码在两台机器上出不同的图。
    ///
    /// `ThemeMode::Auto` 不在这里处理：那要看系统主题（`iced::system::theme()` 的结果在
    /// `Message::SystemTheme` 里落地）。
    pub(in crate::app) fn apply_config_theme(&mut self) {
        if self.has_screenshot() || self.options.theme.is_some() {
            return;
        }
        let mode = self.bridge.load().1;
        self.theme_mode = mode;
        // M229：设置页那一行画的是这个字段（唯一写者见 [`App::set_theme_mode`]）。
        self.settings.language_appearance.theme = mode;
        match mode {
            ThemeMode::Light => self.set_flavor(SchemeFlavor::Light),
            ThemeMode::Dark => self.set_flavor(SchemeFlavor::Dark),
            ThemeMode::Auto => {}
        }
    }

    /// 换主题档（M229：设置页「通用 → 主题」→ `pages::Message::Theme`）。
    ///
    /// 四件事：①改 `theme_mode`（唯一真相）②同步设置页那一行的档位 ③重建 `Ui`（色板换了，
    /// `Theme::name` 与 49 个角色一起变，iced 才会重算缓存）④落盘 `appearance.theme_mode`。
    /// `Auto` 自己没有颜色，要等系统回答——所以返回 `iced::system::theme()` 那个 Task，
    /// 由既有的 `Message::SystemTheme` 分支落地（它只在 `theme_mode == Auto` 时吃这条消息）。
    pub(in crate::app) fn set_theme_mode(&mut self, mode: ThemeMode) -> Task<Message> {
        self.settings.language_appearance.theme = mode;
        if mode == self.theme_mode {
            return Task::none();
        }
        self.theme_mode = mode;

        let prefs = self.appearance_prefs.clone();
        self.bridge.persist(&prefs, mode);
        match self.bridge.path() {
            Some(path) => diag!("[theme] mode={} 已写回 {}", mode.as_str(), path.display()),
            None => diag!(
                "[theme] mode={} 未落盘（数据根解析不出来，内存态运行）：重启会回到旧值",
                mode.as_str()
            ),
        }

        match mode {
            ThemeMode::Light => {
                self.set_flavor(SchemeFlavor::Light);
                self.after_theme_switch();
                Task::none()
            }
            ThemeMode::Dark => {
                self.set_flavor(SchemeFlavor::Dark);
                self.after_theme_switch();
                Task::none()
            }
            // 跟随系统：当前先按已有深浅画，紧接着由系统主题回答覆盖
            // （`set_flavor` 有「同档不重建」的保护，所以这里不必先猜一次）。
            ThemeMode::Auto => iced::system::theme().map(Message::SystemTheme),
        }
    }

    /// 换档之后重建视图模型 + 记一行取证日志（色板名就是证据：`ink` / `silver`）。
    fn after_theme_switch(&mut self) {
        self.view_dirty = true;
        self.refresh_view();
        diag!(
            "[theme] mode={} board={} Theme::name={}",
            self.theme_mode().as_str(),
            self.board_name(),
            self.ui.name()
        );
    }

    /// 当前主题档（设置页那一行的选中态来自 `settings.groups`，这里是取证与测试的读数口）。
    pub(in crate::app) fn theme_mode(&self) -> ThemeMode {
        self.theme_mode
    }

    /// 当前色板名（日志与测试用：`ink` / `silver`；取色方案接管时仍报它所属深浅的那块）。
    pub(in crate::app) fn board_name(&self) -> &'static str {
        self.ui.board().name()
    }

    /// 当前令牌求值器（测试与截图日志用）。
    pub(in crate::app) fn ui(&self) -> &Ui {
        &self.ui
    }
}

#[cfg(test)]
mod tests {
    use super::super::App;
    use crate::app::components::feedback::progress;
    use crate::cli::Options;
    use crate::pages::appearance::Change;
    use crate::theme::Families;
    use unlucky_design::color::{Color, SchemeFlavor};
    use unlucky_presenter::mock::MockBackend;
    use unlucky_presenter::settings_bridge::SettingsBridge;

    const FAMILIES: Families = Families {
        text: "Segoe UI",
        display: "Segoe UI",
        mono: "Consolas",
    };

    /// 强调色接线（M250 ACCENTWIRE）的 App 级判据：`rebuild_ui` 真的换了令牌。
    ///
    /// **这条是判据一的变异探针**：把 [`App::rebuild_ui`] 改回「恒走 `Ui::new` 出厂板」
    /// （或在它前面短路掉 `accent.color()` 分支），下面 `assert_ne!` 当场红——`pages/appearance`
    /// 那条用例只证明「`Ui::with_accent` 能给出不同的色」，抓不到「谁去调它」。
    ///
    /// 抓得住：接线不存在 / 被短路 / 强调色解析结果没进 `Ui`。
    /// 抓不住：像素（样式闭包被别处覆盖）——那是出图的事。
    #[test]
    fn rebuild_ui_follows_the_configured_accent() {
        let mut app = App::with_presenter(
            Options::default(),
            FAMILIES,
            SchemeFlavor::Light,
            Box::new(MockBackend::new()),
            SettingsBridge::memory_only(),
        );

        // 出厂态：同一组件（进度指示条 = `Role::Primary`）取到的值
        let (_, factory_primary) = progress::linear_colors(app.ui());
        assert_eq!(app.ui().name(), "ull-silver");

        // 设置页填色 → `on_appearance` 会 rebuild 一次（与真机路径同一个函数）
        app.on_appearance(Change::Accent(Color::rgb(0x00, 0x6A, 0x5C).to_hex()));
        let (_, accent_primary) = progress::linear_colors(app.ui());

        assert_ne!(
            factory_primary, accent_primary,
            "填了自定义强调色，`rebuild_ui` 却还给出出厂板的 Primary ⇒ 接线没落地"
        );
        assert_eq!(
            app.ui().flavor(),
            SchemeFlavor::Light,
            "强调色只换颜色来源，不改明暗"
        );
    }
}
