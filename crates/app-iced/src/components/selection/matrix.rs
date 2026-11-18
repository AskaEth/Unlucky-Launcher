//! 选择类家族的**状态矩阵**（D30：浅/深 × 五态）：12 行 × 5 列。
//!
//! ```text
//!                        | enabled | hovered | pressed | focused | disabled |
//!   checkbox·off/on/indet/error · radio·off/on · switch·off/on
//!   chip·assist / chip·filter / chip·filter·on / chip·input
//! ```
//!
//! 每格都**强制**成该态（`Checkbox::state(..)` 等取证开关）：截图时不能让 60 个控件同时 hover，
//! 而强制值只影响「画什么」，不动任何状态机——所以矩阵图里的五态与运行期五态逐像素同源。
//!
//! 为什么住在 `selection/` 而不是 `gallery/`：`gallery/<family>.rs` 只负责「怎么摆」，本文件负责
//! 「摆什么」，混在一起会顶到 400 行红线（像素断言在 `gallery/selection.rs`）。消息类型是 `()`
//! （矩阵只供视觉验收，点击不产生业务效果），由 gallery 侧一次 `.map(..)` 接成 `DemoMessage`。

use iced::widget::{column, container, row, text};
use iced::{Element, Length};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::theme::Ui;

use super::super::gallery::{cell_width, heading, row_label, state_cells};
use super::super::interaction::Interaction;
use super::{self as selection, CheckValue, Checkbox, Chip, ChipVariant, Radio, Switch};

/// 整段矩阵（列头 + 12 行）；消息类型是 `()`。
pub fn matrix<'a>(ui: &'a Ui) -> Element<'a, ()> {
    let mut rows = column![header_row(ui)].spacing(ui.space(Space::X1));
    for spec in Row::ALL {
        let cells = Interaction::ALL
            .iter()
            .map(|state| cell(ui, draw(ui, spec, *state)))
            .collect::<Vec<_>>();

        rows = rows.push(
            row![
                row_label(ui, spec.name()).map(|_| ()),
                row(cells).spacing(ui.space(Space::X2))
            ]
            .spacing(ui.space(Space::X2)),
        );
    }

    rows.into()
}

/// 家族小节的标题 + 判据说明（与矩阵分开，好让 gallery 侧决定顺序）。
pub fn heading_and_note<'a>(ui: &'a Ui) -> Element<'a, ()> {
    heading(
        ui,
        "selection · checkbox / radio / switch / chip",
        "规格卡 + 15 §18/§19 + 12b §10/§13：checkbox **18×18 圆角 4** 描边 2（向内收）勾 10.8 方头 · \
         radio 外圈 **18** 环线 2 内点 10 · switch 轨道 52×32 滑块 24/16 · chip 高 32 圆角 8 · \
         Ø40 状态层 · 命中 48",
    )
    .map(|_| ())
}

/// 图例（一行说明五态口径）。
pub fn legend<'a>(ui: &'a Ui) -> Element<'a, ()> {
    let style = ui.style(TypeRole::BodySmall);
    text(
        "图例：列 = 五态（focus 与 pressed 同 α 10%，看起来一样是规范要求）；行 = 组件×取值。\
         复选框第三行是 indeterminate（勾形变成居中短横，不换图标）；第四行是错误态。",
    )
    .size(style.size_px)
    .font(ui.font_of(style))
    .color(ui.color(Role::OnSurfaceVariant))
    .into()
}

/// 矩阵的一行（组件 × 取值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Row {
    /// 三态复选框；布尔位 = 错误态。
    Checkbox(CheckValue, bool),
    /// 单选框；布尔位 = 选中。
    Radio(bool),
    /// 开关；布尔位 = 打开。
    Switch(bool),
    /// Chip；`(类, 选中)`。
    Chip(ChipVariant, bool),
}

impl Row {
    /// 行序（矩阵自上而下）。
    const ALL: [Row; 12] = [
        Row::Checkbox(CheckValue::Unchecked, false),
        Row::Checkbox(CheckValue::Checked, false),
        Row::Checkbox(CheckValue::Indeterminate, false),
        Row::Checkbox(CheckValue::Unchecked, true),
        Row::Radio(false),
        Row::Radio(true),
        Row::Switch(false),
        Row::Switch(true),
        Row::Chip(ChipVariant::Assist, false),
        Row::Chip(ChipVariant::Filter, false),
        Row::Chip(ChipVariant::Filter, true),
        Row::Chip(ChipVariant::Input, false),
    ];

    /// 左栏标签。
    fn name(self) -> &'static str {
        match self {
            Row::Checkbox(CheckValue::Unchecked, false) => "checkbox·off",
            Row::Checkbox(CheckValue::Checked, false) => "checkbox·on",
            Row::Checkbox(CheckValue::Indeterminate, false) => "checkbox·indet",
            Row::Checkbox(..) => "checkbox·error",
            Row::Radio(false) => "radio·off",
            Row::Radio(true) => "radio·on",
            Row::Switch(false) => "switch·off",
            Row::Switch(true) => "switch·on",
            Row::Chip(ChipVariant::Assist, _) => "chip·assist",
            Row::Chip(ChipVariant::Filter, false) => "chip·filter",
            Row::Chip(ChipVariant::Filter, true) => "chip·filter·on",
            Row::Chip(..) => "chip·input",
        }
    }
}

/// 一格的组件（强制成 `state`）。
fn draw(ui: &Ui, spec: Row, state: Interaction) -> Element<'_, ()> {
    let disabled = state.is_disabled();

    match spec {
        // 五态共用同一套装配：只有「有没有消息」随禁用变（语义上的禁用，不是画成禁用）
        Row::Checkbox(value, error) => {
            let mut checkbox = match (disabled, value) {
                (true, value) => Checkbox::disabled("", value),
                (false, CheckValue::Indeterminate) => Checkbox::tri("", value, ()),
                (false, CheckValue::Checked) => Checkbox::new("", true, ()),
                (false, CheckValue::Unchecked) => Checkbox::new("", false, ()),
            }
            .state(state);
            if error {
                checkbox = checkbox.error();
            }
            selection::checkbox(ui, checkbox)
        }
        Row::Radio(selected) => {
            let radio = if disabled {
                Radio::disabled("", selected)
            } else {
                Radio::new("", selected, ())
            };
            selection::radio(ui, radio.state(state))
        }
        Row::Switch(value) => {
            let switch = if disabled {
                Switch::disabled("", value)
            } else {
                Switch::new("", value, ())
            };
            selection::switch(ui, switch.state(state))
        }
        Row::Chip(variant, selected) => {
            let label = match variant {
                ChipVariant::Assist => "Assist",
                ChipVariant::Filter => "Filter",
                ChipVariant::Input => "Input",
            };
            // 禁用**只清消息**（`Chip::disabled()` 是修饰器）：图标与选中态必须保留，
            // 否则禁用列里 `filter·on` 与 `filter` 会逐像素同形（复审 P2-1）。
            let chip = match variant {
                ChipVariant::Assist => Chip::assist(label, ()).leading(IconKind::Download),
                // Filter 的勾只在**选中**时给（选中态才需要「已筛选」的强提示）
                ChipVariant::Filter if selected => {
                    Chip::filter(label, true, ()).leading(IconKind::Check)
                }
                ChipVariant::Filter => Chip::filter(label, false, ()).leading(IconKind::Filter),
                // Input chip：前置图标 + 后置关闭键（avatar 位用图标代替）
                ChipVariant::Input => Chip::input(label, ())
                    .leading(IconKind::Folder)
                    .trailing(IconKind::Close),
            };
            let chip = if disabled { chip.disabled() } else { chip };
            selection::chip(ui, chip.state(state))
        }
    }
}

/// 列头（五态名）。
fn header_row(ui: &Ui) -> Element<'_, ()> {
    let style = ui.style(TypeRole::LabelSmall);
    let mut head = row![row_label(ui, "").map(|_| ())].spacing(ui.space(Space::X2));

    for (name, width) in state_cells(ui) {
        head = head.push(
            container(
                text(name)
                    .size(style.size_px)
                    .font(ui.font_of(style))
                    .color(ui.color(Role::OnSurfaceVariant)),
            )
            .width(Length::Fixed(width)),
        );
    }

    head.into()
}

/// 一个矩阵单元格（固定宽，内容左对齐）。
fn cell<'a>(ui: &Ui, content: Element<'a, ()>) -> Element<'a, ()> {
    container(content)
        .width(Length::Fixed(cell_width(ui)))
        .into()
}
