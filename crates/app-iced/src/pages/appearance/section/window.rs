//! 外观分组第 ① ② 段：**整窗**两件——整窗透明度滑块、窗口圆角（含 notice 与说明段）。
//!
//! 这两段是唯一**一个控件牵动另一个控件**的地方：圆角「跟随系统」时把整窗透明度调低会让圆角失效
//! （`DwmExtendFrameIntoClientArea` 的世界里没有半透明边框），所以只有这一对放在同一个文件里——
//! notice 的判据（`corner == System && window_opacity < 1.0`）跨这两段。

use iced::widget::space;
use iced::Element;

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_i18n::catalog::Key;
use unlucky_i18n::Lang;
use unlucky_presenter::prefs::WindowCorner;

use crate::app::components::inputs::{slider, Slider};
use crate::pages::Message;
use crate::theme::Ui;

use super::super::alpha::{window_alpha_from_fraction, window_fraction, WINDOW_ALPHA_FLOOR};
use super::super::state::State;
use super::super::Change;
use super::{choice, choice_row, fixed2, note_line, slider_row, t, tr};

/// 把 ① 整窗透明度与 ② 窗口圆角按视觉顺序推进 `rows`。
pub(super) fn push<'a>(
    ui: &'a Ui,
    state: &'a State,
    lang: Lang,
    rows: &mut Vec<Element<'a, Message>>,
) {
    // ① 整窗透明度
    let window_percent = format!("{:.0}%", state.window_opacity * 100.0);
    rows.push(slider_row(
        ui,
        t(Key::SettingsAppearanceWindowOpacity, lang),
        window_percent,
        tr(
            lang,
            Key::SettingsAppearanceWindowOpacityHint,
            &[("floor", &fixed2(WINDOW_ALPHA_FLOOR))],
        ),
        state.window_opacity < 1.0,
        slider(
            Slider::new(ui, window_fraction(state.window_opacity)).on_change(|fraction| {
                Message::Appearance(Change::WindowOpacity(window_alpha_from_fraction(fraction)))
            }),
        ),
    ));

    // ② 窗口圆角（M116）：只有两档 + 一句按平台说明；不做 8/12/16 档（系统不暴露半径）。
    let corner = state.window_corner();
    rows.push(choice_row(
        ui,
        t(Key::SettingsAppearanceCorner, lang),
        vec![
            choice(
                ui,
                t(Key::SettingsAppearanceCornerSystem, lang),
                corner == WindowCorner::System,
                Change::Corner(WindowCorner::System),
            ),
            choice(
                ui,
                t(Key::SettingsAppearanceCornerSquare, lang),
                corner == WindowCorner::Square,
                Change::Corner(WindowCorner::Square),
            ),
        ],
    ));
    // **tonal notice**（r5 视觉终稿）：M3 里表达「当前状态」的惯用形态是**容器**，不是改字色。
    // 只在「跟随系统 + α<1」出现；与下面的说明段之间再留 `Space::X1`（列间距 X1 + 这里 X1 = X2）。
    if corner == WindowCorner::System && state.window_opacity < 1.0 {
        rows.push(super::super::notice(
            ui,
            t(Key::SettingsAppearanceCornerSuspended, lang),
        ));
        let gap: Element<'a, Message> = space::vertical().height(ui.space(Space::X1)).into();
        rows.push(gap);
    }
    let tone = ui.color(Role::OnSurfaceVariant);
    rows.push(note_line(
        ui,
        t(Key::SettingsAppearanceCornerHint, lang),
        tone,
    ));
}
