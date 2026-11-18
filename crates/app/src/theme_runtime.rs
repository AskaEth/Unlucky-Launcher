//! 主题与外观的运行期逻辑（从 `ui_bind.rs` 拆出，守住单文件 ≤400 行）。
//!
//! 全是 `impl Inner` 的方法：配置热切轮询、系统主题对齐、外观令牌下发、背景图/模糊层重建。
//! 拆出来的边界是「时间驱动的状态维护」——`ui_bind.rs` 只留「建窗口 + 每拍驱动 + 灌属性」。

use slint::ComponentHandle;

use crate::bg_image::{self, BackgroundLayers};
use crate::conv;
use crate::ui_bind::Inner;
use unlucky_presenter::prefs::{AppearancePrefs, BgFill, ThemeMode};
use unlucky_presenter::theme::{
    config_poll_due, system_theme_poll_due, system_theme_value, BackgroundKey,
};
use unlucky_presenter::toast::ToastKind;

impl Inner {
    pub(crate) fn poll_settings(&self) {
        if !config_poll_due(self.ticks.get()) {
            return;
        }
        let Some((appearance, theme)) = self.bridge.borrow_mut().reload_if_changed() else {
            return;
        };
        {
            let mut state = self.state.borrow_mut();
            state.prefs.appearance = appearance;
            state.prefs.theme = theme;
        }
        self.apply_theme();
        self.apply_appearance();
    }

    /// 系统主题对齐：启动时先读一次真实系统主题（避免 auto 首帧按浅色画再翻黑）；
    /// 之后仅在「auto + 后端不认识系统主题」（`Theme.system-theme-known == false`）时兜底轮询。
    pub(crate) fn poll_system_theme(&self) {
        let auto = matches!(self.state.borrow().prefs.theme, ThemeMode::Auto);
        let backend_known = self.ui.get_system_theme_known();
        let due = system_theme_poll_due(
            self.system_theme_probed.get(),
            auto,
            backend_known,
            self.ticks.get(),
        );
        if !due {
            return;
        }
        self.system_theme_probed.set(true);
        let value = system_theme_value();
        if self.ui.get_system_theme() != value {
            self.ui.set_system_theme(value);
        }
    }

    /// 启动时的系统主题对齐（首帧前调用一次）。
    pub(crate) fn align_system_theme(&self) {
        self.poll_system_theme();
    }

    /// 主题三态 → 根组件属性；`ui/main.slint` 里的 changed 处理器再赋给 `Theme` 全局
    /// （auto 时 Theme 内部读系统的 `Palette.color-scheme` 与本进程探测到的系统主题）。
    pub(crate) fn apply_theme(&self) {
        let mode = self.state.borrow().prefs.theme;
        self.ui.set_theme_mode(conv::theme_mode(mode));
    }

    /// 外观（高级）四件 → 根组件属性（进而写进 `Theme` 全局）。
    ///
    /// 背景图只在「开关打开 + 路径非空 + 换了输入」时才重新解码/生成模糊层。
    pub(crate) fn apply_appearance(&self) {
        let appearance = self.state.borrow().prefs.appearance.clone();
        self.ui.set_bg_fill(conv::bg_fill(appearance.bg_fill));
        self.ui.set_bg_dim(appearance.bg_dim.clamp(0.0, 0.9));
        self.ui
            .set_panel_alpha(appearance.panel_alpha.clamp(0.4, 1.0));
        self.ui.set_glass(appearance.glass);
        self.ui
            .set_accent_color(conv::accent_color(&appearance.accent_color));
        self.ui
            .set_accent_hex(appearance.accent_color.clone().into());
        self.refresh_background(&appearance);
    }

    /// 按需重建背景图 + 模糊层。
    fn refresh_background(&self, appearance: &AppearancePrefs) {
        if !appearance.bg_active() {
            let had = self.background_key.borrow_mut().take().is_some();
            if had {
                self.background.replace(BackgroundLayers::default());
                self.ui.set_background_image(slint::Image::default());
                self.ui.set_background_blur(slint::Image::default());
            }
            return;
        }

        let key = BackgroundKey {
            path: appearance.bg_image.clone(),
            fill: appearance.bg_fill,
            viewport: self.tile_viewport(appearance.bg_fill),
        };
        if self.background_key.borrow().as_ref() == Some(&key) {
            return;
        }
        self.background_key.replace(Some(key.clone()));

        // 模糊层按窗口物理尺寸放大（软件渲染器不做图片缩放插值，见 bg_image::prepare）；
        // tile 之外的模式窗口缩放不重建画布，模糊层被拉伸也无感。
        let real_viewport = self.window_physical_size();
        match bg_image::prepare(&key.path, key.fill, real_viewport) {
            Some(layers) => {
                self.ui.set_background_image(layers.image.clone());
                self.ui.set_background_blur(layers.blur.clone());
                self.background.replace(layers);
            }
            None => {
                // 载入失败（路径不存在 / 不是 PNG）：清空图层并提示一次，别留上一张图的残影
                self.background.replace(BackgroundLayers::default());
                self.ui.set_background_image(slint::Image::default());
                self.ui.set_background_blur(slint::Image::default());
                self.state.borrow_mut().toasts.push(
                    ToastKind::Warn,
                    "背景图载入失败（当前只支持 PNG，且路径需存在）",
                );
            }
        }
    }

    /// tile 画布的窗口尺寸（量化到 `TILE_REGENERATE_STEP`，避免拖动窗口时反复重算）。
    ///
    /// 判据在 [`unlucky_presenter::theme::tile_viewport`]（两侧 UI 共用一份）；这里只负责
    /// 提供窗口物理尺寸。
    fn tile_viewport(&self, fill: BgFill) -> (u32, u32) {
        if fill != BgFill::Tile {
            return (0, 0);
        }
        unlucky_presenter::theme::tile_viewport(fill, self.window_physical_size())
    }

    /// 窗口物理像素尺寸（取不到时 (0, 0)，tile 退化为单块）。
    fn window_physical_size(&self) -> (u32, u32) {
        let size = self.ui.window().size();
        (size.width, size.height)
    }

    /// 落盘当前偏好（主题 + 外观四件）。
    pub(crate) fn persist_settings(&self) {
        let state = self.state.borrow();
        self.bridge
            .borrow_mut()
            .persist(&state.prefs.appearance, state.prefs.theme);
    }
}
