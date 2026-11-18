//! 尺寸档矩阵：**按钮 XS~XL 五档** + **FAB 四态**（规格卡 `BUTTONS / XS - XL` 与
//! `FAB / SMALL - DEFAULT - MEDIUM - EXTENDED`）。
//!
//! ## 为什么单开一段（而不是塞进 `buttons`）
//!
//! `buttons` 段是「五式 × 五态 + 图标四式」的 40 格矩阵，一屏（1180×760）本来就贴着折叠线；
//! 五档梯子（最高 136）与四态 FAB（最宽 160）塞进去会被压到屏幕外——那样出的图**看不见**，
//! 等于没拍。所以按「一段 = 一个能一屏判完的问题」拆开：
//!
//! ```text
//! ULL_GALLERY_SECTION=sizes   # 本段：尺寸梯子 + FAB 四态
//! ```
//!
//! ## 图上要能判什么
//!
//! 1. 五档高度递增、圆角全部是**半高**（药丸），标签字号跟着档位升（14/14/16/24/32）；
//! 2. FAB 四态：40/56/80 三个方态 + 160×56 扩展态（图标 + 标签）；
//! 3. 配色：FAB 一律 `PrimaryContainer` / `OnPrimaryContainer`（规格卡上四颗都是淡绿底深绿字）。
//!
//! 判不了：状态层（FAB 在这一段是静息态，五态矩阵在 `buttons` 段的按钮上验）、以及动效。

use iced::widget::{column, row};
use iced::{Element, Length};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::inkcat::size::{ControlSize, FabSize};
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::theme::Ui;

use super::super::buttons::{self, fab, Button, Fab};
use super::super::interaction::Interaction;
use super::{heading, DemoMessage};

/// 整段（标题 + 五档梯子 + 四态 FAB + 图例）。
pub(super) fn section<'a>(ui: &'a Ui) -> Element<'a, DemoMessage> {
    let legend = ui.style(TypeRole::BodySmall);

    let ladder = row(ControlSize::ALL
        .iter()
        .map(|size| sized(ui, *size))
        .collect::<Vec<_>>())
    .spacing(ui.space(Space::X4))
    .align_y(iced::Alignment::Center);

    let fabs = row(FabSize::ALL
        .iter()
        .map(|size| fab_cell(ui, *size))
        .collect::<Vec<_>>())
    .spacing(ui.space(Space::X4))
    .align_y(iced::Alignment::Center);

    column![
        heading(
            ui,
            "sizes · buttons xs~xl + fab small~extended",
            "规格卡：按钮 32/40/56/96/136（圆角 = 半高，标签 14/14/16/24/32 字重 500）；\
             FAB 40/r12 · 56/r16 · 80/r20 · 160×56/r16（图标 20/26/34/24，扩展态内距 28、间距 8）"
        ),
        labelled(ui, "buttons xs - xl", ladder.into()),
        labelled(ui, "fab small - extended", fabs.into()),
        iced::widget::text(
            "图例：产品页的按钮只用 S 档（40，= M3 control_height）；M/L/XL 留给空态主行动与展示型入口。\
             FAB 四颗都是静息态，配色 PrimaryContainer / OnPrimaryContainer。",
        )
        .size(legend.size_px)
        .font(ui.font_of(legend))
        .color(ui.color(Role::OnSurfaceVariant)),
    ]
    .spacing(ui.space(Space::X2))
    .into()
}

/// 一档 Filled 按钮（标签是档位名的大写：XS / S / M / L / XL。
///
/// **必须大写**：小写的 `l` 在 Inter/Segoe 里就是一根竖线，XL 会读成「x|」——第一版出图就是这么错的。
fn sized<'a>(ui: &'a Ui, size: ControlSize) -> Element<'a, DemoMessage> {
    let label: &'static str = match size {
        ControlSize::Xs => "XS",
        ControlSize::S => "S",
        ControlSize::M => "M",
        ControlSize::L => "L",
        ControlSize::Xl => "XL",
    };
    buttons::filled(
        ui,
        Button::new(label, DemoMessage::Pressed("size")).size(size),
    )
}

/// 一颗 FAB（扩展态带标签；静息态强制，避免光标位置影响出图）。
fn fab_cell<'a>(ui: &'a Ui, size: FabSize) -> Element<'a, DemoMessage> {
    let spec = if matches!(size, FabSize::Extended) {
        Fab::extended(IconKind::Plus, "New", DemoMessage::Pressed("fab"))
    } else {
        Fab::new(IconKind::Plus, DemoMessage::Pressed("fab"))
    }
    .size(size)
    .state(Interaction::Enabled);
    fab(ui, spec)
}

/// 一行「小标题 + 内容」。
fn labelled<'a>(
    ui: &'a Ui,
    label: &'a str,
    content: Element<'a, DemoMessage>,
) -> Element<'a, DemoMessage> {
    let style = ui.style(TypeRole::LabelSmall);
    column![
        iced::widget::text(label)
            .size(style.size_px)
            .font(ui.font_of(style))
            .color(ui.color(Role::OnSurfaceVariant)),
        iced::widget::container(content)
            .width(Length::Fill)
            .padding(iced::Padding::from([ui.space(Space::X2), 0.0])),
    ]
    .spacing(ui.space(Space::X1))
    .into()
}
