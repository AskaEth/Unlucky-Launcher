//! 版本管理页的**三栏装配**：类型栏 + 主版本栏 + 列表列（宽档不够时退回顶部 chips）。
//!
//! ## 三栏各自的内容
//!
//! | 栏 | 画什么 | 交互 |
//! |---|---|---|
//! | 版本类型（第一栏） | 折叠开关 + 四个类型项（原顶部 tabs 的四个 chip，语义一字不改） | [`Message::SetCategory`]（真指令：`AppCommand::ListVersions`） |
//! | 主版本（第二栏） | 「全部版本」+ 快照里出现的每个主版本桶（[`super::group`]） | [`Message::SelectMajor`]（纯视图态） |
//! | 列表 / 内容（第三栏） | 搜索条 + 命中数 + 计数头 + 版本行 + 底部安装面板 | [`Message::SetVersionQuery`] / 选版本 / 安装 |
//!
//! ## 宽度不在这里拍
//!
//! 两条栏的宽度来自 [`VersionRails`]（宽档裁决，见 `pages/layout/rails.rs` 的数值表）。
//! 本文件只做两件事：把裁决出来的数字用上、把「用户手动折叠类型栏」这条覆盖叠上去。
//!
//! ## 为什么搜索条在第三栏而不是横跨三栏
//!
//! 搜索过滤的对象是**第三栏的列表**；横跨三栏会让人以为它也在搜侧栏里的类型。放进列表列顶部，
//! 「输入 → 下面的行变少」这条因果在视觉上就是同一条竖线；命中数是这条因果的读数，紧贴右侧。
//!
//! ## 键盘可达
//!
//! 两层侧栏的每一项都是真组件按钮（`navigation::list_item` / `navigation::tabs` /
//! `buttons::icon`）：焦点由 iced 的焦点系统给，Tab 依次走「类型栏 → 主版本栏 → 搜索框 → 版本行 →
//! 安装」，Enter 激活。页面不自己写焦点环（那是组件的状态层）。

use iced::widget::tooltip::Position;
use iced::widget::{column, container, row, scrollable, tooltip};
use iced::{Alignment, Element, Length, Padding};

use unlucky_design::color::Role;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use unlucky_i18n::{Catalog, Key};

use crate::app::components::buttons::{self, IconVariant};
use crate::app::components::navigation::list_item::{list_item, ListItem};
use crate::app::components::navigation::tabs::{tabs, Tab, Tabs};
use crate::app::components::selection::{chip, Chip};
use crate::theme::Ui;

use super::super::frame;
use super::super::layout::{VersionRails, RAIL_ICON_WIDTH};
use super::super::Message;
use super::model::{Category, Download};
use super::rows;

pub(super) mod major;

pub(super) use major::{major_entries, major_rail};

// 只有测试要从这里拿「一项画成什么样」的判定（lib 里由 `major_item` 内部消费）
#[cfg(test)]
pub(super) use major::{major_shape, MajorShape};

/// 类型栏里一项的图标。
///
/// 图标语汇只有 28 个描边字形（没有 `History` / `Camera`），所以借用：
/// **全部 = 网格、正式版 = 勾、预览版 = 刷新、远古版 = 档案袋**。已登记为偏离。
pub(super) const fn category_icon(category: Category) -> IconKind {
    match category {
        Category::All => IconKind::Instances,
        Category::Release => IconKind::Check,
        Category::Snapshot => IconKind::Refresh,
        Category::Ancient => IconKind::Folder,
    }
}

/// 三栏装配（`Wide` / `Narrow` / `Compact` 三档共用）。
pub(super) fn three_column<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Download,
    rails: VersionRails,
    body_height: f32,
) -> Element<'a, Message> {
    // 分高方式与列表高度都从 `TailShare` 一处取（`Pinned` 档不给整行高度）。
    // M238：面板预算按版本菜单的行数推，所以 `list_height` 也要拿到那个条数。
    let state = super::install::panel_state(model);
    let share = rows::TailShare::of(rails.tier, &state);
    let list_height = share.list_height(body_height, rows::menu_items(&state));
    row![
        panel(
            ui,
            rails.type_rail,
            type_rail(ui, catalog, model, rails.type_rail)
        ),
        panel(
            ui,
            rails.major_rail,
            major_rail(ui, catalog, model, rails.major_rail)
        ),
        container(rows::content(ui, catalog, model, share, list_height))
            .width(Length::Fixed(rails.list))
            .height(Length::Fill),
    ]
    .spacing(super::super::layout::rails::RAIL_GAP)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// 一条侧栏的外壳：固定宽 + 面板底色 + 独立滚动（栏内内容不横向挤）。
fn panel<'a>(
    ui: &'a Ui,
    width: f32,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    container(scrollable(content).height(Length::Fill).style(frame::scroll_style(ui)))
        .width(Length::Fixed(width))
        .height(Length::Fill)
        // 水平不留内边距：140 宽里 `ListItem` 自己的 16+16 已经够，再多 8 会把 `Snapshot` 切掉
        // （首轮出图实测「Snapsh」）。垂直留 8 让第一项不贴顶。
        //
        // 底色取 `Card` 层（`SurfaceContainerLow`，比页面 Canvas 高一档）：首轮出图用的是 Canvas，
        // 两条栏和页面底色一模一样 ⇒ 读起来像「一堆文字浮在左边」，不像两条侧栏。
        .padding(Padding::from([ui.space(Space::X2), 0.0]))
        .style(move |_theme| frame::filled(ui.layer(MaterialLayer::Card)))
        .into()
}

/// 第一栏：版本类型（`140` 展开 / `64` 图标条）。
fn type_rail<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Download,
    width: f32,
) -> Element<'a, Message> {
    // 宽度 ≤ 图标条宽 ⇒ 只画图标。用户手动折叠与窄档折叠走同一条判据（不另设开关）。
    let icon_only = width <= RAIL_ICON_WIDTH;

    let mut body = column![].spacing(ui.space(Space::X1)).width(Length::Fill);
    let fold = fold_action(ui, model, icon_only);
    if icon_only {
        body = body.push(
            container(fold)
                .width(Length::Fill)
                .align_x(Alignment::Center),
        );
    } else {
        body = body.push(rail_title(ui, catalog.text(Key::VersionRailTitle), fold));
    }

    for category in Category::ALL {
        let selected = category == model.category;
        let label = catalog.text(category.label_key());
        let press = Message::SetCategory(category);

        body = body.push(if icon_only {
            // 图标条：40×40 图标按钮（命中区 48 由栏内留白补足），选中 = `Tonal`。
            // 64 宽放不下标签，所以每一项都挂 tooltip（复审建议）：四个图标既不说话也没有标题时，
            // 用户只能靠猜。tooltip 的容器色/圆角走令牌，不新造样式。
            container(labeled(ui, label, category_icon(category), selected, press))
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .into()
        } else {
            let mut item = ListItem::new(label, press).leading(category_icon(category));
            if selected {
                item = item.selected();
            }
            list_item(ui, item)
        });
    }

    body.into()
}

/// 带 tooltip 的图标项（图标条用）。tooltip 位置在右侧——它是竖栏，右边就是内容区，
/// 弹在下面会盖住下一项。
fn labeled<'a>(
    ui: &'a Ui,
    label: &'a str,
    kind: IconKind,
    selected: bool,
    press: Message,
) -> Element<'a, Message> {
    let button = buttons::icon(ui, select_variant(selected), kind, Some(press), None);
    tooltip(button, tip_body(ui, label), Position::Right).into()
}

/// tooltip 的正文盒子（`BodySmall` + `OnSurface` 叠 `Overlay` 层）：类型栏的图标项与
/// 主版本栏图标条档的「全部版本」共用一份文字语言。
fn tip_body<'a>(ui: &'a Ui, label: &'a str) -> Element<'a, Message> {
    container(frame::text_role(
        ui,
        TypeRole::BodySmall,
        false,
        label,
        ui.color(Role::OnSurface),
    ))
    .padding(ui.space(Space::X2))
    .style(move |_theme| frame::filled(ui.layer(MaterialLayer::Overlay)))
    .into()
}

/// 选中态 → 图标按钮的三式（`Tonal` = 当前项，与 ListItem 的 `SecondaryContainer` 同色系）。
fn select_variant(selected: bool) -> IconVariant {
    if selected {
        IconVariant::Tonal
    } else {
        IconVariant::Standard
    }
}

/// 栏标题（`LabelMedium` + `OnSurfaceVariant`，与列表计数头同一档字级）。
///
/// 高度固定成 `control_height`（40 = 图标按钮的边长）：两条栏的标题必须**横向对齐**，
/// 而第一栏标题右侧有个 40 高的折叠按钮、第二栏没有。不固定高度的话两行标题会差 12px。
fn rail_title<'a>(
    ui: &'a Ui,
    title: &'a str,
    action: Element<'a, Message>,
) -> Element<'a, Message> {
    container(
        row![
            container(frame::text_role(
                ui,
                TypeRole::LabelMedium,
                false,
                title,
                ui.color(Role::OnSurfaceVariant),
            ))
            .width(Length::Fill),
            action,
        ]
        .width(Length::Fill)
        .align_y(Alignment::Center)
        .padding(Padding::from([0.0, ui.space(Space::X2)])),
    )
    .height(Length::Fixed(ui.metrics().control_height))
    .align_y(Alignment::Center)
    .into()
}

/// 折叠 / 展开类型栏的图标按钮（`Sidebar` 字形；折叠中 = `Tonal`）。
fn fold_action<'a>(ui: &'a Ui, model: &Download, icon_only: bool) -> Element<'a, Message> {
    // 已经被宽度压成图标条时不再给开关：那时点它也不会变宽（档位说了算），
    // 一个按了没反应的按钮正是 D30 判 p1 的那种假控件。
    if icon_only && !model.type_rail_folded {
        return frame::gap(ui.metrics().nav_item_gap);
    }
    buttons::icon(
        ui,
        select_variant(model.type_rail_folded),
        IconKind::Sidebar,
        Some(Message::ToggleTypeRail),
        None,
    )
}

/// 顶部 chips（可用宽 < 600 时三栏不成立：类型退回 Primary tabs，主版本变成 Secondary chips 行）。
///
/// 与三栏档**同源**：同一批 [`Category`] / 同一批主版本桶 / 同两条消息，不是另造一套筛选状态。
pub(super) fn stacked_chips<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Download,
) -> Element<'a, Message> {
    let mut types = Tabs::primary();
    for category in Category::ALL {
        let mut tab = Tab::new(
            catalog.text(category.label_key()),
            Message::SetCategory(category),
        );
        if category == model.category {
            tab = tab.selected();
        }
        types = types.tab(tab);
    }

    // 主版本 chips：**从同一份 `major_entries` 画**，并且整行套横向滚动（复审 P2-2）。
    //
    // 为什么必须能滚：真实清单里 `Release` 就有 ~20 个桶（1.21…1.7），chips 档的可用宽不到 600、
    // 一行只放得下 5~6 个。没有横向滚动时后面的桶**永远点不到**——而这正是「主版本下拉」要解决的
    // 那件事（设计系统里没有 dropdown 组件，所以用可横向滚动的 chips 行承担同一职责）。
    let mut majors = row![]
        .spacing(ui.space(Space::X2))
        .align_y(Alignment::Center);
    for entry in major_entries(catalog, model) {
        majors = majors.push(chip(
            ui,
            Chip::filter(
                entry.label,
                entry.selected,
                Message::SelectMajor(entry.key.map(str::to_string)),
            ),
        ));
    }

    let strip = scrollable(majors)
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new()
                .width(frame::SCROLLBAR_WIDTH)
                .scroller_width(frame::SCROLLBAR_WIDTH)
                .margin(frame::SCROLLBAR_MARGIN),
        ))
        .height(Length::Fixed(
            ui.metrics().control_height + ui.space(Space::X2),
        ))
        .style(frame::scroll_style(ui));

    column![tabs(ui, types), strip]
        .spacing(ui.space(Space::X2))
        .width(Length::Fill)
        .into()
}
