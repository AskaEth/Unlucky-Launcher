//! 共用小件：带底色的容器、容忍域的标题、逐条列表。
//!
//! 三个都在两个以上地方用到，所以单独一个文件——不是为了行数好看，是因为
//! 「容忍域长什么样」只有一处定义，改颜色/图标才不用找第二个副本。

use iced::widget::text::{IntoFragment, LineHeight};
use iced::widget::{column, container, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Pixels};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::{TypeRole, TypeStyle};

use unlucky_i18n::Catalog;
use unlucky_presenter::dto::ImportIssueLevelView;

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

/// 定高文字：`line = Some(q)` ⇒ 行框（并因此是**行距**）= 行量子 `q`；`None` ⇒ 排版令牌的行框
/// （与 [`frame::text_role`] 逐字等价：同 size / 同 line_height / 同字体 / 同颜色）。
///
/// 为什么需要 `Some` 这一档（M350）：令牌给的行框有**两档**（`BodySmall` 16 / `LabelLarge` 20），
/// 再加上 4 的行距、以及 24 高的 `Warn` 图标把段头行顶到 24，一段注记里就有 16 / 20 / 24 三种
/// 竖向量——**固定高度的视口底边会落进某一个行框内部**（那是「切字」的真因，与墨迹高矮无关：
/// 墨迹在两种行框里都装得下，实测 16 的框里占 `+2..+14`、20 的框里占 `+5..+17`）。
/// 把每一行都拉到同一个量子上，视口再取整数个量子，底边就恒落在行框线上，见 `view/pick.rs`
/// 的 `notes_cap`。
fn line_text<'a, C>(
    ui: &'a Ui,
    role: TypeRole,
    emphasized: bool,
    content: C,
    color: Color,
    line: Option<f32>,
) -> iced::widget::Text<'a>
where
    C: IntoFragment<'a>,
{
    let style: TypeStyle = if emphasized {
        ui.emphasized(role)
    } else {
        ui.style(role)
    };

    text(content)
        .size(style.size_px)
        .line_height(LineHeight::Absolute(Pixels(
            line.unwrap_or(style.line_height_px),
        )))
        .font(ui.font_of(style))
        .color(color)
}

/// 容忍域的标题（`Warn` 图标 + 三级色）。
///
/// 收 `String` 而不是 `&str`：标题要带计数（`Not in the pack: 13`），而计数是运行时 `format!`
/// 出来的，借用不出去。
///
/// `line = Some(q)` 时**字形也收进量子**（M350）：预检屏的注记段头必须正好一行量子高。行高是
/// `max(图标, 文字)`，而令牌的 `ICON_PX` 是 24 > `LabelLarge` 的 20 ⇒ 段头行框会变成 24（不是 20），
/// 每遇到一个段头就把整段栅格推歪 4px——那正是 M345 那版折线落进行框内部的原因之一。
/// `None` 用 `ICON_PX`（报告/导出那几屏原样）。
pub(super) fn tolerable_title(
    ui: &Ui,
    title: String,
    line: Option<f32>,
) -> Element<'_, PageMessage> {
    row![
        frame::glyph(
            IconKind::Warn,
            ui.color(Role::Tertiary),
            line.unwrap_or(frame::ICON_PX)
        ),
        line_text(
            ui,
            TypeRole::LabelLarge,
            true,
            title,
            ui.color(Role::Tertiary),
            line
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

/// 一档注记段：段标题 + 逐条原文（**导入报告与预检屏共用**，M341）。
///
/// 为什么共用：同一批 core 注记（`ImportReportView::warnings` / `ImportScanView::notes`）要在两屏
/// 各说一遍。画法一分开，用户就会看到「报告说要处理、预检说只是告知」这类自相矛盾。分档在模型层
/// 已经只有一处（`model::report_issue::section`），画法同样收成这一处。
///
/// 两档的视觉通道照报告那一屏（M339）：**告警**借容忍域那套段头（`Warn` 图标 + 三级色），
/// **注记**只用一行次级色标题——「有没有琥珀三角」本身就是「要不要动手」那条通道。
///
/// 有界与否**不在这里决定**：预检屏把两段合成一个 [`bounded`] 区再放上屏（两段各自封顶的话，
/// 两个上限相加照样能把动作行顶出屏幕）。
///
/// `line = Some(q)`（M350，只有预检屏用）：段头与每条正文都按同一个行量子 `q` 定高、段内行距
/// **归 0**（行距已经含在 `q` 里）⇒ 整段的每一行都落在 `q` 的整数倍上，注记视口才敢按行量子
/// 取整（推导见 `view/pick.rs` 的 `notes_cap`）。`None` = 令牌行框 + `Space::X1` 行距（报告 /
/// 导出的那几段原样不动）。
pub(super) fn issue_section<'a>(
    ui: &'a Ui,
    level: ImportIssueLevelView,
    title: String,
    lines: &'a [String],
    line: Option<f32>,
) -> Element<'a, PageMessage> {
    let body = match level {
        ImportIssueLevelView::Warning => Role::OnSurface,
        ImportIssueLevelView::Note => Role::OnSurfaceVariant,
    };
    let heading: Element<'a, PageMessage> = match level {
        ImportIssueLevelView::Warning => tolerable_title(ui, title, line),
        ImportIssueLevelView::Note => {
            line_text(ui, TypeRole::LabelLarge, true, title, ui.color(body), line).into()
        }
    };
    let gap = match line {
        // 行距已经含在量子（= 行框高）里，再加 X1 就把栅格推歪 4px
        Some(_) => 0.0,
        None => ui.space(Space::X1),
    };
    let mut block = column![heading].spacing(gap).width(Length::Fill);
    for text_line in lines {
        block = block.push(line_text(
            ui,
            TypeRole::BodySmall,
            false,
            text_line.as_str(),
            ui.color(body),
            line,
        ));
    }
    block.into()
}
