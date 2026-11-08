//! gallery 里的导航类小节：四块状态矩阵 + 一块长文案块（**断言在 `navigation::probe`**）。
//!
//! ## 块选择器 `ULL_NAV_PART`
//!
//! `ULL_GALLERY_SECTION=navigation` 只隔离家族；本家族自己就有四块矩阵 + 长文案块，竖排比 760 高，
//! 一屏只拍得到第一块。所以按契约 §9.2 的同一套规则再切一层（环境变量 + 默认兼容 + 未知值回落并打
//! stderr）：`ULL_NAV_PART=list|tabs|drawer|appbar|long|all`（默认 `all`）。
//!
//! ```text
//! ULL_GALLERY_SECTION=navigation ULL_NAV_PART=list ./target/debug/unlucky-launcher-iced.exe //!   --software-renderer --screen=gallery --screenshot target/m79/list-light.png --screenshot-theme=light
//! ULL_NAV_DIR=target/m79 cargo test -p unlucky-app-iced navigation::probe
//! ```
//!
//! ## 矩阵形状
//!
//! ```text
//! ListItem   行 = 一 / 两 / 三行（44 / 60 / 76）   列 = 选中 / 未选中 / 禁用（三列槽位完全相同）
//! Tabs       三行 Primary（选中第 1/2/3 个）+ 一行 Secondary + 一行五态
//! Drawer     rail 88 与 drawer 240 **同尺度并排**（D33），各 3 项、第 2 项 hover、第 3 项禁用
//! AppBar     静息 / 滚动两态（56 高；滚动只换容器色 + L2）
//! long       长文案：页头 + 三行 ListItem + drawer（单行约束 D-M79-5 的取证）
//! ```
//!
//! 取景板（`SurfaceContainerLow`）不是装饰：`Surface` 的页头/导航与页面底色同色，不垫一层板子，
//! 截图里量不到边界（probe 就是靠板量 88 / 240 的）。

use iced::widget::{column, container, row, text};
use iced::{Background, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::theme::Ui;

use super::super::interaction::Interaction;
use super::super::navigation::{
    self, app_bar, list_item, navigation, tabs, AppBar, AppBarAction, ListItem, NavGroup, NavItem,
    NavLayout, Navigation, Tab, Tabs, TabsVariant,
};
use super::{heading, DemoMessage};

/// 块选择器的环境变量名（语义见模块文档）。
pub const PART_ENV: &str = "ULL_NAV_PART";

/// 本次渲染哪一块（`all` = 四块都渲染，默认）。
fn part() -> String {
    let Ok(raw) = std::env::var(PART_ENV) else {
        return "all".to_owned();
    };
    let name = raw.trim().to_ascii_lowercase();
    match name.as_str() {
        "" | "all" | "list" | "tabs" | "drawer" | "appbar" | "long" => name,
        _ => {
            eprintln!(
                "[gallery] {PART_ENV}={raw:?} 不是已知块（all|list|tabs|drawer|appbar|long），已回落 all"
            );
            "all".to_owned()
        }
    }
}

/// 演示消息（组件只认泛型，映射在 gallery 侧落地）。
fn pressed() -> DemoMessage {
    DemoMessage::Pressed("navigation")
}

fn icon(name: &str) -> IconKind {
    match name {
        "download" => IconKind::Download,
        "logs" => IconKind::Logs,
        "settings" => IconKind::Settings,
        _ => IconKind::Instances,
    }
}

/// 取景板：`SurfaceContainerLow` 底（`Surface` 的页头/导航要靠它才有边界）。
fn plate<'a>(
    ui: &'a Ui,
    pad: f32,
    content: impl Into<Element<'a, DemoMessage>>,
) -> Element<'a, DemoMessage> {
    container(content)
        .padding(pad)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(ui.color(Role::SurfaceContainerLow))),
            ..container::Style::default()
        })
        .into()
}

/// 一行小字图例。
fn note<'a>(ui: &'a Ui, value: &'a str) -> Element<'a, DemoMessage> {
    let style = ui.style(TypeRole::BodySmall);
    text(value)
        .size(style.size_px)
        .font(ui.font_of(style))
        .color(ui.color(Role::OnSurfaceVariant))
        .into()
}

/// 导航类家族的小节。
pub(super) fn section<'a>(ui: &'a Ui) -> Element<'a, DemoMessage> {
    let picked = part();
    let mut body = column![heading(
        ui,
        "navigation · list item / tabs / drawer / app bar",
        "12b §4/§6/§16 + 15 §21：ListItem 44/60/76 · Tabs 48 + 指示条 3 · \
         Drawer = rail 88 ⇄ drawer 240（同一容器）· AppBar 56 + 滚动换色"
    )]
    .spacing(ui.space(Space::X8));

    if picked == "all" || picked == "list" {
        body = body.push(list_part(ui));
    }
    if picked == "all" || picked == "tabs" {
        body = body.push(tabs_part(ui));
    }
    if picked == "all" || picked == "drawer" {
        body = body.push(drawer_part(ui));
    }
    if picked == "all" || picked == "appbar" {
        body = body.push(app_bar_part(ui));
    }
    if picked == "long" {
        body = body.push(navigation::matrix::long_part(ui, pressed));
    }
    body.into()
}

/// 一个列表格（宽 = `nav_drawer_width` 240：够放下 icon + 两行文本，且与 drawer 药丸 216 可区分）。
fn list_cell<'a>(ui: &'a Ui, item: ListItem<'a, DemoMessage>) -> Element<'a, DemoMessage> {
    container(list_item(ui, item))
        .width(Length::Fixed(ui.metrics().nav_drawer_width))
        .into()
}

/// ListItem：三行高 × {选中 / 未选中 / 禁用}。
fn list_part<'a>(ui: &'a Ui) -> Element<'a, DemoMessage> {
    let states = ["selected", "enabled", "disabled"];

    let mut grid = column![].spacing(ui.space(Space::X4));
    let header = states
        .iter()
        .map(|name| {
            container(note(ui, name))
                .width(Length::Fixed(ui.metrics().nav_drawer_width))
                .into()
        })
        .collect::<Vec<Element<'a, DemoMessage>>>();
    grid = grid.push(
        row![
            note(ui, "rows +16 each · 44 / 60 / 76"),
            row(header).spacing(ui.space(Space::X2))
        ]
        .spacing(ui.space(Space::X2)),
    );

    for (index, lines) in ["one line", "two lines", "three lines"].iter().enumerate() {
        let mut cells = row![].spacing(ui.space(Space::X2));
        for state in states {
            // 三列的**内容完全相同**（同样的槽位），只有态不同——否则「选中 trailing = Primary」
            // 这条结论没有可比对象（视觉 r1 P2-2 点名）
            let mut item = ListItem::new(lines, pressed())
                .leading(icon("instances"))
                .trailing(icon("settings"));
            if index >= 1 {
                item = item.supporting("3 versions · 1.21.4");
            }
            if index >= 2 {
                item = item.overline("RECENT");
            }
            item = match state {
                "selected" => item.selected(),
                // 禁用是修饰器：这里只清消息，槽位全部保留
                "disabled" => item.disabled(),
                _ => item.state(Interaction::Enabled),
            };
            cells = cells.push(list_cell(ui, item));
        }
        grid = grid.push(cells);
    }

    column![
        note(
            ui,
            "ListItem · 选中 = SecondaryContainer（偏离 D-M79-1：A1 的 ListItem 不换底色）· \
             禁用 = OnSurface@38%（A1 ListTokens）"
        ),
        grid,
    ]
    .spacing(ui.space(Space::X2))
    .into()
}

/// Tabs：两行 Primary（短 / 长文案各选中一次 → 指示条宽度必须不同）+ Secondary + 五态。
fn tabs_part<'a>(ui: &'a Ui) -> Element<'a, DemoMessage> {
    let primary_row = |selected: usize| {
        let mut builder = Tabs::primary();
        for (index, label) in ["Logs", "Instance details", "Settings"].iter().enumerate() {
            let mut tab = Tab::new(label, pressed());
            if index == 2 {
                // 6.1：带图标的 Tabs 容器高 64（纯文字 48）
                tab = tab.icon(icon("settings"));
            }
            if index == selected {
                tab = tab.selected();
            }
            builder = builder.tab(tab);
        }
        tabs(ui, builder)
    };

    let state_row = {
        let mut builder = Tabs::primary();
        for (index, state) in [
            Interaction::Enabled,
            Interaction::Hovered,
            Interaction::Focused,
            Interaction::Pressed,
            Interaction::Disabled,
        ]
        .iter()
        .enumerate()
        {
            let mut tab = Tab::new("Tab", pressed()).state(*state);
            if index == 0 {
                tab = tab.selected();
            }
            builder = builder.tab(tab);
        }
        tabs(ui, builder)
    };

    let secondary = tabs(
        ui,
        Tabs::secondary()
            .tab(Tab::new("Release", pressed()).selected())
            .tab(Tab::new("Snapshot", pressed()))
            .tab(Tab::new("Legacy", pressed()).disabled()),
    );

    // 两式的名字进图例（同时把 `TabsVariant::ALL` 变成可点检的常量，而不是死代码）
    let legend = TabsVariant::ALL
        .iter()
        .map(|variant| note(ui, variant.name()))
        .collect::<Vec<_>>();

    column![
        note(
            ui,
            "Tabs · Primary 三行 = 选中第 1 / 2 / 3 个（指示条跟着走且宽度跟随该项）；Secondary：1px SurfaceVariant"
        ),
        row(legend).spacing(ui.space(Space::X4)),
        primary_row(0),
        primary_row(1),
        primary_row(2),
        secondary,
        note(ui, "Primary 五态（第 1 项选中；未选项 hover/focus/pressed 提到 OnSurface）"),
        state_row,
    ]
    .spacing(ui.space(Space::X2))
    .into()
}

/// 导航容器：rail 88 与 drawer 240 **同尺度并排**（D33 的可验收形式）。
fn drawer_part<'a>(ui: &'a Ui) -> Element<'a, DemoMessage> {
    // 取景板高 288（= Space::X8 × 9）：装得下 drawer 的「两个分组 + 三项 + 两次标题」，否则末行被裁
    let plate_height = ui.space(Space::X8) * 9.0;
    let mut panes = row![].spacing(ui.space(Space::X6));

    for layout in NavLayout::ALL {
        let builder = match layout {
            NavLayout::Rail => Navigation::rail(),
            NavLayout::Drawer => Navigation::drawer(),
        };
        let pane = builder
            .group(
                NavGroup::titled("LIBRARY")
                    .item(NavItem::new("Instances", icon("instances"), pressed()).selected())
                    .item(
                        NavItem::new("Download", icon("download"), pressed())
                            .state(Interaction::Hovered),
                    ),
            )
            .group(
                NavGroup::titled("SYSTEM")
                    .item(NavItem::new("Settings", icon("settings"), pressed()).disabled()),
            );
        let width = match layout {
            NavLayout::Rail => ui.metrics().nav_rail_width,
            NavLayout::Drawer => ui.metrics().nav_drawer_width,
        };
        // 取景板比容器大一圈（pad 8）：`Surface` 的导航与页面底色同色，板要露边才量得到
        let pad = ui.space(Space::X2);
        panes = panes.push(
            container(container(navigation(ui, pane)).height(Length::Fixed(plate_height)))
                .width(Length::Fixed(width + pad * 2.0))
                .height(Length::Fixed(plate_height + pad * 2.0))
                .padding(pad)
                .style(move |_theme| container::Style {
                    background: Some(Background::Color(ui.color(Role::SurfaceContainerLow))),
                    ..container::Style::default()
                }),
        );
    }

    column![
        note(
            ui,
            "D33：同一个容器两个宽度 —— rail（药丸 56×32）｜drawer（药丸 216×56，左右各缩 12）；             第 2 项 hover、第 3 项禁用"
        ),
        panes,
        row![
            note(ui, "左→右："),
            note(ui, NavLayout::Rail.name()),
            note(ui, NavLayout::Drawer.name()),
        ]
        .spacing(ui.space(Space::X2)),
    ]
    .spacing(ui.space(Space::X2))
    .into()
}

/// AppBar small：静息 / 滚动两态（56 高；滚动换容器色 + L2）。
fn app_bar_part<'a>(ui: &'a Ui) -> Element<'a, DemoMessage> {
    let bar = |scrolled: bool| {
        app_bar(
            ui,
            AppBar::new("Instances")
                .subtitle(if scrolled {
                    "scrolled · SurfaceContainer + L2"
                } else {
                    "resting · Surface + L0"
                })
                .leading(AppBarAction::inert(icon("instances")))
                .trailing(AppBarAction::new(icon("settings"), pressed()))
                .scrolled(scrolled),
        )
    };

    column![
        note(
            ui,
            "AppBar small · 56 高（A1 64，偏离 D-M79-2）· 滚动只换容器色与高程，不改高度（A1 无 OnScroll…Height）"
        ),
        plate(ui, ui.space(Space::X2), column![bar(false), bar(true)].spacing(ui.space(Space::X2))),
    ]
    .spacing(ui.space(Space::X2))
    .into()
}
