//! 禁用态的**合成底**（M234）：同一批禁用控件摆在三种底上，一眼看出「叠的是哪一层」。
//!
//! ## 这张图回答什么
//!
//! M223 记的「CR 1.05、形丢失」是这么来的：`disabled.rs` 的入口收一个 `Role` 当合成底，
//! 而**所有调用点都传死 `Role::Surface`**——卡片（`SurfaceContainer`）与面板
//! （`SurfaceContainerLow`）里的禁用控件于是把 10%（按钮族）/ 12%（面状控件族）那一层叠在
//! **画布色**上，与它真正的底差一层。
//!
//! 这里把三列**并排**摆出来，每列一块板、板底就是控件真踩的那层底：
//!
//! ```text
//!   canvas    Surface              ← 页面底色
//!   card      SurfaceContainer     ← `surfaces::elevated::card` 的底（`elevated::BASE`）
//!   panel     SurfaceContainerLow  ← Card 层（取景板 / 面板）
//! ```
//!
//! 每列都是**同一族控件**：Filled/Outlined 禁用按钮、Filled 禁用字段、禁用开关与 chip、
//! 禁用搜索条、禁用列表项、禁用滑块、禁用 tab。控件拿的都是这一列的 [`Base`]，
//! 所以列与列之间的差别只有「底」。
//!
//! ## 判据（数值在 `probe/bases.rs`）
//!
//! 像素判据判的是**精确值**：卡列里的禁用按钮容器必须等于 `on-surface@10% 叠卡片底`，
//! 且**不等于**画布底那一档。注意量级：Ink/Silver 板上 `Surface` 与 `SurfaceContainer`
//! 只差个位数 RGB，10% 之后差 **≈1/255**——肉眼看不出来，所以这张图的用处是**取证**，
//! 不是「看出区别」。真判据在 `interaction::disabled::tests`（逐值）与 `probe/bases.rs`（像素）。
//!
//! ```text
//! ULL_GALLERY_SECTION=disabledbase $EXE --software-renderer --screen=gallery \
//!   --screenshot out.png --screenshot-theme=light
//! ULL_GALLERY_SECTION=disabledbase ULL_GALLERY_SHOT=out.png \
//!   cargo test -p unlucky-app-iced gallery::probe
//! ```

use iced::widget::{column, container, row, text};
use iced::{Background, Border, Element, Length, Padding};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::theme::Ui;

use super::super::buttons::{self, Button};
use super::super::inputs::{self, Field, SearchBar, Slider};
use super::super::interaction::{self, Base, Interaction};
use super::super::navigation::{self, ListItem, Tab, Tabs};
use super::super::selection::{self, CheckValue, Checkbox, Chip, Radio, Switch};
use super::{heading, DemoMessage};

/// 一列的底（**板画什么，控件就叠什么**）。
const COLUMNS: [(&str, Base); 3] = [
    ("canvas · Surface", Base::CANVAS),
    (
        "card · SurfaceContainer",
        super::super::surfaces::elevated::BASE,
    ),
    (
        "panel · SurfaceContainerLow",
        Base::of(Role::SurfaceContainerLow),
    ),
];

/// 板宽：3 × 360 + 2 × 8 间距 = 1096 < 1180 − 2×24 页边距。
const PLATE_WIDTH: f32 = 360.0;

/// `#RRGGBB`（图上的色值要能直接和 probe 的期望值对上）。
fn hex(color: iced::Color) -> String {
    let channel = |v: f32| (v * 255.0).round() as u8;
    format!(
        "#{:02X}{:02X}{:02X}",
        channel(color.r),
        channel(color.g),
        channel(color.b)
    )
}

/// 整段（标题 + 三块并排的板）。
pub(super) fn section<'a>(ui: &'a Ui) -> Element<'a, DemoMessage> {
    let note = ui.style(TypeRole::BodySmall);
    let columns = COLUMNS
        .iter()
        .copied()
        .map(|(title, base)| plate(ui, title, base))
        .collect::<Vec<_>>();

    column![
        heading(
            ui,
            "disabled · 合成底 = 所在容器的底（M234）",
            "10%（按钮族）/ 12%（面状控件族）/ 38%（内容）全部预合成成实色；\
             三列控件相同，只有**脚下的底**不同",
        ),
        row(columns).spacing(ui.space(Space::X2)),
        text(
            "三列的控件完全相同，只有脚下的底不同。禁用容器保留 90% 的底差（10% 只叠 on-surface），所以列间差别 = 底差的九成（浅色 Silver 11/255、深色 Ink 6/255）。这张图是取证；判据在 `interaction::disabled::tests`（逐值）与 `gallery::probe` 的 disabledbase 段（精确像素）",
        )
        .size(note.size_px)
        .font(ui.font_of(note))
        .color(ui.color(Role::OnSurfaceVariant)),
    ]
    .spacing(ui.space(Space::X2))
    .into()
}

/// 一块板：**底色就是这一列的 `base`**，控件全部把同一个 `base` 传进 `.base(..)`。
///
/// 板上写着这一列**算出来的**色值（同一入口：`disabled_button_colors` / `disabled_colors`）：
/// 三列只差 ≈1/255，肉眼判不了，图上的数字是给 reviewer 与 probe 对账用的。
fn plate<'a>(ui: &'a Ui, title: &'a str, base: Base) -> Element<'a, DemoMessage> {
    let caption = ui.style(TypeRole::BodySmall);
    let label = text(title)
        .size(caption.size_px)
        .font(ui.font_of(caption))
        .color(ui.color(Role::OnSurface));
    let numbers = text(format!(
        "底 {} · 禁用按钮(10%) {} · 禁用字段(12%) {}",
        hex(ui.color(base.role())),
        hex(interaction::disabled_button_colors(ui, base).0),
        hex(interaction::disabled_colors(ui, base).0),
    ))
    .size(caption.size_px)
    .font(ui.font_of(caption))
    .color(ui.color(Role::OnSurfaceVariant));

    container(
        column![
            column![label, numbers].spacing(ui.space(Space::X1)),
            column_body(ui, base)
        ]
        .spacing(ui.space(Space::X2)),
    )
    .width(Length::Fixed(PLATE_WIDTH))
    .padding(Padding::from(ui.space(Space::X2)))
    .style(move |_theme| container::Style {
        // 板底 = 这一列的 base；描边只为让 `canvas` 列也有边界（画布色与页面同色）。
        background: Some(Background::Color(ui.color(base.role()))),
        border: Border {
            color: ui.color(Role::OutlineVariant),
            width: crate::theme::STROKE_PX,
            radius: ui.radius_dp(unlucky_design::shape::ShapeScale::Medium),
        },
        ..container::Style::default()
    })
    .into()
}

/// 一列控件：**同族不同底**，每条都显式传 `base`。
fn column_body<'a>(ui: &'a Ui, base: Base) -> Element<'a, DemoMessage> {
    let gap = ui.space(Space::X2);
    column![
        row![
            buttons::filled(ui, Button::disabled("保存").base(base)),
            buttons::outlined(ui, Button::disabled("取消").base(base)),
        ]
        .spacing(gap),
        inputs::text_field(
            ui,
            Field::new("")
                .label("名称")
                .placeholder("禁用字段")
                .state(Interaction::Disabled)
                .base(base),
            |_| DemoMessage::Pressed("field"),
        ),
        row![
            selection::checkbox(
                ui,
                Checkbox::disabled("勾选", CheckValue::Unchecked).base(base),
            ),
            selection::radio(ui, Radio::disabled("单选", false).base(base)),
            selection::switch(ui, Switch::disabled("开关", false).base(base)),
            selection::chip(
                ui,
                Chip::filter("标签", false, DemoMessage::Pressed("chip"))
                    .disabled()
                    .base(base),
            ),
        ]
        .spacing(gap),
        inputs::search_bar(
            ui,
            SearchBar::new("")
                .placeholder("搜索")
                .state(Interaction::Disabled)
                .base(base),
            |_| DemoMessage::Pressed("search"),
        ),
        navigation::list_item(
            ui,
            ListItem::new("列表项", DemoMessage::Pressed("item"))
                .disabled()
                .base(base),
        ),
        inputs::slider(
            Slider::new(ui, 0.45)
                .state(Interaction::Disabled)
                .base(base),
        ),
        navigation::tabs(
            ui,
            Tabs::primary()
                .tab(Tab::new("标签页", DemoMessage::Pressed("tab")).state(Interaction::Disabled),)
                .base(base),
        ),
    ]
    .spacing(gap)
    .into()
}
