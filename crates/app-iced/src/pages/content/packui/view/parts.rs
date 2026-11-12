//! 共用小件：带底色的容器、容忍域的标题、逐条列表。
//!
//! 三个都在两个以上地方用到，所以单独一个文件——不是为了行数好看，是因为
//! 「容忍域长什么样」只有一处定义，改颜色/图标才不用找第二个副本。

use iced::widget::{column, container, row};
use iced::{Alignment, Background, Border, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use unlucky_i18n::Catalog;

use crate::pages::frame;
use crate::pages::Message as PageMessage;
use crate::theme::Ui;

use super::super::model::Row;

/// 带底色的容器（探测块与致命块共用；颜色由调用方给）。
pub(super) fn marker_box<'a>(
    ui: &'a Ui,
    role: Role,
    content: impl Into<Element<'a, PageMessage>>,
) -> Element<'a, PageMessage> {
    let role = ui.color(role);
    container(content.into())
        .width(Length::Fill)
        .padding(ui.space(Space::X3))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(role)),
            border: Border {
                radius: ui.radius_dp(ShapeScale::Large),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

/// 容忍域的标题（`Warn` 图标 + 三级色）。
///
/// 收 `String` 而不是 `&str`：标题要带计数（`Not in the pack: 13`），而计数是运行时 `format!`
/// 出来的，借用不出去。
pub(super) fn tolerable_title(ui: &Ui, title: String) -> Element<'_, PageMessage> {
    row![
        frame::glyph(IconKind::Warn, ui.color(Role::Tertiary), frame::ICON_PX),
        frame::text_role_owned(
            ui,
            TypeRole::LabelLarge,
            true,
            title,
            ui.color(Role::Tertiary),
        ),
    ]
    .spacing(ui.space(Space::X2))
    .align_y(Alignment::Center)
    .into()
}

pub(super) fn rows_block<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    rows: &'a [Row],
) -> Element<'a, PageMessage> {
    let _ = catalog;
    column(
        rows.iter()
            .map(|row| {
                row![
                    frame::text_role(
                        ui,
                        TypeRole::BodySmall,
                        false,
                        row.path.as_str(),
                        ui.color(Role::OnSurface),
                    ),
                    frame::text_role(
                        ui,
                        TypeRole::BodySmall,
                        false,
                        row.note.as_str(),
                        ui.color(Role::OnSurfaceVariant),
                    ),
                ]
                .spacing(ui.space(Space::X2))
                .width(Length::Fill)
                .into()
            })
            .collect::<Vec<_>>(),
    )
    .spacing(ui.space(Space::X1))
    .width(Length::Fill)
    .into()
}

/// 逐条列表的**有界**容器：条目多时给一个固定高度的滚动区。
///
/// 为什么必须要有界：这一块长在内容库页正列的上方，而整页没有滚动（只有结果列表自己滚）⇒
/// 报告一长，「关闭 / 以包为准重新导入 / 确认写盘」这些动作就被顶出屏幕。出图时实测到了这一点
/// （冲突态与导出预检态的动作按钮落在 900px 以下）。
///
/// 少于 `rows_threshold` 条时直接铺开：给两行内容套一个 120px 的空框，比溢出更难看。
pub(super) fn bounded<'a>(
    ui: &'a Ui,
    content: impl Into<Element<'a, PageMessage>>,
    rows: usize,
    rows_threshold: usize,
    height: f32,
) -> Element<'a, PageMessage> {
    let content = content.into();
    if rows <= rows_threshold {
        return content;
    }
    iced::widget::scrollable(content)
        .height(Length::Fixed(height))
        .direction(iced::widget::scrollable::Direction::Vertical(
            iced::widget::scrollable::Scrollbar::new()
                .width(frame::SCROLLBAR_WIDTH)
                .scroller_width(frame::SCROLLBAR_WIDTH)
                .margin(frame::SCROLLBAR_MARGIN),
        ))
        .style(frame::scroll_style(ui))
        .into()
}
