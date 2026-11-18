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

use iced::border::Radius;
use iced::widget::{button, column, container, row};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Padding, Shadow, Theme as IcedTheme,
};

use unlucky_design::color::Role;
use unlucky_design::inkcat::metrics as chipbox;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::catalog::{Catalog, Key};
use unlucky_presenter::prefs::ThemeMode;

use crate::app::components::interaction::{self, Interaction};
use crate::pages::{frame, Message};
use crate::theme::Ui;

/// 三档全序（Ink / Silver / 跟随系统）：`ThemeMode` 在 presenter，声明序是 `Auto/Light/Dark`，
/// 画出来的顺序按「深 → 浅 → 跟随」，所以这里另给一张表，不靠枚举声明序。
pub(super) const THEME_MODES: [ThemeMode; 3] = [ThemeMode::Dark, ThemeMode::Light, ThemeMode::Auto];

/// 主题行：标题 + 三档互斥 chip + 一句说明（排版与语言行同族）。
///
/// `forced`（M285）只往下递给 chip：`None` = 按观测态画（产品路径的常态）。
pub(super) fn view<'a>(
    ui: &'a Ui,
    catalog: &Catalog,
    mode: ThemeMode,
    forced: Option<Interaction>,
) -> Element<'a, Message> {
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
            ui.color(super::VALUE_ECHO),
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
                forced,
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
fn chip<'a>(
    ui: &'a Ui,
    text: &'a str,
    selected: bool,
    mode: ThemeMode,
    forced: Option<Interaction>,
) -> Element<'a, Message> {
    let box_ = chipbox::CHIP;
    let height = box_.height;
    let content_color = if selected {
        ui.color(Role::OnSecondaryContainer)
    } else {
        ui.color(Role::OnSurfaceVariant)
    };
    let border_color = ui.color(Role::OutlineVariant);
    let stroke = crate::theme::STROKE_PX;
    let radius = ui.radius(box_.shape, height);

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
    .style(chip_style(
        ui,
        selected,
        content_color,
        border_color,
        stroke,
        radius,
        forced,
    ))
    .on_press(Message::Theme(mode))
    .into()
}

/// [`chip`] 的样式闭包（抽成自由函数：单测要**直接**喂 `Hovered` / `Pressed` 两态）。
///
/// - **α 只从 [`Interaction`] 来**：hover 8% / pressed 10%（M3）。M284 之前两种态合并成一条
///   `matches!(status, Hovered | Pressed)` 分支、都叠 `StateLayer::Hover`，按下于是被压成 8%；
/// - **合成底是 [`frame::BASE`]**（M237：主题三档住在设置卡里，状态层叠卡底而不是画布）。
fn chip_style<'a>(
    ui: &'a Ui,
    selected: bool,
    content_color: Color,
    border_color: Color,
    stroke: f32,
    radius: Radius,
    forced: Option<Interaction>,
) -> impl Fn(&IcedTheme, button::Status) -> button::Style + 'a {
    move |_theme, status| {
        let state = crate::pages::settings::resolve_state(forced, status);
        // 选中档位的底**也是**可交互的（M299）：走唯一入口 `selected_fill`——静息逐位等于
        // `SecondaryContainer`，hover / pressed 在它上面再叠一层状态层。
        let background = if selected {
            Some(Background::Color(interaction::selected_fill(
                ui,
                state,
                Role::OnSecondaryContainer,
                Role::SecondaryContainer,
            )))
        } else {
            interaction::layer_color(ui, state, Role::OnSurface, frame::BASE.role())
                .map(Background::Color)
        };

        button::Style {
            background,
            text_color: content_color,
            border: Border {
                color: border_color,
                width: if selected { 0.0 } else { stroke },
                radius,
            },
            shadow: Shadow::default(),
            snap: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unlucky_design::color::{SchemeFlavor, StateLayer};
    use unlucky_design::typography::Platform;

    use crate::theme::Families;

    fn ui() -> Ui {
        Ui::with_platform(
            SchemeFlavor::Light,
            Platform::Windows,
            Families {
                text: "Segoe UI",
                display: "Segoe UI",
                mono: "Consolas",
            },
        )
    }

    fn solid(style: button::Style) -> Color {
        match style.background {
            Some(Background::Color(color)) => color,
            None => panic!("未选中态的 hover/pressed 必须叠状态层"),
            Some(other) => panic!("背景必须是实色，得到 {other:?}"),
        }
    }

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

    /// hover(8%) 与 pressed(10%) 是**两档**，且各自逐值等于令牌表（把 10% 改成 11% 也会红）。
    ///
    /// 抓得住：①回到 `matches!(status, Hovered | Pressed)` 合并分支（两色相等 ⇒ 第一条红）；
    /// ②pressed 用 hover 的 α、或合成底被换回画布（第二、三条红）。
    /// 抓不住：观感；**过渡中间帧**（forced 路径不套 `tracked`，`layer_color` 直落端点值）。
    ///
    /// 底的角色取 `MaterialLayer::Card.surface_role()`（`frame::BASE` 就是它）：既盯住「叠的是卡底」
    /// 这条语义，也不往 `settings/*.rs` 的接线点计数里多塞一次字面量（`pages/tests/base_wiring.rs`）。
    fn assert_two_layers(ui: &Ui, style: impl Fn(&IcedTheme, button::Status) -> button::Style) {
        let theme = ui.theme();
        let hover = solid(style(&theme, button::Status::Hovered));
        let pressed = solid(style(&theme, button::Status::Pressed));

        assert_ne!(hover, pressed, "hover 与 pressed 必须是两档（8% / 10%）");
        let base = unlucky_design::elevation::MaterialLayer::Card.surface_role();
        assert_eq!(
            hover,
            ui.state_over(StateLayer::Hover, Role::OnSurface, base)
        );
        assert_eq!(
            pressed,
            ui.state_over(StateLayer::Pressed, Role::OnSurface, base)
        );
        assert!(
            style(&theme, button::Status::Active).background.is_none(),
            "静息态不叠状态层"
        );
    }

    #[test]
    fn the_theme_chip_draws_pressed_darker_than_hover() {
        let ui = ui();
        assert_two_layers(
            &ui,
            chip_style(
                &ui,
                false,
                ui.color(Role::OnSurfaceVariant),
                ui.color(Role::OutlineVariant),
                crate::theme::STROKE_PX,
                ui.radius(chipbox::CHIP.shape, chipbox::CHIP.height),
                None,
            ),
        );
    }

    /// **语言行那枚 chip**（`general::chip` 的 `chip_style`）同理：hover 8% / pressed 10%。
    ///
    /// 为什么这条断言住在 `theme.rs` 而不是语言行自己的文件：M285 起语言行在
    /// `general/language.rs`，它的 `chip_style` 声明成 `pub(super)`（`general` 子树可见），
    /// 所以这里不必把它抬成 crate 面。
    #[test]
    fn the_language_chip_draws_pressed_darker_than_hover() {
        use super::super::language::chip_style as language_chip_style;

        let ui = ui();
        assert_two_layers(
            &ui,
            language_chip_style(
                &ui,
                false,
                ui.color(Role::OnSurfaceVariant),
                ui.color(Role::OutlineVariant),
                crate::theme::STROKE_PX,
                ui.radius(chipbox::CHIP.shape, chipbox::CHIP.height),
                None,
            ),
        );
    }

    /// 选中档位的四条：静息 = 实色容器、hover / pressed 各一档、都逐位等于令牌求值（M299）。
    ///
    /// 抓得住：把选中支改回 `Some(ui.color(SecondaryContainer))`（= 选中就不响应）⇒ 全相等，前三条
    /// 红；hover 与 pressed 同 α ⇒ 第四条红；合成底换成卡底以外的角色 ⇒ 后两条红。
    fn assert_selected_layers(
        ui: &Ui,
        style: impl Fn(&IcedTheme, button::Status) -> button::Style,
    ) {
        let theme = ui.theme();
        let rest = solid(style(&theme, button::Status::Active));
        let hover = solid(style(&theme, button::Status::Hovered));
        let pressed = solid(style(&theme, button::Status::Pressed));

        assert_eq!(
            rest,
            ui.color(Role::SecondaryContainer),
            "选中静息 = 实色容器（不是满档状态层）"
        );
        assert_ne!(hover, rest, "选中 + hover 必须与静息不同");
        assert_ne!(pressed, rest, "选中 + pressed 必须与静息不同");
        assert_ne!(hover, pressed, "8% 与 10% 必须能区分");
        assert_eq!(
            hover,
            ui.state_over(
                StateLayer::Hover,
                Role::OnSecondaryContainer,
                Role::SecondaryContainer
            )
        );
        assert_eq!(
            pressed,
            ui.state_over(
                StateLayer::Pressed,
                Role::OnSecondaryContainer,
                Role::SecondaryContainer
            )
        );
    }

    /// **两个选中档位**（主题三档 / 语言三档）都跟态走，且走的是同一个入口。
    ///
    /// 抓不住：像素（`DevDocs/artifacts/m299/settings-general-*-hover|pressed.png`，两枚 chip 都在
    /// General 组）；也抓不住「自己抄了一份**正确**的叠层」——由 `selection/tests/single_entry.rs`
    /// 的接线扫描负责。
    #[test]
    fn a_selected_mode_chip_reacts_to_hover_and_press() {
        use super::super::language::chip_style as language_chip_style;

        let ui = ui();
        assert_selected_layers(
            &ui,
            chip_style(
                &ui,
                true,
                ui.color(Role::OnSecondaryContainer),
                ui.color(Role::OutlineVariant),
                crate::theme::STROKE_PX,
                ui.radius(chipbox::CHIP.shape, chipbox::CHIP.height),
                None,
            ),
        );
        assert_selected_layers(
            &ui,
            language_chip_style(
                &ui,
                true,
                ui.color(Role::OnSecondaryContainer),
                ui.color(Role::OutlineVariant),
                crate::theme::STROKE_PX,
                ui.radius(chipbox::CHIP.shape, chipbox::CHIP.height),
                None,
            ),
        );
    }
}
