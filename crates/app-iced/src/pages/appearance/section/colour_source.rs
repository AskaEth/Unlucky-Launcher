//! 外观分组第 ⑨ 段：**配色来源**（背景图 / 壁纸 / 色卡三档；它选的正是 D38 三级优先序的起点）。

use iced::Element;
use unlucky_i18n::catalog::Key;
use unlucky_i18n::Lang;
use unlucky_presenter::prefs::PaletteSource;

use crate::pages::Message;
use crate::theme::Ui;

use super::super::state::State;
use super::super::Change;
use super::{choice, choice_row, t};

/// 把 ⑨ 配色来源推进行 `rows`。
pub(super) fn push<'a>(
    ui: &'a Ui,
    state: &'a State,
    lang: Lang,
    rows: &mut Vec<Element<'a, Message>>,
) {
    // ⑨ 配色来源
    rows.push(choice_row(
        ui,
        t(Key::SettingsAppearancePaletteSource, lang),
        vec![
            choice(
                ui,
                t(Key::SettingsAppearancePaletteBackground, lang),
                state.palette_source == PaletteSource::BackgroundImage,
                Change::PaletteSource(PaletteSource::BackgroundImage),
            ),
            choice(
                ui,
                t(Key::SettingsAppearancePaletteWallpaper, lang),
                state.palette_source == PaletteSource::Wallpaper,
                Change::PaletteSource(PaletteSource::Wallpaper),
            ),
            choice(
                ui,
                t(Key::SettingsAppearancePalettePreset, lang),
                state.palette_source == PaletteSource::Preset,
                Change::PaletteSource(PaletteSource::Preset),
            ),
        ],
    ));
}
