//! 按钮家族的状态矩阵（D30：浅/深 × 五态）。
//!
//! ## 矩阵形状
//!
//! ```text
//!                        | enabled | hovered | pressed | focused | disabled |
//!   filled               |  [btn]  |  [btn]  |  [btn]  |  [btn]  |  [btn]   |
//!   tonal                |   ...   （下同）
//!   elevated             |
//!   outlined             |
//!   text                 |
//!   icon·standard        |  [ico]  |   ...
//!   icon·filled          |
//!   icon·tonal           |
//! ```
//!
//! 40 个单元格共 5 列 × 8 行，一屏（1180×760）装不下，靠 `gallery/mod.rs` 的 scrollable 滚动；
//! 截图取的是**整窗**，所以首屏能看到按钮家族的全部 5 行（图标行可能需要滚动，报告里会说明）。
//!
//! ## 为什么每格都强制状态
//!
//! iced 的 `button::Status` 只由**真实鼠标**驱动，截图时没法让 40 个按钮同时 hover。
//! 组件层的 `Button::state(...)` 就是为这件事开的取证开关（`--force-state` 只能给整屏一种态）。
//! 强制值**只影响画什么**，不动任何状态机——所以矩阵图里的五态与运行期五态逐像素同源。

use iced::widget::{column, row};
use iced::{Element, Length};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::theme::Ui;

use super::super::buttons::{self, Button, IconVariant, Variant};
use super::super::interaction::Interaction;
use super::{cell_width, heading, row_label, state_cells, DemoMessage};

/// 按钮家族整段（标题 + 列头 + 5 行文字按钮 + 3 行图标按钮 + 一条图例说明）。
pub(super) fn section<'a>(ui: &'a Ui) -> Element<'a, DemoMessage> {
    let legend = ui.style(TypeRole::BodySmall);

    let mut rows = column![header_row(ui)].spacing(ui.space(Space::X2));

    for variant in Variant::ALL {
        let cells = Interaction::ALL
            .iter()
            .map(|state| cell(ui, text_button(ui, variant, *state)))
            .collect::<Vec<_>>();

        rows = rows.push(
            row![
                row_label(ui, variant.name()),
                row(cells).spacing(ui.space(Space::X2))
            ]
            .spacing(ui.space(Space::X2)),
        );
    }

    for variant in IconVariant::ALL {
        let cells = Interaction::ALL
            .iter()
            .map(|state| cell(ui, icon_button(ui, variant, *state)))
            .collect::<Vec<_>>();

        rows = rows.push(
            row![
                row_label(ui, variant.name()),
                row(cells).spacing(ui.space(Space::X2))
            ]
            .spacing(ui.space(Space::X2)),
        );
    }

    column![
        heading(
            ui,
            "buttons · filled / tonal / elevated / outlined / text + icon",
            "12b §1/§2：高 40 · 圆角 full(20) · 内边距 24（带图标 16/24）· 图标 18 间距 8 · \
             hover 8% / focus 10% / pressed 10% / disabled 容器 10% 内容 38%"
        ),
        rows,
        iced::widget::text(
            "图例：列 = 五态（focus 与 pressed 同 α 10%，看起来一样是规范要求）；\
             行 = 变体。虚线不表示任何东西，只是矩阵栅格。"
        )
        .size(legend.size_px)
        .font(ui.font_of(legend))
        .color(ui.color(Role::OnSurfaceVariant)),
    ]
    .spacing(ui.space(Space::X2))
    .into()
}

/// 列头（五态名）。
fn header_row<'a>(ui: &'a Ui) -> Element<'a, DemoMessage> {
    let style = ui.style(TypeRole::LabelSmall);
    let mut head = row![row_label(ui, "")].spacing(ui.space(Space::X2));

    for (name, width) in state_cells(ui) {
        head = head.push(
            iced::widget::container(
                iced::widget::text(name)
                    .size(style.size_px)
                    .font(ui.font_of(style))
                    .color(ui.color(Role::OnSurfaceVariant)),
            )
            .width(Length::Fixed(width)),
        );
    }

    head.into()
}

/// 一个矩阵单元格（固定宽，内容左对齐；高度只由按钮自己决定）。
fn cell<'a>(ui: &'a Ui, content: Element<'a, DemoMessage>) -> Element<'a, DemoMessage> {
    iced::widget::container(content)
        .width(Length::Fixed(cell_width(ui)))
        .into()
}

/// 文字按钮的一格（强制成 `state`）。
fn text_button<'a>(ui: &'a Ui, variant: Variant, state: Interaction) -> Element<'a, DemoMessage> {
    // 标签用变体名 + 首字母大写：图上要能一眼对上左栏，且不引入 i18n 依赖（gallery 不是产品页）
    let label = match variant {
        Variant::Filled => "Filled",
        Variant::Tonal => "Tonal",
        Variant::Elevated => "Elevated",
        Variant::Outlined => "Outlined",
        Variant::Text => "Text",
    };

    // 五态共用**同一套**装配，只有「有没有消息」随禁用变：
    // 早先禁用格单独走 `Button::disabled(label)`，把前后图标一起丢了，于是 disabled 列看起来
    // 像另一个组件（视觉 reviewer P2-3）。
    let mut spec = if state.is_disabled() {
        Button::disabled(label)
    } else {
        Button::new(label, DemoMessage::Pressed(label))
    }
    .state(state);
    if variant != Variant::Text {
        // 两个样例都必须是**描边**字形：`Play` 是填充字形，和 `ChevronRight` 混在一颗按钮里
        // 会显得线宽不一致（视觉 reviewer P2-3）。主操作允许实心，但只能单独出现。
        spec = spec
            .leading(IconKind::Download)
            .trailing(IconKind::ChevronRight);
    }

    match variant {
        Variant::Filled => buttons::filled(ui, spec),
        Variant::Tonal => buttons::tonal(ui, spec),
        Variant::Elevated => buttons::elevated(ui, spec),
        Variant::Outlined => buttons::outlined(ui, spec),
        Variant::Text => buttons::text(ui, spec),
    }
}

/// 图标按钮的一格（强制成 `state`）。
fn icon_button<'a>(
    ui: &'a Ui,
    variant: IconVariant,
    state: Interaction,
) -> Element<'a, DemoMessage> {
    let press = if state.is_disabled() {
        None
    } else {
        Some(DemoMessage::Pressed(variant.name()))
    };

    buttons::icon(ui, variant, IconKind::Settings, press, Some(state))
}
