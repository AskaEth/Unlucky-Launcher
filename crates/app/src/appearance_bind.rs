//! 「外观（高级）」四件的回调接线（从 `callbacks.rs` 拆出，守住单文件 ≤400 行）。
//!
//! 与 `wire_settings` 的分工：这个文件只负责 09 §101-104 的四个高级特性
//! （背景图 / 面板半透明度 / 玻璃拟态 / 强调色）＋ 主题三态。
//! 每个回调都是同一套动作：改 `state.prefs` → 下发到 Slint（`apply_*`）→ 落盘（`persist_settings`）→
//! `sync()`。出厂全部关闭，所以「不碰设置页」时这里一次都不会触发。

use std::rc::Weak;

use crate::conv;
use crate::ui_bind::Inner;

/// 接主题三态与外观四件的回调。
pub(crate) fn wire(ui: &crate::MainWindow, weak: &Weak<Inner>) {
    {
        let weak = weak.clone();
        ui.on_theme_changed(move |mode| {
            let Some(this) = weak.upgrade() else { return };
            this.state.borrow_mut().prefs.theme = conv::theme_mode_from(mode);
            this.apply_theme();
            this.persist_settings();
            this.sync();
        });
    }
    // ── 09 §101-104「外观（高级）」四件（出厂全关，改动即刻落盘）────────
    {
        let weak = weak.clone();
        ui.on_bg_image_enabled_changed(move |enabled| {
            let Some(this) = weak.upgrade() else { return };
            this.state.borrow_mut().prefs.appearance.bg_image_enabled = enabled;
            this.apply_appearance();
            this.persist_settings();
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_bg_image_changed(move |path| {
            let Some(this) = weak.upgrade() else { return };
            this.state.borrow_mut().prefs.appearance.bg_image = path.to_string();
            this.apply_appearance();
            this.persist_settings();
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_bg_fill_changed(move |fill| {
            let Some(this) = weak.upgrade() else { return };
            this.state.borrow_mut().prefs.appearance.bg_fill = conv::bg_fill_from(fill);
            this.apply_appearance();
            this.persist_settings();
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_bg_dim_changed(move |value| {
            let Some(this) = weak.upgrade() else { return };
            this.state.borrow_mut().prefs.appearance.bg_dim = value;
            this.apply_appearance();
            this.persist_settings();
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_panel_alpha_changed(move |value| {
            let Some(this) = weak.upgrade() else { return };
            this.state.borrow_mut().prefs.appearance.panel_alpha = value;
            this.apply_appearance();
            this.persist_settings();
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_glass_changed(move |enabled| {
            let Some(this) = weak.upgrade() else { return };
            this.state.borrow_mut().prefs.appearance.glass = enabled;
            this.apply_appearance();
            this.persist_settings();
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_accent_color_changed(move |text| {
            let Some(this) = weak.upgrade() else { return };
            let accepted = this.state.borrow_mut().prefs.appearance.set_accent(&text);
            // 非法输入：只亮设置页的 warn 行，不改现状、不落盘
            this.ui.set_accent_invalid(!accepted);
            if accepted {
                this.apply_appearance();
                this.persist_settings();
            }
            this.sync();
        });
    }
    {
        let weak = weak.clone();
        ui.on_accent_swatch_picked(move |index| {
            let Some(this) = weak.upgrade() else { return };
            let Ok(index) = usize::try_from(index) else {
                return;
            };
            let accepted = this
                .state
                .borrow_mut()
                .prefs
                .appearance
                .set_accent_preset(index);
            if !accepted {
                return;
            }
            let hex = this.state.borrow().prefs.appearance.accent_color.clone();
            this.ui.set_accent_color(conv::accent_color(&hex));
            this.ui.set_accent_invalid(false);
            this.apply_appearance();
            this.persist_settings();
            this.sync();
        });
    }
}
