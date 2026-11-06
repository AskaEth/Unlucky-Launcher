//! 设置页骨架：页头、左列分组导航、表单列的滚动区、演示卡片、空态位。
//!
//! ## 为什么这里还再导出一批 `pages::frame` 的名字
//!
//! M99 拆目录时立的硬要求是**纯搬移**：调用点一字不改。设置页原来的代码从 `super::frame` 取词
//! （`frame::text_role` / `frame::card` / `frame::ICON_PX` …）；拆成目录后 `frame` 这个名字落到了
//! 本文件上，于是把用到的那几件**再导出**一次，`mod.rs` / `tests.rs` 的调用点就能原样保留。
//! 这不是新 API，只是一条转发（新增控件仍归 [`crate::pages::frame`]）。

pub(super) use super::super::frame::{
    card, divider, empty_state, glyph_box, left_aligned, page, panes, scroll_style, single_column,
    text_action, text_role, Header, ICON_PX, SCROLLBAR_MARGIN, SCROLLBAR_WIDTH,
};

use iced::widget::{column, container, mouse_area, row, scrollable};
use iced::{Alignment, Background, Border, Element, Length, Padding};

use unlucky_design::color::Role;
use unlucky_design::component_map::Component;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::icons::IconKind;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Key, Lang};

use crate::pages::appearance::Change;
use crate::pages::{Action, Message};
use crate::theme::Ui;

use super::Group;

/// 空态位高度：给图标块 96 + 标题 + 副文案 + 按钮留够空间（`[估]`，按 12b §15 的元素最小高度累加）。
const EMPTY_SLOT_HEIGHT: f32 = 320.0;

/// 页头：标题 + 副标题 + 两个占位动作。
///
/// M106：三条文案走 catalog（`lang` 由 `settings::view` 传下来）。页标题复用 `NavSettings`——
/// 左列与页头说的是同一件事，不该有第二条译文。
pub(super) fn header<'a>(ui: &'a Ui, lang: Lang) -> Header<'a> {
    let catalog = lang.catalog();
    Header::new(catalog.text(Key::NavSettings))
        .subtitle(catalog.text(Key::SettingsSubtitle))
        .action(text_action(
            ui,
            catalog.text(Key::SettingsResetSection),
            Action::ResetSection,
        ))
        .action(text_action(
            ui,
            catalog.text(Key::SettingsSave),
            Action::SaveSettings,
        ))
}

// --------------------------------------------------------------- 侧列

/// 左列分组导航（supporting-pane 的辅助列，宽 = 令牌 `nav_drawer_width` = 240）。
pub(super) fn group_nav<'a>(ui: &'a Ui, selected: usize, lang: Lang) -> Element<'a, Message> {
    let catalog = lang.catalog();
    let items = column(
        super::GROUPS
            .iter()
            .enumerate()
            .map(|(index, group)| {
                let label = super::GROUP_LABELS
                    .get(index)
                    .map_or(group.name, |key| catalog.text(*key));
                group_item(ui, group, label, index, index == selected)
            })
            .collect::<Vec<_>>(),
    )
    .spacing(ui.space(Space::X1))
    .padding(Padding::from([ui.space(Space::X2), ui.space(Space::X2)]))
    .width(Length::Fill);

    container(items)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(ui.layer(MaterialLayer::Card))),
            border: Border {
                radius: ui.radius_dp(ShapeScale::Large),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

fn group_item<'a>(
    ui: &'a Ui,
    group: &'a Group,
    label: &'a str,
    index: usize,
    selected: bool,
) -> Element<'a, Message> {
    let spec = Component::NavigationItem.spec();
    let height = ui.metrics().row_height;

    let content_color = if selected {
        ui.color(Role::OnSecondaryContainer)
    } else {
        ui.color(Role::OnSurfaceVariant)
    };

    let inner = row![
        glyph_box(group.icon, content_color, ICON_PX),
        text_role(ui, spec.text, false, label, content_color),
    ]
    .spacing(ui.space(Space::X3))
    .width(Length::Fill)
    .align_y(Alignment::Center);

    let pill = container(inner)
        .width(Length::Fill)
        .height(height)
        .padding(Padding::from([0.0, ui.space(Space::X3)]))
        .align_y(Alignment::Center)
        .style(move |_theme| container::Style {
            background: selected.then(|| Background::Color(ui.color(Role::SecondaryContainer))),
            border: Border {
                radius: ui.radius(ShapeScale::Full, height),
                ..Border::default()
            },
            ..container::Style::default()
        });

    // 点选切分组：走页面自己的 `Message::Appearance(SelectGroup)`（只改视图状态，不落盘）。
    mouse_area(pill)
        .on_press(Message::Appearance(Change::SelectGroup(index)))
        .into()
}

// --------------------------------------------------------------- 表单列

/// 表单列的滚动区：只有正文滚，页头不动（与实例页列表列同一套口径）。
///
/// 「外观」分组有 7 个控件 + 最多 3 行诊断，1180×760 下会超出可视区，所以这一层是必需的。
pub(super) fn scroll<'a>(
    ui: &'a Ui,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    scrollable(content)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new()
                .width(SCROLLBAR_WIDTH)
                .scroller_width(SCROLLBAR_WIDTH)
                .margin(SCROLLBAR_MARGIN),
        ))
        .height(Length::Fill)
        .style(scroll_style(ui))
        .into()
}

/// 演示行构成的一组设置 = 一张卡，行间 1px 分隔线（M3 设置列表的常规做法）。
///
/// 已经接了真控件的分组（`GROUP.body` 是 `Some`）不走这里。
pub(super) fn demo_card<'a>(
    ui: &'a Ui,
    rows: &'a [(&'static str, &'static str)],
) -> Element<'a, Message> {
    let mut stacked = column![].spacing(0.0).width(Length::Fill);

    for (index, (label, value)) in rows.iter().enumerate() {
        if index > 0 {
            stacked = stacked.push(divider(ui));
        }
        stacked = stacked.push(setting_row(ui, label, value));
    }

    card(ui, stacked)
}

fn setting_row<'a>(ui: &'a Ui, label: &'a str, value: &'a str) -> Element<'a, Message> {
    let height = ui.metrics().control_height + Space::X2.dp();

    row![
        container(text_role(
            ui,
            TypeRole::BodyMedium,
            false,
            label,
            ui.color(Role::OnSurface),
        ))
        .width(Length::Fill)
        .align_y(Alignment::Center),
        text_role(
            ui,
            TypeRole::BodySmall,
            false,
            value,
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .height(height)
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .into()
}

/// 空态位：没有逐实例覆盖时的样子（12b §15 的口径）。
pub(super) fn overrides_empty<'a>(ui: &'a Ui, lang: Lang) -> Element<'a, Message> {
    let catalog = lang.catalog();
    container(empty_state(
        ui,
        IconKind::Instances,
        catalog.text(Key::SettingsOverridesEmptyTitle),
        catalog.text(Key::SettingsOverridesEmptyBody),
        vec![text_action(
            ui,
            catalog.text(Key::SettingsOverridesAdd),
            Action::AddOverride,
        )],
    ))
    .width(Length::Fill)
    .height(Length::Fixed(EMPTY_SLOT_HEIGHT))
    .into()
}
