//! 冲突清单那一屏的画法（M160）：**分页** + 三个入口 + 计数摘要 + 确认/取消。
//!
//! ## 版面
//!
//! ```text
//! ┌ 导入前：有 312 个文件已存在且内容不同 ───────────────┐
//! │ 保留 309 · 覆盖 3 · 覆盖将丢弃 2.4 MB                │
//! │ 没有改动的一律按「保留」：…                            │
//! │ [全部保留我的] [全部覆盖]                             │
//! ├ 第 1-40 项，共 312 项          [上一页] [下一页]      │
//! │ config/sodium-options.json  包内 overrides/ · 本来就在 │
//! │                       你的：4.1 KB · 包内：3.8 KB [覆盖]│
//! │ …（**只画当前这一页**）                                │
//! ├ [按这些选择导入]  [取消导入]                          │
//! └─────────────────────────────────────────────────────┘
//! ```
//!
//! ## 三处「不误导」
//!
//! 1. **缺省说明写在按钮下面**：用户不必猜"没点过的那 309 条会怎样"；
//! 2. **计数摘要里的丢弃量只算真选了覆盖的**：用最坏数字吓人不算如实；
//! 3. **按钮写的是"点了会变成什么"**（当前保留 ⇒ 按钮写「覆盖」），所以点一下就知道后果。
//!
//! ## 为什么分页而不是滚动
//!
//! 几千条冲突一次画完 = 几千个 widget：布局与滚动都卡。分页把 widget 数量钉在
//! [`super::super::model::PAGE_SIZE`] 条以内，翻页只是换数据。

use iced::widget::{column, container, row};
use iced::{Alignment, Background, Border, Element, Length, Padding};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use unlucky_i18n::Catalog;

use crate::app::components::buttons::{self, Button};
use crate::app::components::selection::{chip, Chip};
use crate::pages::frame;
use crate::pages::Message as PageMessage;
use crate::theme::Ui;

use super::super::model::{Pick, PickRow};
use super::super::PackMessage;
use super::intent;

/// 处置按钮槽的宽度（px）：取最宽状态「⟳ Overwrite」的实际占用再留余量，
/// 保证「Keep」与「Overwrite」两种状态的右边缘落在同一条线上。
const TOGGLE_SLOT: f32 = 132.0;

/// 整块。
pub(super) fn pick_block<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    pick: &'a Pick,
) -> Element<'a, PageMessage> {
    let mut block = column![
        frame::text_role(
            ui,
            TypeRole::TitleSmall,
            true,
            pick.title.as_str(),
            ui.color(Role::OnSurface),
        ),
        // 计数摘要：这是"我要丢掉多少"的唯一一处数字，用强调色
        frame::text_role(
            ui,
            TypeRole::BodyMedium,
            true,
            pick.summary.as_str(),
            ui.color(Role::Primary),
        ),
        frame::text_role(
            ui,
            TypeRole::BodySmall,
            false,
            pick.default_note,
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .spacing(ui.space(Space::X1))
    .width(Length::Fill);

    if let Some(note) = &pick.unknown_note {
        block = block.push(
            row![
                frame::glyph(IconKind::Warn, ui.color(Role::Tertiary), frame::ICON_PX),
                frame::text_role(
                    ui,
                    TypeRole::BodySmall,
                    false,
                    note.as_str(),
                    ui.color(Role::Tertiary),
                ),
            ]
            .spacing(ui.space(Space::X1))
            .align_y(Alignment::Center),
        );
    }

    // 三个入口：整批保留 / 整批覆盖（逐项就是下面每行那个按钮）
    block = block.push(
        row![
            chip(
                ui,
                Chip::filter(
                    pick.keep_all,
                    pick.keep_all_active,
                    intent(PackMessage::PickKeepAll),
                ),
            ),
            chip(
                ui,
                Chip::filter(
                    pick.overwrite_all,
                    pick.overwrite_all_active,
                    intent(PackMessage::PickOverwriteAll),
                ),
            ),
        ]
        .spacing(ui.space(Space::X2)),
    );

    // 分页头（当前页之外的行一律不画）
    block = block.push(bounded(
        column(
            std::iter::once(pager(ui, catalog, pick))
                .chain(pick.rows.iter().map(|item| pick_row(ui, item)))
                .collect::<Vec<_>>(),
        )
        .spacing(ui.space(Space::X1))
        .width(Length::Fill),
        375.0,
    ));

    // 列表与动作行之间留出与其它区块一致的呼吸（复审 r1 视觉 2）：滚到底时最后一行会紧贴
    // 动作行，靠这层 padding 把两者分开（直接调列 spacing 会把行间距也一起拉开）。
    block = block.push(
        container(
            row![
                buttons::filled(
                    ui,
                    Button::new(pick.confirm.as_str(), intent(PackMessage::ConfirmImport))
                        .leading(IconKind::Check),
                ),
                buttons::text(
                    ui,
                    Button::new(pick.cancel, intent(PackMessage::CancelImport)),
                ),
            ]
            .spacing(ui.space(Space::X2))
            .align_y(Alignment::Center),
        )
        .padding(Padding {
            top: ui.space(Space::X2),
            ..Padding::ZERO
        }),
    );

    container(block)
        .width(Length::Fill)
        .padding(ui.space(Space::X3))
        .style(move |_theme| container::Style {
            border: Border {
                color: ui.color(Role::Outline),
                width: 1.0,
                radius: ui.radius_dp(ShapeScale::Large),
            },
            background: Some(Background::Color(ui.color(Role::Surface))),
            ..container::Style::default()
        })
        .into()
}

/// 分页头：区间 + 上一页 / 下一页。
fn pager<'a>(ui: &'a Ui, _catalog: Catalog, pick: &'a Pick) -> Element<'a, PageMessage> {
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
fn pick_row<'a>(ui: &'a Ui, item: &'a PickRow) -> Element<'a, PageMessage> {
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

/// 有界容器：内容多时自己滚（与结果块的 `bounded` 同一条口径，但这里限高不同）。
fn bounded<'a>(
    content: impl Into<Element<'a, PageMessage>>,
    height: f32,
) -> Element<'a, PageMessage> {
    iced::widget::scrollable(container(content).width(Length::Fill))
        .height(Length::Fixed(height))
        .into()
}
