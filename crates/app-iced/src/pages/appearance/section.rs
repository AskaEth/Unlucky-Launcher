//! 设置页「外观」分组（**真控件**）+ 背景层 Element + 面板层配色。
//!
//! ## 控件与键的对应
//!
//! | 控件 | 键 | 改动消息 |
//! |---|---|---|
//! | 整窗透明度滑块（下限 0.08） | `appearance.window_opacity` | [`Change::WindowOpacity`] |
//! | 窗口圆角（跟随系统 / 直角，M116） | `appearance.window_corner` | [`Change::Corner`] |
//! | 面板透明度滑块（下限 0.4，CPU 档禁用） | `appearance.panel_alpha` | [`Change::PanelAlpha`] |
//! | 背景图开关 | `appearance.bg_image_enabled` | [`Change::BgEnabled`] |
//! | 背景图路径（文本框） | `appearance.bg_image` | [`Change::BgImage`] |
//! | 填充（封面 / 拉伸 / 平铺） | `appearance.bg_fill` | [`Change::BgFill`] |
//! | 暗化强度（0~0.95） | `appearance.bg_dim` | [`Change::BgDim`] |
//! | 配色来源（三档） | `appearance.palette_source` | [`Change::PaletteSource`] |
//!
//! 文案一律英文基线（i18n 是别的 mission；本文件不拼中文、不 import 尚不存在的 crate）。
//!
//! ## 为什么「值」与「生效值」分开显示
//!
//! CPU 档下面板 α 被强制 1.0（D36），而用户请求值可能是 0.5。滑块位置按**生效值**画、旁边写
//! 「requested 0.50 → forced 1.00」，这样「为什么拖了没反应」在界面上就有答案，而不是只躺在日志里。

use iced::widget::{button, column, container, row, space};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Padding, Shadow, Theme as IcedTheme,
};

use unlucky_design::color::Role;
use unlucky_design::component_map::Component;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::catalog::Key;
use unlucky_presenter::prefs::{BgFill, PaletteSource, WindowCorner};

use crate::app::components::inputs::{slider, text_field, Field, Slider};
use crate::pages::{frame, Message};
use crate::theme::Ui;

use super::alpha::{
    panel_alpha_from_fraction, panel_fraction, window_alpha_from_fraction, window_fraction,
    WINDOW_ALPHA_FLOOR,
};
use super::background::{self, BG_DIM_MAX};
use super::state::State;
use super::Change;

/// 「外观」分组整块（一张卡）。
pub fn section<'a>(ui: &'a Ui, state: &'a State) -> Element<'a, Message> {
    let mut rows = column![].spacing(ui.space(Space::X1)).width(Length::Fill);

    // ① 整窗透明度
    let window_percent = format!("{:.0}%", state.window_opacity * 100.0);
    rows = rows.push(slider_row(
        ui,
        "Window transparency",
        window_percent,
        format!(
            "Applies to the whole window. Floor {:.2}; 1.00 = fully opaque.",
            WINDOW_ALPHA_FLOOR
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
    rows = rows.push(choice_row(
        ui,
        t(Key::SettingsAppearanceCorner),
        vec![
            choice(
                ui,
                t(Key::SettingsAppearanceCornerSystem),
                corner == WindowCorner::System,
                Change::Corner(WindowCorner::System),
            ),
            choice(
                ui,
                t(Key::SettingsAppearanceCornerSquare),
                corner == WindowCorner::Square,
                Change::Corner(WindowCorner::Square),
            ),
        ],
    ));
    // **tonal notice**（r5 视觉终稿）：M3 里表达「当前状态」的惯用形态是**容器**，不是改字色。
    // 只在「跟随系统 + α<1」出现；与下面的说明段之间再留 `Space::X1`（列间距 X1 + 这里 X1 = X2）。
    if corner == WindowCorner::System && state.window_opacity < 1.0 {
        rows = rows.push(super::notice(ui, t(Key::SettingsAppearanceCornerSuspended)));
        let gap: Element<'a, Message> = space::vertical().height(ui.space(Space::X1)).into();
        rows = rows.push(gap);
    }
    let tone = ui.color(Role::OnSurfaceVariant);
    rows = rows.push(note_line(ui, t(Key::SettingsAppearanceCornerHint), tone));

    // ③ 面板透明度（CPU 档强制 1.0）
    let forced = state.panel_is_forced();
    let panel_percent = if forced {
        format!("100% (forced from {:.2})", state.panel_alpha)
    } else {
        format!("{:.0}%", state.effective_panel_alpha() * 100.0)
    };
    let panel_hint = if forced {
        "Software renderer: panels stay fully opaque (window transparency still works)."
    } else {
        "Only the panel/card layer. Floor 0.40; independent of window transparency."
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
    rows = rows.push(slider_row(
        ui,
        "Panel transparency",
        panel_percent,
        panel_hint.to_owned(),
        !forced,
        panel_control,
    ));

    // ④ 背景图开关
    rows = rows.push(choice_row(
        ui,
        "Background image",
        vec![
            choice(ui, "Off", !state.bg_enabled, Change::BgEnabled(false)),
            choice(ui, "On", state.bg_enabled, Change::BgEnabled(true)),
        ],
    ));

    // ⑤ 路径（文本框：受控，输入即生效）
    let path_field: Element<'a, Message> = text_field(
        ui,
        Field::new(&state.bg_path)
            .label("Image path")
            .placeholder("D:/Pictures/background.png"),
        |value| Message::Appearance(Change::BgImage(value)),
    );
    rows = rows.push(
        column![label(ui, "Image path"), path_field]
            .spacing(ui.space(Space::X1))
            .width(Length::Fill),
    );

    // ⑥ 诊断（读盘失败 / CPU 降级 / 配色回落）——**不许静默**，且必须在**首屏可见处**
    // （复审 r1 P2-3：文案落在折叠线以下等于没提示）。位置就在背景图那两行的紧下方。
    for note in state.diagnostics() {
        rows = rows.push(note_line(ui, note, ui.color(Role::Error)));
    }
    if let Some(note) = state.backend_note() {
        rows = rows.push(note_line(ui, note, ui.color(Role::OnSurfaceVariant)));
    }
    if let Some(note) = state
        .image()
        .and_then(|image| background::small_tile_note(image, state.bg_fill))
    {
        rows = rows.push(note_line(ui, &note, ui.color(Role::OnSurfaceVariant)));
    }
    if let Some(note) = state.palette_note() {
        rows = rows.push(note_line(ui, note, ui.color(Role::OnSurfaceVariant)));
    }
    let resolved = format!("Colours: {}", state.palette_level());
    rows = rows.push(note_line(ui, &resolved, ui.color(Role::OnSurfaceVariant)));

    // ⑦ 填充
    rows = rows.push(choice_row(
        ui,
        "Fill",
        vec![
            choice(
                ui,
                "Cover",
                state.bg_fill == BgFill::Cover,
                Change::BgFill(BgFill::Cover),
            ),
            choice(
                ui,
                "Contain",
                state.bg_fill == BgFill::Contain,
                Change::BgFill(BgFill::Contain),
            ),
            choice(
                ui,
                "Tile",
                state.bg_fill == BgFill::Tile,
                Change::BgFill(BgFill::Tile),
            ),
        ],
    ));

    // ⑧ 暗化
    rows = rows.push(slider_row(
        ui,
        "Dim",
        format!("{:.0}%", state.bg_dim * 100.0),
        format!(
            "Blends the image toward the panel colour so text stays readable. Max {BG_DIM_MAX:.2}."
        ),
        state.bg_dim > 0.0,
        slider(
            Slider::new(ui, state.bg_dim / BG_DIM_MAX)
                .on_change(|fraction| Message::Appearance(Change::BgDim(fraction * BG_DIM_MAX))),
        ),
    ));

    // ⑨ 配色来源
    rows = rows.push(choice_row(
        ui,
        "Colour source",
        vec![
            choice(
                ui,
                "Background",
                state.palette_source == PaletteSource::BackgroundImage,
                Change::PaletteSource(PaletteSource::BackgroundImage),
            ),
            choice(
                ui,
                "Wallpaper",
                state.palette_source == PaletteSource::Wallpaper,
                Change::PaletteSource(PaletteSource::Wallpaper),
            ),
            choice(
                ui,
                "Preset",
                state.palette_source == PaletteSource::Preset,
                Change::PaletteSource(PaletteSource::Preset),
            ),
        ],
    ));

    card(ui, rows)
}

/// 外观卡片：底色走 `ui.layer`（面板 α 的唯一来源），与其它页的卡片同源。
fn card<'a>(ui: &'a Ui, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    let spec = Component::OutlinedCard.spec();
    let background = ui.layer(MaterialLayer::Card);

    container(content)
        .width(Length::Fill)
        .padding(ui.metrics().card_padding)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(background)),
            border: ui.outlined(spec.shape),
            ..container::Style::default()
        })
        .into()
}

// --------------------------------------------------------------- 行与控件

fn label<'a>(ui: &'a Ui, content: &'a str) -> Element<'a, Message> {
    frame::text_role(
        ui,
        TypeRole::BodyMedium,
        false,
        content,
        ui.color(Role::OnSurface),
    )
}

/// 设置页文案的 i18n 取词口（M116 起新条目一律走词表，不再写字面量）。
///
/// 为什么是 [`Lang::En`]：本文件的既有文案是英文基线（i18n 接线是另一条 mission，
/// 见本文件头部「文案一律英文基线」）。走词表的意义是**新增条目已经有中英两套**，
/// 接线时只换这一行，不用回来重写控件。
fn t(key: Key) -> &'static str {
    unlucky_i18n::Lang::En.catalog().text(key)
}

fn note_line<'a>(ui: &'a Ui, content: &str, color: Color) -> Element<'a, Message> {
    // 诊断文案是**运行时拼出来的**（路径、错误原因），只能走 owned 版本。
    frame::text_role_owned(ui, TypeRole::BodySmall, false, content.to_owned(), color)
}

/// 一行「标签 + 值 + 滑块 + 说明」。
fn slider_row<'a>(
    ui: &'a Ui,
    title: &'a str,
    value: String,
    hint: String,
    enabled: bool,
    control: Element<'a, Message>,
) -> Element<'a, Message> {
    let value_color = if enabled {
        ui.color(Role::OnSurface)
    } else {
        ui.color(Role::OnSurfaceVariant)
    };

    let head = row![
        container(label(ui, title))
            .width(Length::Fill)
            .align_y(Alignment::Center),
        frame::text_role_owned(ui, TypeRole::LabelLarge, true, value, value_color),
    ]
    .width(Length::Fill)
    .align_y(Alignment::Center);

    column![
        head,
        control,
        frame::text_role_owned(
            ui,
            TypeRole::BodySmall,
            false,
            hint,
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill)
    .padding(Padding::from([ui.space(Space::X2), 0.0]))
    .into()
}

/// 一行「标签 + 一排互斥按钮」。
fn choice_row<'a>(
    ui: &'a Ui,
    title: &'a str,
    choices: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    row![
        container(label(ui, title))
            .width(Length::Fill)
            .align_y(Alignment::Center),
        row(choices)
            .spacing(ui.space(Space::X1))
            .align_y(Alignment::Center),
    ]
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .padding(Padding::from([ui.space(Space::X2), 0.0]))
    .into()
}

/// 一个互斥按钮（选中态用 `SecondaryContainer`，未选中用透明 + `OnSurfaceVariant`）。
fn choice<'a>(ui: &'a Ui, text: &'a str, selected: bool, change: Change) -> Element<'a, Message> {
    let height = ui.metrics().control_height;
    let content_color = if selected {
        ui.color(Role::OnSecondaryContainer)
    } else {
        ui.color(Role::OnSurfaceVariant)
    };

    button(
        container(frame::text_role(
            ui,
            TypeRole::LabelLarge,
            selected,
            text,
            content_color,
        ))
        .align_y(Alignment::Center),
    )
    .height(height)
    .padding(Padding::from([0.0, ui.space(Space::X3)]))
    .style(move |_theme: &IcedTheme, status| {
        let background = if selected {
            Some(Background::Color(ui.color(Role::SecondaryContainer)))
        } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            Some(Background::Color(ui.state_over(
                unlucky_design::color::StateLayer::Hover,
                Role::OnSurface,
                Role::Surface,
            )))
        } else {
            None
        };

        button::Style {
            background,
            text_color: content_color,
            border: Border {
                radius: ui.radius(ShapeScale::Full, height),
                ..Border::default()
            },
            shadow: Shadow::default(),
            snap: true,
        }
    })
    .on_press(Message::Appearance(change))
    .into()
}
