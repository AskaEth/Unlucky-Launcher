//! gallery 里的输入类小节（D30 的状态矩阵登记）：TextField / Search / Slider / Menu。
//!
//! ## 为什么分成四个 `part`
//!
//! 一整段（四组件 × 五态 × 两式）比 760 高的窗口还高，`--screen=gallery` 的截图只能看到顶部。
//! 所以本文件把证据拆成四个自足的 `part`（[`text_field_part`] / [`search_part`] /
//! [`slider_part`] / [`menu_part`]），[`section`] 按 D30 的顺序把它们竖排——
//! 出图时用副本工程把 `section()` 换成单个 `part`（只调段序、不改组件代码，报告里有 diff 与说明）。
//!
//! ## 矩阵形状
//!
//! ```text
//! TextField（两行 × 五态）            列 = enabled / hovered / pressed / focused / disabled
//!   filled    | [字段] | ...
//!   outlined  | [字段] | ...
//!   另加三格：空 / 有值 / 错误（9.7 的判据要单独一眼）
//!
//! Search  （五行 + 一个展开面板）
//! Slider  （五态一行 + 0% / 50% / 100% 三行 + 离散 4 段一行）
//! Menu    （五态各一项的一只菜单 + 一只「展开态」菜单：选中 / 分隔线 / 禁用）
//! ```
//!
//! ## 每个格子都强制状态
//!
//! 与按钮家族同一个理由：截图时没法让 20 个控件同时 hover。`state(..)` 只是取证开关，
//! 不动任何状态机——图上的五态与运行期五态同源。

use iced::widget::{column, container, row, text};
use iced::{Element, Length};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::theme::Ui;

use super::super::inputs::{
    self, Field, FieldVariant, Menu, MenuItem, SearchBar, SearchView, Slider,
};
use super::super::interaction::Interaction;
use super::{cell_width, heading, row_label, state_cells, DemoMessage};

/// 演示消息（组件只认泛型，gallery 的映射在这里落地）。
fn pressed(id: &'static str) -> DemoMessage {
    DemoMessage::Pressed(id)
}

/// 输入类家族整段（四个 part 竖排；见模块文档的「为什么分成四个 part」）。
pub(super) fn section<'a>(ui: &'a Ui) -> Element<'a, DemoMessage> {
    column![
        heading(
            ui,
            "inputs · text field / search / slider / menu",
            "12b §9/§11/§8 + 15 §20：TextField 高 56（无标签 40）· Search 条 56 药丸 + 图标可视边 16 \
             （= pad 12 + offset 4）· Slider 轨 16 + 竖条 4×44 · Menu 项 48 + L2 阴影"
        ),
        text_field_part(ui),
        search_part(ui),
        slider_part(ui),
        menu_part(ui),
    ]
    .spacing(ui.space(Space::X8))
    .into()
}

/// 一行小字（图例/说明）。
fn note<'a>(ui: &'a Ui, value: &'a str) -> Element<'a, DemoMessage> {
    let style = ui.style(TypeRole::BodySmall);
    text(value)
        .size(style.size_px)
        .font(ui.font_of(style))
        .color(ui.color(Role::OnSurfaceVariant))
        .into()
}

/// 固定宽的矩阵格。
fn cell<'a>(ui: &'a Ui, content: impl Into<Element<'a, DemoMessage>>) -> Element<'a, DemoMessage> {
    container(content)
        .width(Length::Fixed(cell_width(ui)))
        .into()
}

/// 五态列头。
fn state_header<'a>(ui: &'a Ui) -> Element<'a, DemoMessage> {
    let style = ui.style(TypeRole::LabelSmall);
    let mut head = row![row_label(ui, "")].spacing(ui.space(Space::X2));
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

/// 一行矩阵（左侧标签 + 五格）。
fn state_row<'a>(
    ui: &'a Ui,
    label: &'a str,
    cells: Vec<Element<'a, DemoMessage>>,
) -> Element<'a, DemoMessage> {
    row![
        row_label(ui, label),
        row(cells).spacing(ui.space(Space::X2))
    ]
    .spacing(ui.space(Space::X2))
    .into()
}

/// TextField：两式 × 五态 + 空 / 有值 / 错误 三格。
fn text_field_part<'a>(ui: &'a Ui) -> Element<'a, DemoMessage> {
    let mut matrix = column![state_header(ui)].spacing(ui.space(Space::X2));

    for variant in FieldVariant::ALL {
        let cells = Interaction::ALL
            .iter()
            .map(|state| {
                cell(
                    ui,
                    inputs::text_field(
                        ui,
                        Field::new("实例一")
                            .variant(variant)
                            .label("名称")
                            .state(*state),
                        |_| pressed("field"),
                    ),
                )
            })
            .collect::<Vec<_>>();
        matrix = matrix.push(state_row(ui, variant.name(), cells));
    }

    // 三格用 **320** 宽（= 2 个矩阵格）：9.2 的最小宽 280 只在这里可验（矩阵格 160 更窄）
    let wide = Length::Fixed(cell_width(ui) * 2.0);
    let extras = row![
        inputs::text_field(
            ui,
            Field::new("")
                .label("名称")
                .placeholder("请输入名称")
                .width(wide),
            |_| pressed("field"),
        ),
        inputs::text_field(
            ui,
            Field::new("1.21.4")
                .label("名称")
                .leading(IconKind::Folder)
                .width(wide),
            |_| pressed("field"),
        ),
        inputs::text_field(
            ui,
            Field::new("")
                .variant(FieldVariant::Outlined)
                .label("名称")
                .placeholder("必填")
                .trailing(IconKind::Error)
                .supporting("该名称已被占用")
                .error()
                .width(wide),
            |_| pressed("field"),
        ),
    ]
    .spacing(ui.space(Space::X2));

    column![
        note(
            ui,
            "列 = 五态 · 行 = 两式。静息指示线/描边 1px、聚焦与错误 2px（A1 FocusOutlineWidth）；\
             错误态线/标签/辅助文本全 Error"
        ),
        matrix,
        note(
            ui,
            "另加：空（占位 OnSurfaceVariant）/ 有值（带前置图标 24）/ 错误（2px + 辅助文本）"
        ),
        extras,
    ]
    .spacing(ui.space(Space::X2))
    .into()
}

/// Search：五态 bar 竖排 + 一个展开的 docked 面板。
fn search_part<'a>(ui: &'a Ui) -> Element<'a, DemoMessage> {
    let bar_width = ui.space(Space::X8) * 12.0;
    let mut bars = column![].spacing(ui.space(Space::X2));
    for state in Interaction::ALL {
        let bar = inputs::search_bar(
            ui,
            SearchBar::new("")
                .placeholder("搜索实例 / 版本 / mod")
                .state(state)
                .width(Length::Fixed(bar_width)),
            |_| pressed("search"),
        );
        bars = bars.push(
            row![row_label(ui, state.name()), bar]
                .spacing(ui.space(Space::X2))
                .align_y(iced::Alignment::Center),
        );
    }

    let results: &[&str] = &[
        "实例：原版 1.21.4",
        "版本：Fabric 0.16.9",
        "mod：Sodium 0.6.5",
    ];
    let view = inputs::search_view(
        ui,
        SearchView::new("1.21", results, "没有匹配的实例").placeholder("搜索实例 / 版本 / mod"),
        |_| pressed("search"),
    );

    column![
        note(
            ui,
            "bar：高 56 · 圆角 full(28) · SurfaceContainerHighest（M229 r2 抬档：与 filled 字段同底）· 图标可视边 16 = pad 12 + offset 4（M69 不变量）· \
             聚焦 1px Secondary；view：docked，头部 56 + 1px Outline 分隔 + 结果区最小高 240"
        ),
        row![bars, view].spacing(ui.space(Space::X6)),
    ]
    .spacing(ui.space(Space::X2))
    .into()
}

/// Slider：五态一行 + 0%/50%/100% 三行 + 离散 4 段一行。
fn slider_part<'a>(ui: &'a Ui) -> Element<'a, DemoMessage> {
    let cells = Interaction::ALL
        .iter()
        .map(|state| {
            let enabled = !state.is_disabled();
            let mut spec = Slider::new(ui, 0.5).state(*state);
            if enabled {
                spec = spec.on_change(|_| pressed("slider"));
            }
            cell(ui, inputs::slider(spec))
        })
        .collect::<Vec<_>>();

    let track_width = ui.space(Space::X8) * 14.0;
    let mut values = column![].spacing(ui.space(Space::X2));
    for (label, fraction) in [("0%", 0.0), ("50%", 0.5), ("100%", 1.0)] {
        let spec = Slider::new(ui, fraction)
            .on_change(|_| pressed("slider"))
            .state(Interaction::Enabled);
        values = values.push(
            row![
                row_label(ui, label),
                container(inputs::slider(spec)).width(Length::Fixed(track_width)),
            ]
            .spacing(ui.space(Space::X2))
            .align_y(iced::Alignment::Center),
        );
    }

    // 离散：4 段 = 3 个内部停靠点（25 / 50 / 75%），当前 25% ⇒ 能看到 50% 与 75% 两个点
    let discrete = row![
        row_label(ui, "离散 4"),
        container(inputs::slider(
            Slider::new(ui, 0.25)
                .steps(4)
                .on_change(|_| pressed("slider"))
                .state(Interaction::Enabled)
        ))
        .width(Length::Fixed(track_width)),
    ]
    .spacing(ui.space(Space::X2))
    .align_y(iced::Alignment::Center);

    column![
        note(
            ui,
            "轨 16 full（Primary / SecondaryContainer）· 竖条 4×44 full（overflow 上下各 14）· \
             focus/pressed 收窄到 2 · 命中 48 · 离散停靠点 4"
        ),
        state_row(ui, "五态", cells),
        note(ui, "0% / 50% / 100%：竖条在两端贴边但不越界（左缘 = 轨道左缘，右缘 = 轨道右缘）"),
        values,
        note(
            ui,
            "离散：steps(4) ⇒ 3 个内部停靠点（25/50/75%），未激活轨在停靠点两侧留 6 空隙，\n             激活段只画到竖条中心",
        ),
        discrete,
    ]
    .spacing(ui.space(Space::X2))
    .into()
}

/// Menu：一只五态菜单 + 一只展开态菜单（选中 / 分组线 / 禁用）。
fn menu_part<'a>(ui: &'a Ui) -> Element<'a, DemoMessage> {
    let width = ui.space(Space::X8) * 8.0;

    let mut states = Menu::new().width(Length::Fixed(width));
    for state in Interaction::ALL {
        let mut item = MenuItem::new("重命名", pressed("menu")).state(state);
        if state.is_disabled() {
            item = MenuItem::disabled("重命名").state(state);
        }
        states = states.item(item.leading(IconKind::Folder));
    }

    let open = Menu::new()
        .width(Length::Fixed(width))
        .item(MenuItem::new("重命名", pressed("menu")).leading(IconKind::Folder))
        .item(
            MenuItem::new("复制实例", pressed("menu"))
                .leading(IconKind::Download)
                .trailing(IconKind::Check)
                .selected(),
        )
        .divider()
        .item(MenuItem::disabled("删除").leading(IconKind::Trash));

    column![
        note(
            ui,
            "容器 SurfaceContainer + L2 阴影（blur 6 / offset 2 / Shadow 角色）· 圆角 4（令牌表）· \
             项高 48 · 水平 12 · 上下留白 8 · 选中项 SecondaryContainer 指示器内缩 4、圆角 8"
        ),
        row![
            column![note(ui, "五态（每项强制一态）"), inputs::menu(ui, states)]
                .spacing(ui.space(Space::X2)),
            column![
                note(ui, "展开：含选中项 / 分组分隔线（复用 divider）/ 不可点项"),
                inputs::menu(ui, open)
            ]
            .spacing(ui.space(Space::X2)),
        ]
        .spacing(ui.space(Space::X6)),
    ]
    .spacing(ui.space(Space::X2))
    .into()
}
