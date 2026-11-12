//! 设置页「General → 主题」行：Ink / Silver / 跟随系统三档（M229）。
//!
//! 从 `general.rs` 拆出来的理由只有一条：加这三档之后那个文件 474 行（400 行硬线）。
//! 拆法是「整段搬走 + 一个 `pub(super) fn view` 入口」（起初叫 `row`，与 `iced::widget::row`
//! 撞名后改名）；搬出的函数体只改了「未选中态补 1px `OutlineVariant` 描边」这一处，
//! 目的是与同页语言行的 chip 同几何（复审 F2 之后两行才对齐）。
//!
//! 三档的语义：**Ink = 深色主方案、Silver = 浅色主方案**，落盘取值仍是 `appearance.theme_mode`
//! 的 `dark` / `light`；第三档「跟随系统」= `auto`，由系统深浅落到对应色板
//! （`BrandBoard::for_flavor`）。所以这里**不新增配置键**，旧配置读进来语义也一致。

use iced::widget::{button, column, container, row};
use iced::{Alignment, Background, Border, Element, Length, Padding, Shadow, Theme as IcedTheme};

use unlucky_design::color::{Role, StateLayer};
use unlucky_design::inkcat::metrics as chipbox;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::catalog::{Catalog, Key};
use unlucky_presenter::prefs::ThemeMode;

use crate::pages::{frame, Message};
use crate::theme::Ui;

/// 三档全序（Ink / Silver / 跟随系统）：`ThemeMode` 在 presenter，声明序是 `Auto/Light/Dark`，
/// 画出来的顺序按「深 → 浅 → 跟随」，所以这里另给一张表，不靠枚举声明序。
pub(super) const THEME_MODES: [ThemeMode; 3] = [ThemeMode::Dark, ThemeMode::Light, ThemeMode::Auto];

/// 主题行：标题 + 三档互斥 chip + 一句说明（排版与语言行同族）。
pub(super) fn view<'a>(ui: &'a Ui, catalog: &Catalog, mode: ThemeMode) -> Element<'a, Message> {
    let head = row![
        container(frame::text_role(
            ui,
            TypeRole::BodyMedium,
            false,
            catalog.text(Key::SettingsTheme),
            ui.color(Role::OnSurface),
        ))
        .width(Length::Fill)
        .align_y(Alignment::Center),
        frame::text_role(
            ui,
            TypeRole::LabelLarge,
            true,
            catalog.text(mode_label(mode)),
            ui.color(Role::OnPrimaryFixedVariant),
        ),
    ]
    .width(Length::Fill)
    .align_y(Alignment::Center);

    let chips = row(THEME_MODES
        .iter()
        .map(|choice| {
            chip(
                ui,
                catalog.text(mode_label(*choice)),
                *choice == mode,
                *choice,
            )
        })
        .collect::<Vec<_>>())
    .spacing(ui.space(Space::X1))
    .align_y(Alignment::Center);

    column![
        head,
        chips,
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            catalog.text(Key::SettingsThemeHint),
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill)
    .padding(Padding::from([ui.space(Space::X2), 0.0]))
    .into()
}

/// 一档主题的选项名（Ink / Silver / 跟随系统）。
fn mode_label(mode: ThemeMode) -> Key {
    match mode {
        ThemeMode::Dark => Key::SettingsThemeInk,
        ThemeMode::Light => Key::SettingsThemeSilver,
        ThemeMode::Auto => Key::SettingsThemeFollowSystem,
    }
}

/// 一枚主题档位按钮（选中态 `SecondaryContainer`，未选中透明 + 1px `OutlineVariant`）。
///
/// 几何取规格卡的 chip（高 32、圆角 8，`inkcat::metrics::CHIP`）：主题三档是**可并排的筛选**，
/// 不是主行动按钮，用 chip 档比语言行那枚 40 高药丸更贴规格卡。
fn chip<'a>(ui: &'a Ui, text: &'a str, selected: bool, mode: ThemeMode) -> Element<'a, Message> {
    let box_ = chipbox::CHIP;
    let height = box_.height;
    let content_color = if selected {
        ui.color(Role::OnSecondaryContainer)
    } else {
        ui.color(Role::OnSurfaceVariant)
    };
    let border_color = ui.color(Role::OutlineVariant);
    let stroke = crate::theme::STROKE_PX;

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
    .padding(Padding::from([0.0, chipbox::CHIP_PAD_X]))
    .style(move |_theme: &IcedTheme, status| {
        let background = if selected {
            Some(Background::Color(ui.color(Role::SecondaryContainer)))
        } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            // M237：主题三档同样住在设置卡里，状态层叠卡底（`frame::BASE`）。
            Some(Background::Color(ui.state_over(
                StateLayer::Hover,
                Role::OnSurface,
                frame::BASE.role(),
            )))
        } else {
            None
        };

        button::Style {
            background,
            text_color: content_color,
            border: Border {
                color: border_color,
                width: if selected { 0.0 } else { stroke },
                radius: ui.radius(box_.shape, height),
            },
            shadow: Shadow::default(),
            snap: true,
        }
    })
    .on_press(Message::Theme(mode))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 三档 = 两块板 + 一种跟随，顺序是深 → 浅 → 跟随（换序会让「Ink 在最左」这条观感失守）。
    #[test]
    fn the_three_modes_are_ink_silver_and_follow_system() {
        assert_eq!(
            THEME_MODES,
            [ThemeMode::Dark, ThemeMode::Light, ThemeMode::Auto]
        );
        assert_eq!(mode_label(ThemeMode::Dark), Key::SettingsThemeInk);
        assert_eq!(mode_label(ThemeMode::Light), Key::SettingsThemeSilver);
        assert_eq!(mode_label(ThemeMode::Auto), Key::SettingsThemeFollowSystem);
    }
}
