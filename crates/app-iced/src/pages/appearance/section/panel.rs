//! 外观分组第 ③ 段：**面板透明度**滑块（下限 0.4；CPU 档强制 1.0 并禁用滑块）。

use iced::Element;

use unlucky_i18n::catalog::Key;
use unlucky_i18n::Lang;

use crate::app::components::inputs::{slider, Slider};
use crate::pages::Message;
use crate::theme::Ui;

use super::super::alpha::{panel_alpha_from_fraction, panel_fraction, PANEL_ALPHA_FLOOR};
use super::super::state::State;
use super::super::Change;
use super::{fixed2, slider_row, t, tr};

/// 把 ③ 面板透明度推进 `rows`。
pub(super) fn push<'a>(
    ui: &'a Ui,
    state: &'a State,
    lang: Lang,
    rows: &mut Vec<Element<'a, Message>>,
) {
    // ③ 面板透明度（CPU 档强制 1.0）
    let forced = state.panel_is_forced();
    let panel_percent = if forced {
        // 数值占位：`{requested}` 是请求值（词表不认 `{:.2}`，所以在这里先格式化成两位小数）。
        tr(
            lang,
            Key::SettingsAppearancePanelOpacityForcedValue,
            &[("requested", &fixed2(state.panel_alpha))],
        )
    } else {
        format!("{:.0}%", state.effective_panel_alpha() * 100.0)
    };
    let panel_hint = if forced {
        t(Key::SettingsAppearancePanelOpacityForcedHint, lang).to_owned()
    } else {
        tr(
            lang,
            Key::SettingsAppearancePanelOpacityHint,
            &[("floor", &fixed2(PANEL_ALPHA_FLOOR))],
        )
    };
    let panel_control = if forced {
        slider(Slider::new(ui, panel_fraction(1.0)))
    } else {
        slider(
            Slider::new(ui, panel_fraction(state.panel_alpha)).on_change(|fraction| {
                Message::Appearance(Change::PanelAlpha(panel_alpha_from_fraction(fraction)))
            }),
        )
    };
    rows.push(slider_row(
        ui,
        t(Key::SettingsAppearancePanelOpacity, lang),
        panel_percent,
        panel_hint,
        !forced,
        panel_control,
    ));
}
