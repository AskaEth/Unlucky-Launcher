//! 设置页 General 组的**语言行**（M285 `LANGSET`：从 `general.rs` 纯搬移）。
//!
//! 搬出来的理由只有行数：`general.rs` 390 行、撞 400 行硬线（`components/gate.rs`），
//! 而 M285 要给这一行的三枚互斥按钮接 `forced: Option<Interaction>`（`--force-state` 直通）。
//! 切法与 [`super::theme`] 完全同构——**一个行控件一个文件**：正文 [`view`] + 它的 [`chip`]
//! + [`chip_style`]（样式闭包抽成自由函数，单测才能直接喂 `Hovered` / `Pressed` 两态）。
//!
//! 搬移是逐字搬的：三个函数的语句、注释一个字没动。

use iced::border::Radius;
use iced::widget::{button, column, container, row};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Padding, Shadow, Theme as IcedTheme,
};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::catalog::Key;
use unlucky_i18n::LangChoice;

use crate::app::components::interaction::{self, Interaction};
use crate::pages::{frame, Language, Message};
use crate::theme::Ui;

/// 语言行：标题 + 生效语言回显 + 三档互斥按钮 + 一句说明。
pub(super) fn view<'a>(
    ui: &'a Ui,
    language: Language,
    forced: Option<Interaction>,
) -> Element<'a, Message> {
    let catalog = language.effective.catalog();
    let effective = LangChoice::Fixed(language.effective).label_key();

    let head = row![
        container(frame::text_role(
            ui,
            TypeRole::BodyMedium,
            false,
            catalog.text(Key::SettingsLanguage),
            ui.color(Role::OnSurface),
        ))
        .width(Length::Fill)
        .align_y(Alignment::Center),
        frame::text_role_owned(
            ui,
            TypeRole::LabelLarge,
            true,
            catalog.render(
                Key::SettingsLanguageEffective,
                &[("language", catalog.text(effective))],
            ),
            ui.color(super::VALUE_ECHO),
        ),
    ]
    .width(Length::Fill)
    .align_y(Alignment::Center);

    let chips = row(LangChoice::ALL
        .iter()
        .map(|choice| {
            chip(
                ui,
                catalog.text(choice.label_key()),
                *choice == language.choice,
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
            catalog.text(Key::SettingsLanguageHint),
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill)
    .padding(Padding::from([ui.space(Space::X2), 0.0]))
    .into()
}

/// 一个互斥按钮（选中态 `SecondaryContainer`，未选中透明 + `OnSurfaceVariant`）。
///
/// 与 `pages/appearance/section/mod.rs` 的 `choice` 同款几何，但文案来自 catalog（那份是英文基线）。
///
/// **几何（M229）**：取规格卡的 chip（高 32、圆角 8、内距 16、未选中 1px `OutlineVariant`），
/// 与同屏的主题三档 chips 完全同源——两行都是「并排的互斥筛选」，长得不一样会让读者以为是
/// 两种控件（M229 第一版就是这样：语言 40 高药丸、主题 32 高方角，出图一眼就不齐）。
fn chip<'a>(
    ui: &'a Ui,
    text: &'a str,
    selected: bool,
    choice: LangChoice,
    forced: Option<Interaction>,
) -> Element<'a, Message> {
    let box_ = unlucky_design::inkcat::metrics::CHIP;
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
    .padding(Padding::from([
        0.0,
        unlucky_design::inkcat::metrics::CHIP_PAD_X,
    ]))
    .style(chip_style(
        ui,
        selected,
        content_color,
        border_color,
        stroke,
        radius,
        forced,
    ))
    .on_press(Message::Language(choice))
    .into()
}

/// [`chip`] 的样式闭包（抽成自由函数：单测要**直接**喂 `Hovered` / `Pressed` 两态）。
///
/// `forced`（M285）是 `--force-state` 的落点，交给 `settings::resolve_state`（全仓唯一的 resolve 点）；
/// `None` 时行为与 M285 之前逐字节相同。`pub(super)` 是因为 `general/theme.rs` 的单测要直接喂它。
///
/// - **α 只从 [`Interaction`] 来**：hover 8% / pressed 10%（M3）。M284 之前两种态合并成一条
///   `matches!(status, Hovered | Pressed)` 分支、都叠 `StateLayer::Hover`，按下于是被压成 8%；
/// - **合成底是 [`frame::BASE`]**（M237：这一组住在 `frame::card` 里，状态层叠卡底而不是画布）。
pub(super) fn chip_style<'a>(
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
