//! 外观分组第 ④ ⑤ ⑥ ⑦ ⑧ 段：**背景图**——开关、路径、诊断、填充档、暗化强度。
//!
//! 这五段挤在一起是因为它们读同一份 `bg_*` 状态，而且**诊断必须紧贴控件**（复审 r1 P2-3：
//! 读盘失败 / CPU 降级 / 配色回落这些提示落在折叠线以下等于没提示）。

use iced::widget::column;
use iced::{Element, Length};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_i18n::catalog::Key;
use unlucky_i18n::Lang;
use unlucky_presenter::prefs::BgFill;

use crate::app::components::inputs::{slider, text_field, Field, Slider};
use crate::pages::Message;
use crate::theme::Ui;

use super::super::background::{self, BG_DIM_MAX};
use super::super::palette;
use super::super::state::State;
use super::super::Change;
use super::{choice, choice_row, fixed2, label, note_line, slider_row, t, tr, tr_note};

/// 把 ④ 开关、⑤ 路径、⑥ 诊断、⑦ 填充、⑧ 暗化按视觉顺序推进 `rows`。
pub(super) fn push<'a>(
    ui: &'a Ui,
    state: &'a State,
    lang: Lang,
    rows: &mut Vec<Element<'a, Message>>,
) {
    // ④ 背景图开关
    rows.push(choice_row(
        ui,
        t(Key::SettingsAppearanceBackgroundImage, lang),
        vec![
            choice(
                ui,
                t(Key::SettingsAppearanceOff, lang),
                !state.bg_enabled,
                Change::BgEnabled(false),
            ),
            choice(
                ui,
                t(Key::SettingsAppearanceOn, lang),
                state.bg_enabled,
                Change::BgEnabled(true),
            ),
        ],
    ));

    // ⑤ 路径（文本框：受控，输入即生效）
    let path_field: Element<'a, Message> = text_field(
        ui,
        Field::new(&state.bg_path)
            .label(t(Key::SettingsAppearanceImagePath, lang))
            .placeholder("D:/Pictures/background.png"),
        |value| Message::Appearance(Change::BgImage(value)),
    );
    rows.push(
        column![
            label(ui, t(Key::SettingsAppearanceImagePath, lang)),
            path_field
        ]
        .spacing(ui.space(Space::X1))
        .width(Length::Fill)
        .into(),
    );

    // ⑥ 诊断（读盘失败 / 配色回落）——**不许静默**，且必须在**首屏可见处**
    // （复审 r1 P2-3：文案落在折叠线以下等于没提示）。位置就在背景图那两行的紧下方。
    //
    // CPU 档的「面板/玻璃被强制」那句**不在这里**：它是张卡级状态，挂在被禁用的面板滑块旁边
    // （`panel.rs` 的 `PanelOpacityForcedHint`），一屏只出现一次（M180 复审 P2-1）。
    for note in state.diagnostics() {
        let text = tr_note(lang, note);
        rows.push(note_line(ui, &text, ui.color(Role::Error)));
    }
    if let Some(note) = state
        .image()
        .and_then(|image| background::small_tile_note(image, state.bg_fill))
    {
        let text = tr_note(lang, &note);
        rows.push(note_line(ui, &text, ui.color(Role::OnSurfaceVariant)));
    }
    if let Some(key) = state.palette_note() {
        rows.push(note_line(
            ui,
            t(key, lang),
            ui.color(Role::OnSurfaceVariant),
        ));
    }
    // 生效的配色层级：显示的是**回落之后真正生效**的那一级（与底部「配色来源」里用户选的那一档
    // 不一定相同）。码值 → 已有的三条档位 key（不新增平行词表），所以中文界面也读得懂。
    let level = tr(
        lang,
        Key::SettingsAppearancePaletteLevel,
        &[("level", t(palette::level_key(state.palette_level()), lang))],
    );
    rows.push(note_line(ui, &level, ui.color(Role::OnSurfaceVariant)));

    // ⑦ 填充
    rows.push(choice_row(
        ui,
        t(Key::SettingsAppearanceBgFill, lang),
        vec![
            choice(
                ui,
                t(Key::SettingsAppearanceBgFillCover, lang),
                state.bg_fill == BgFill::Cover,
                Change::BgFill(BgFill::Cover),
            ),
            choice(
                ui,
                t(Key::SettingsAppearanceBgFillContain, lang),
                state.bg_fill == BgFill::Contain,
                Change::BgFill(BgFill::Contain),
            ),
            choice(
                ui,
                t(Key::SettingsAppearanceBgFillTile, lang),
                state.bg_fill == BgFill::Tile,
                Change::BgFill(BgFill::Tile),
            ),
        ],
    ));

    // ⑧ 暗化
    rows.push(slider_row(
        ui,
        t(Key::SettingsAppearanceBgDim, lang),
        format!("{:.0}%", state.bg_dim * 100.0),
        tr(
            lang,
            Key::SettingsAppearanceBgDimHint,
            &[("max", &fixed2(BG_DIM_MAX))],
        ),
        state.bg_dim > 0.0,
        slider(
            Slider::new(ui, state.bg_dim / BG_DIM_MAX)
                .on_change(|fraction| Message::Appearance(Change::BgDim(fraction * BG_DIM_MAX))),
        ),
    ));
}
