//! 主题与 `Ui` 的重算口径（从 `state.rs` 搬出来守 400 行硬线；M119 拆，纯搬不改）。
//!
//! 三件事在这里：①`Ui` 什么时候重建（[`App::rebuild_ui`]）②主题档怎么定
//! （[`App::set_flavor`] / [`App::apply_config_theme`]）③读当前求值器（[`App::ui`]）。
//!
//! 截图通道的判据 [`App::has_screenshot`] 也放在这儿：上面三条里两条都要「截图不读用户配置」。

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
        eprintln!(
            "[theme] {} -> {} (Theme::name 变化触发缓存重算)",
            previous,
            self.ui.name()
        );
    }

    /// 按「配色来源是否取色成功」重建 [`Ui`]。
    ///
    /// 色卡档（③）刻意继续走 `Scheme::baseline`：`resolve_source` 的色卡档返回的是
    /// `DEFAULT_CARD`（天蓝卡）方案，与令牌 baseline 逐色不同，应用它会把**全部既有验收图的
    /// 像素**改掉。这条偏离登记在 `pages::appearance` 的偏离表 #4。
    pub(in crate::app) fn rebuild_ui(&mut self) {
        // 面板 α（D36）在这里注入 `Ui`：全仓所有表面色都从 `Ui::layer` 出去，所以这是**唯一**
        // 生效点（外壳 / 三页卡片 / 外观分组卡片 / 空态图标块自动同源，调用点没有机会各处自乘）。
        let panel_alpha = self.appearance.effective_panel_alpha();
        self.ui = match self.appearance.derived_scheme() {
            Some(scheme) => {
                eprintln!(
                    "[appearance] palette={} → Ui::from_scheme（取色方案接管 49 个角色）",
                    self.appearance.palette_level()
                );
                Ui::from_scheme(*scheme, self.families)
            }
            None => Ui::new(self.appearance.flavor(), self.families),
        }
        .with_panel_alpha(panel_alpha);
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
        match mode {
            ThemeMode::Light => self.set_flavor(SchemeFlavor::Light),
            ThemeMode::Dark => self.set_flavor(SchemeFlavor::Dark),
            ThemeMode::Auto => {}
        }
    }

    /// 当前令牌求值器（测试与截图日志用）。
    pub(in crate::app) fn ui(&self) -> &Ui {
        &self.ui
    }
}
