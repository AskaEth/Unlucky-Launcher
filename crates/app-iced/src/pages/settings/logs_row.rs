//! 设置页「General」组的**日志文件夹**一行（M188）。
//!
//! 用户投诉原话：「我们的启动日志放在哪？…他找半天没找到崩溃日志」「只有一个安装日志」。
//! 这一行是对那句投诉的直接回答，三件事缺一不可：
//!
//! 1. **路径永远显示**（不是只在打开成功时才显示）：`<数据根>/logs`，三模式解析出来的真值；
//! 2. **两个入口**：「打开文件夹」走平台打开器（`app::dialog::export::open_dir`，与崩溃弹窗同一个，
//!    不另写一套 `cfg`），「复制路径」把路径送进剪贴板——打开失败时用户至少能照着路径自己去找；
//! 3. **说清里面有什么**：`launcher.log`（滚动上限写进文案）与 `logs/game/` 的会话 jsonl，
//!    以及旧目录「仍能读、没搬动」——否则用户换了版本会以为旧日志丢了。
//!
//! 为什么单独一个文件：`pages/settings/general.rs` 贴着 400 行硬线（语言行 + 字体行 + 三行只读），
//! 这一行自带按钮与样式，塞回去必破线。

use iced::border::Radius;
use iced::widget::{button, column, container, row as row_widget};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Padding, Shadow, Theme as IcedTheme,
};

use unlucky_design::color::Role;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::catalog::{Catalog, Key};

use crate::app::components::interaction::{self, Interaction};
use crate::pages::{frame, Action, Message};
use crate::theme::Ui;

/// 这一行（竖排：标签 / 路径 / 两个按钮 / 一句说明）。
pub(super) fn folder_row<'a>(
    ui: &'a Ui,
    catalog: &Catalog,
    forced: Option<Interaction>,
) -> Element<'a, Message> {
    let path = crate::app::logs_folder::logs_dir()
        .map(|dir| dir.display().to_string())
        .unwrap_or_else(|| catalog.text(Key::SettingsLogsUnknown).to_string());

    let hint = catalog.text(Key::SettingsLogsFolderHint);
    let actions = row_widget![
        action_button(
            ui,
            catalog.text(Key::LogsOpenFolder),
            Message::Action(Action::OpenLogs),
            forced,
        ),
        action_button(
            ui,
            catalog.text(Key::SettingsLogsCopy),
            Message::CopySummary(path.clone()),
            forced,
        ),
    ]
    .spacing(ui.space(Space::X2))
    .align_y(Alignment::Center);

    column![
        frame_text(
            ui,
            TypeRole::BodyMedium,
            catalog.text(Key::SettingsLogsFolder),
            ui.color(Role::OnSurface),
        ),
        container(frame_text_owned(
            ui,
            TypeRole::BodySmall,
            path,
            ui.color(Role::OnSurfaceVariant),
        ))
        .width(Length::Fill),
        actions,
        frame_text(
            ui,
            TypeRole::BodySmall,
            hint,
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill)
    .padding(Padding::from([ui.space(Space::X2), 0.0]))
    .into()
}

fn frame_text<'a>(
    ui: &'a Ui,
    role: TypeRole,
    text: &'a str,
    color: iced::Color,
) -> Element<'a, Message> {
    frame::text_role(ui, role, false, text, color)
}

fn frame_text_owned<'a>(
    ui: &'a Ui,
    role: TypeRole,
    text: String,
    color: iced::Color,
) -> Element<'a, Message> {
    frame::text_role_owned(ui, role, false, text, color)
}

/// 描边小按钮（与设置页其他「次级动作」同款：透明底、hover 出状态层、全圆角）。
fn action_button<'a>(
    ui: &'a Ui,
    label: &'a str,
    message: Message,
    forced: Option<Interaction>,
) -> Element<'a, Message> {
    let height = ui.metrics().control_height;
    let content = ui.color(Role::OnSurface);
    let outline = ui.color(Role::OutlineVariant);
    let radius = ui.radius(ShapeScale::Full, height);
    button(
        container(frame_text(ui, TypeRole::LabelLarge, label, content)).align_y(Alignment::Center),
    )
    .height(height)
    .padding(Padding::from([0.0, ui.space(Space::X3)]))
    .style(action_style(ui, content, outline, radius, forced))
    .on_press(message)
    .into()
}

/// [`action_button`] 的样式闭包（抽成自由函数：单测要**直接**喂 `Hovered` / `Pressed` 两态）。
///
/// - **α 只从 [`Interaction`] 来**：hover 8% / pressed 10%（M3）。M284 之前两种态合并成一条
///   `matches!(status, Hovered | Pressed)` 分支、都叠 `StateLayer::Hover`，按下于是被压成 8%；
/// - **合成底是 [`frame::BASE`]**（M237：这一行住在设置卡里，状态层叠卡底而不是画布）。
fn action_style<'a>(
    ui: &'a Ui,
    content: Color,
    outline: Color,
    radius: Radius,
    forced: Option<Interaction>,
) -> impl Fn(&IcedTheme, button::Status) -> button::Style + 'a {
    move |_theme, status| {
        let background = interaction::layer_color(
            ui,
            crate::pages::settings::resolve_state(forced, status),
            Role::OnSurface,
            frame::BASE.role(),
        )
        .map(Background::Color);

        button::Style {
            background,
            text_color: content,
            border: Border {
                color: outline,
                width: 1.0,
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
            None => panic!("hover/pressed 必须叠状态层"),
            Some(other) => panic!("背景必须是实色，得到 {other:?}"),
        }
    }

    /// hover(8%) 与 pressed(10%) 是**两档**，且各自逐值等于令牌表（把 10% 改成 11% 也会红）。
    ///
    /// 抓得住：①回到 `matches!(status, Hovered | Pressed)` 合并分支（两色相等 ⇒ 第一条红）；
    /// ②pressed 用 hover 的 α、或合成底被换回画布（第二、三条红）。
    /// 抓不住：观感；**过渡中间帧**（forced 路径不套 `tracked`，`layer_color` 直落端点值）。
    ///
    /// 底取 `MaterialLayer::Card.surface_role()`（`frame::BASE` 就是它），不重复写字面量。
    #[test]
    fn the_logs_action_button_draws_pressed_darker_than_hover() {
        let ui = ui();
        let style = action_style(
            &ui,
            ui.color(Role::OnSurface),
            ui.color(Role::OutlineVariant),
            ui.radius(ShapeScale::Full, ui.metrics().control_height),
            None,
        );
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
}
