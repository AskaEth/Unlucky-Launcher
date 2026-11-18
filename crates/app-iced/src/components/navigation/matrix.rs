//! 导航家族的长文案取证块（D-M79-5）：**单行 + 裁切**，页面真接长实例名时不许折行。
//!
//! 为什么不放在 `gallery/navigation.rs`：那个文件已经贴到 400 行红线（AGENTS.md §3-1），
//! 而它属于 harness 单文件、scope 只授权一个路径。按 `selection::matrix` 的先例，把**取证块**
//! 收进组件目录、对消息类型泛型（`pressed: impl Fn() -> M`），gallery 只做一次 `.map`。

use iced::widget::{column, container, text};
use iced::{Background, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::theme::Ui;

use super::{
    app_bar, list_item, navigation, tabs, AppBar, AppBarAction, ListItem, NavGroup, NavItem,
    Navigation, Tab, Tabs,
};

/// 一行小字图例。
fn note<'a, M: 'a>(ui: &'a Ui, value: &'a str) -> Element<'a, M> {
    let style = ui.style(TypeRole::BodySmall);
    text(value)
        .size(style.size_px)
        .font(ui.font_of(style))
        .color(ui.color(Role::OnSurfaceVariant))
        .into()
}

/// 取景板（`SurfaceContainerLow`）：`Surface` 的页头与页面底色同色，要垫一层才量得到边界。
fn plate<'a, M: 'a>(ui: &'a Ui, pad: f32, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    container(content)
        .padding(pad)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(ui.color(Role::SurfaceContainerLow))),
            ..container::Style::default()
        })
        .into()
}

fn icon(name: &str) -> IconKind {
    match name {
        "download" => IconKind::Download,
        "logs" => IconKind::Logs,
        "settings" => IconKind::Settings,
        _ => IconKind::Instances,
    }
}

/// 长文案块（D-M79-5 的取证）：单行 + 裁切——不折行、不顶穿 56 的页头、不压 trailing。
/// 矩阵图全用短文案看不出这个坑，真实实例名（`All the Mods 10 - ATM10 …`）一接就中招。
pub fn long_part<'a, M: Clone + 'a>(ui: &'a Ui, pressed: impl Fn() -> M) -> Element<'a, M> {
    const LONG: &str = "All the Mods 10 - ATM10 - 1.21.1 - NeoForge";

    let bar = app_bar(
        ui,
        AppBar::new(LONG)
            .subtitle(LONG)
            .leading(AppBarAction::inert(icon("instances")))
            .trailing(AppBarAction::new(icon("settings"), pressed()))
            .trailing(AppBarAction::new(icon("logs"), pressed())),
    );

    let row = |lines: usize| {
        let mut item = ListItem::new(LONG, pressed())
            .leading(icon("instances"))
            .trailing(icon("settings"));
        if lines >= 2 {
            item = item.supporting(LONG);
        }
        if lines >= 3 {
            item = item.overline(LONG);
        }
        container(list_item(ui, item.selected()))
            .width(Length::Fixed(ui.metrics().nav_drawer_width))
    };

    let pane = navigation(
        ui,
        Navigation::drawer().group(
            NavGroup::titled(LONG)
                .item(NavItem::new(LONG, icon("instances"), pressed()).selected())
                .item(NavItem::new("Settings", icon("settings"), pressed())),
        ),
    );

    // Tabs：长标签不折行，tab 自己变宽（横向滚动兜住溢出）
    let tabs_row = tabs(
        ui,
        Tabs::primary()
            .tab(Tab::new(LONG, pressed()).selected())
            .tab(Tab::new("Settings", pressed())),
    );

    column![
        note(
            ui,
            "长文案（D-M79-5）：单行 + 裁切 —— 不折行、页头仍 56、不压 trailing"
        ),
        plate(ui, ui.space(Space::X2), bar),
        column![row(1), row(2), row(3)].spacing(ui.space(Space::X4)),
        tabs_row,
        container(container(pane).height(Length::Fixed(ui.space(Space::X8) * 6.0)))
            .width(Length::Fixed(ui.metrics().nav_drawer_width)),
    ]
    .spacing(ui.space(Space::X2))
    .into()
}
