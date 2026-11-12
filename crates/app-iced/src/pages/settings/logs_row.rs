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

use iced::widget::{button, column, container, row as row_widget};
use iced::{Alignment, Background, Border, Element, Length, Padding, Shadow, Theme as IcedTheme};

use unlucky_design::color::{Role, StateLayer};
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::catalog::{Catalog, Key};

use crate::pages::{frame, Action, Message};
use crate::theme::Ui;

/// 这一行（竖排：标签 / 路径 / 两个按钮 / 一句说明）。
pub(super) fn folder_row<'a>(ui: &'a Ui, catalog: &Catalog) -> Element<'a, Message> {
    let path = crate::app::logs_folder::logs_dir()
        .map(|dir| dir.display().to_string())
        .unwrap_or_else(|| catalog.text(Key::SettingsLogsUnknown).to_string());

    let hint = catalog.text(Key::SettingsLogsFolderHint);
    let actions = row_widget![
        action_button(
            ui,
            catalog.text(Key::LogsOpenFolder),
            Message::Action(Action::OpenLogs),
        ),
        action_button(
            ui,
            catalog.text(Key::SettingsLogsCopy),
            Message::CopySummary(path.clone()),
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
fn action_button<'a>(ui: &'a Ui, label: &'a str, message: Message) -> Element<'a, Message> {
    let height = ui.metrics().control_height;
    let content = ui.color(Role::OnSurface);
    let outline = ui.color(Role::OutlineVariant);
    button(
        container(frame_text(ui, TypeRole::LabelLarge, label, content)).align_y(Alignment::Center),
    )
    .height(height)
    .padding(Padding::from([0.0, ui.space(Space::X3)]))
    .style(move |_theme: &IcedTheme, status| {
        let background =
            matches!(status, button::Status::Hovered | button::Status::Pressed).then(|| {
                // M237：这一行住在设置卡里（`frame::card`），hover 状态层叠卡底。
                Background::Color(ui.state_over(
                    StateLayer::Hover,
                    Role::OnSurface,
                    frame::BASE.role(),
                ))
            });
        button::Style {
            background,
            text_color: content,
            border: Border {
                color: outline,
                width: 1.0,
                radius: ui.radius(ShapeScale::Full, height),
            },
            shadow: Shadow::default(),
            snap: true,
        }
    })
    .on_press(message)
    .into()
}
