//! 冲突清单的**逐条行**与分页头（M160；M350 从 `view/pick.rs` 纯搬移拆出，零行为变化）。
//!
//! 拆出去的理由只有一条：`view/pick.rs` 当时 **399 行（零余量）**，而 M350 要给注记区做行级几何，
//! 必须先腾出空间。搬移是纯搬移——`TOGGLE_SLOT` / [`pager`] / [`pick_row`] 三块的代码逐字未改，
//! 复审用 diff 就能证明这一屏的像素不受影响。

use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length, Padding};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use unlucky_i18n::Catalog;

use crate::app::components::buttons::{self, Button};
use crate::pages::frame;
use crate::pages::Message as PageMessage;
use crate::theme::Ui;

use super::super::model::{Pick, PickRow};
use super::super::PackMessage;
use super::intent;

/// 处置按钮槽的宽度（px）：取最宽状态「⟳ Overwrite」的实际占用再留余量，
/// 保证「Keep」与「Overwrite」两种状态的右边缘落在同一条线上。
const TOGGLE_SLOT: f32 = 132.0;

/// 分页头：区间 + 上一页 / 下一页。
pub(super) fn pager<'a>(ui: &'a Ui, _catalog: Catalog, pick: &'a Pick) -> Element<'a, PageMessage> {
    let previous = if pick.can_prev {
        buttons::text(
            ui,
            Button::new(
                pick.prev,
                intent(PackMessage::PickPage(pick.page.saturating_sub(1))),
            )
            .leading(IconKind::ChevronDown),
        )
    } else {
        // 到边了就把按钮指回当前页（点了不动的死按钮比"没有按钮"更让人困惑）
        buttons::text(
            ui,
            Button::new(pick.prev, intent(PackMessage::PickPage(pick.page))),
        )
    };
    let next = if pick.can_next {
        buttons::text(
            ui,
            Button::new(pick.next, intent(PackMessage::PickPage(pick.page + 1)))
                .leading(IconKind::ChevronDown),
        )
    } else {
        buttons::text(
            ui,
            Button::new(pick.next, intent(PackMessage::PickPage(pick.page))),
        )
    };
    // 分页行与行内控件共用同一条右锚点（复审 r1 视觉 3）：区间文案靠左，两个按钮用
    // `horizontal_space` 顶到右边，于是「Previous · Next」的右边缘与下面每一行的处置槽对齐。
    // 两个按钮进一个**右对齐的槽**，槽的右边缘就是行的右边缘 —— 于是它们的右端与下面每行的
    // 处置槽落在同一条线上（按钮自身的内边距也一起对齐，不会差那十几像素）。
    let buttons_slot = container(
        row![previous, next]
            .spacing(ui.space(Space::X2))
            .align_y(Alignment::Center),
    )
    .width(Length::Fixed(TOGGLE_SLOT * 2.0))
    .align_x(Alignment::End);
    row![
        frame::text_role(
            ui,
            TypeRole::LabelMedium,
            false,
            pick.range.as_str(),
            ui.color(Role::OnSurfaceVariant),
        ),
        iced::widget::Space::new().width(Length::Fill),
        buttons_slot,
    ]
    .spacing(ui.space(Space::X2))
    .align_y(Alignment::Center)
    .width(Length::Fill)
    .into()
}

/// 一条冲突：**第一行**路径 + 处置按钮，**第二行**来源 · 成因 · 两侧体积。
///
/// 为什么两行：单行里塞四个字段（路径 / 来源 / 体积 / 按钮）会把路径压成一条窄柱，
/// 出图实测每行折成四五段——扫读成本比两行高得多。
pub(super) fn pick_row<'a>(ui: &'a Ui, item: &'a PickRow) -> Element<'a, PageMessage> {
    // 两种取值都是**同一种几何**（等高的 pill），只是填充不同：
    //   · 当前=覆盖 ⇒ **tonal 填充**（这一行会被盖掉，是"待反悔"的那一档）
    //   · 当前=保留 ⇒ **描边**（缺省档；点了才会覆盖）
    // 两种 pill 再各自右对齐进同宽度的槽，于是**右边缘落在同一条线**（复审 r1 视觉 1 的必修项）——
    // 之前一边是描边 pill、一边是无容器的文字按钮，宽窄与右端都不一致。
    let toggle = if item.overwrites() {
        buttons::tonal(
            ui,
            Button::new(
                item.toggle,
                intent(PackMessage::TogglePick(item.path.clone())),
            )
            .leading(IconKind::Refresh),
        )
    } else {
        buttons::outlined(
            ui,
            Button::new(
                item.toggle,
                intent(PackMessage::TogglePick(item.path.clone())),
            )
            .leading(IconKind::Refresh),
        )
    };
    // 路径那半列必须自己吃掉剩余宽度（`Length::Fill`），否则处置槽会紧跟文字、离右边缘一大截，
    // 与分页行的右锚点对不上（出图实测：行内按钮停在 x≈680、分页按钮在 x≈780）。
    let path_slot = container(frame::text_role(
        ui,
        TypeRole::BodySmall,
        false,
        item.path.as_str(),
        ui.color(Role::OnSurface),
    ))
    .width(Length::Fill);
    // 处置按钮自己那一格：定宽 + 右对齐 ⇒ 两种取值的 pill 右边缘落在同一条线上。
    // 右侧再留 8dp：滚动条就贴在内容右边缘（出图实测 x≈791），pill 紧贴着它显得挤。
    let toggle_slot = container(toggle)
        .width(Length::Fixed(TOGGLE_SLOT))
        .align_x(Alignment::End)
        .padding(Padding {
            right: ui.space(Space::X2),
            ..Padding::ZERO
        });
    let head = row![path_slot, toggle_slot]
        .spacing(ui.space(Space::X2))
        .align_y(Alignment::Center)
        .width(Length::Fill);

    let detail = frame::text_role_owned(
        ui,
        TypeRole::BodySmall,
        false,
        format!("{} · {} · {}", item.source, item.reason, item.sizes),
        ui.color(Role::OnSurfaceVariant),
    );

    column![head, detail]
        .spacing(ui.space(Space::X1))
        .width(Length::Fill)
        .into()
}
