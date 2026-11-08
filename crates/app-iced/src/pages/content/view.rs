//! 内容库页的整页装配：页头（含一级切换）+ 左侧类型栏 + 主列（筛选/结果）+ 详情 + 安装面板。
//!
//! ## 版面（几何在 [`super::layout`]，这里只消费结果）
//!
//! ```text
//! ┌─ 页头：Content            [版本管理 | 内容库] ─┐
//! ├──────┬────────────────────────┬──────────────┤
//! │ 类型 │ 搜索 + 四组 chip        │ 详情 / 装入   │
//! │ 四条 │ 计数头                  │ 原因逐条列出   │
//! │ 侧栏 │ 结果列表（独立滚动）      │              │
//! │      │ 加载更多 / 没有更多了    │              │
//! ├──────┴────────────────────────┴──────────────┤
//! │ 安装面板（**下载页那块**，只在有安装时出现）    │
//! └──────────────────────────────────────────────┘
//! ```
//!
//! ## 为什么类型栏用 `list_item`（与 M107 的版本类型栏同一套）
//!
//! 四条内容类型是**一级侧栏**（页面内导航），几何与状态层必须与版本管理页一致：`list_item`
//! 自带 60 高两行档与选中态，页面不自己写 hover/pressed。窄档（`Tight`）退成一行 chip——
//! 栏宽没了就不该硬塞一条 140px 的栏。

use iced::widget::{column, container, row};
use iced::{Element, Length};

use unlucky_design::spacing::Space;

use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::ContentKind;

use crate::app::components::divider;
use crate::app::components::navigation::list_item::{list_item, ListItem};
use crate::app::components::selection::{chip, Chip};
use crate::theme::Ui;

use crate::pages::download;
use crate::pages::frame::{self, Header};
use crate::pages::layout::FrameMetrics;
use crate::pages::Message as PageMessage;

use super::layout::{self as content_layout, Rails, Tier};
use super::model::{Content, Status};
use super::parts;
use super::Message as ContentMessage;

/// 整页入口（由 `pages/mod.rs` 的 `render` 调；页面层不认识 presenter 类型）。
pub(super) fn view<'a>(
    ui: &'a Ui,
    metrics: FrameMetrics,
    model: &'a Content,
    lang: unlucky_i18n::Lang,
    window_height: f32,
) -> Element<'a, PageMessage> {
    let catalog = lang.catalog();
    // 正文可用高 → 版面两档（`Split`：列表自己滚；`ScrollWhole`：主列整列一个滚动条）。
    // 门槛与判据在 [`content_layout`] 里，那里有用例钉住「不算拍脑袋的数」。
    let body_height = frame::body_height(ui, metrics, window_height, true);
    let mode = content_layout::BodyMode::of(body_height);
    let header = Header::new(catalog.text(Key::ContentTitle))
        .subtitle(catalog.text(Key::ContentSubtitle))
        .action(parts::section_switcher(ui, catalog, true));
    frame::page(ui, metrics, header, body(ui, metrics, catalog, model, mode))
}

fn body<'a>(
    ui: &'a Ui,
    metrics: FrameMetrics,
    catalog: Catalog,
    model: &'a Content,
    mode: content_layout::BodyMode,
) -> Element<'a, PageMessage> {
    let rails = content_layout::resolve(metrics.available);

    let mut columns = row![kind_rail(ui, catalog, model, rails)]
        .spacing(rails.gap)
        .width(Length::Fill)
        .height(Length::Fill);

    if rails.tier == Tier::Tight {
        // 窄到放不下侧栏：类型条跑到主列上方（同一个 `kind_rail`，chip 形态）
        let main = column![
            kind_rail(ui, catalog, model, rails),
            divider::full(ui),
            main_column(ui, catalog, model, rails, mode)
        ]
        .spacing(ui.space(Space::X2))
        .width(Length::Fill)
        .height(Length::Fill);
        columns = row![main].width(Length::Fill).height(Length::Fill);
    } else {
        columns = columns.push(main_column(ui, catalog, model, rails, mode));
        if rails.tier.has_detail() {
            columns = columns.push(detail_column(ui, catalog, model, rails));
        }
    }

    let mut stack = column![columns]
        .spacing(ui.space(Space::X3))
        .width(Length::Fill)
        .height(Length::Fill);
    if let Some(panel) = &model.panel {
        // **同一块安装面板**（下载页那个函数）：内容库不另画一套进度/失败/取消。
        stack = stack
            .push(divider::full(ui))
            .push(download::install_panel(ui, catalog, panel));
    }
    stack.into()
}

/// 左侧四条内容类型（`Wide`/`Compact` 是列表项，`Narrow` 是图标项，`Tight` 是一行 chip）。
fn kind_rail<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Content,
    rails: Rails,
) -> Element<'a, PageMessage> {
    if rails.tier == Tier::Tight {
        let mut chips = Vec::new();
        for kind in ContentKind::ALL {
            chips.push(chip(
                ui,
                Chip::filter(
                    kind_label(kind, catalog),
                    model.kind == kind,
                    PageMessage::Content(ContentMessage::SetKind(kind)),
                ),
            ));
        }
        return row(chips)
            .spacing(ui.space(Space::X2))
            .width(Length::Fill)
            .into();
    }

    let labels = rails.tier.rail_has_labels();
    let mut items = column(Vec::new())
        .spacing(ui.space(Space::X1))
        .width(Length::Fill);
    for kind in ContentKind::ALL {
        let label = kind_label(kind, catalog);
        // 图标档没有文字位：标签仍传（无障碍/日志用），宽度由几何裁掉
        let mut item = ListItem::new(
            if labels { label } else { " " },
            PageMessage::Content(ContentMessage::SetKind(kind)),
        )
        .leading(parts::kind_icon(kind));
        if model.kind == kind {
            item = item.selected();
        }
        items = items.push(list_item(ui, item));
    }

    container(items)
        .width(Length::Fixed(rails.rail))
        .height(Length::Fill)
        .into()
}

/// 主列：搜索条 + 筛选 + 计数 + 结果（或四态之一）。
///
/// `mode` 决定**谁滚**（M161，判据在 `content_layout::BodyMode`）：
///
/// * `Split`：列表自己滚（筛选条常驻，高窗口的常态）；
/// * `ScrollWhole`：矮窗口——主列（含搜索条与筛选）整列进**一个**纵向滚动区，列表按自然高度铺开。
///   这样一行 `ListItem` 不会被压成半行（那看起来就是「折行挤压」），也不会出现「列表里一个滚动条、
///   外层又一个」的双滚动。
fn main_column<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Content,
    rails: Rails,
    mode: content_layout::BodyMode,
) -> Element<'a, PageMessage> {
    // 主列的横向内边距不在这里加：`rails.list` 已经把可用宽算干了，再加 2×X2 会让整排
    // 溢出 32px（详情列被切掉一半，复审截图上正是这条）
    let mut head = column![parts::search(ui, catalog, model)]
        .spacing(ui.space(Space::X2))
        .width(Length::Fill);

    // 筛选条**永远在**：它是控制面，不是数据。加载中把筛选条藏起来，用户就没法在慢网络下
    // 换条件或换类型（只能干等），这是「加载态顺手把入口收走」的经典坑。
    head = head.push(parts::filters(ui, catalog, model));
    head = head.push(divider::full(ui));
    if model.status == Status::Ready {
        // 计数头只在有数据时出现：0 条的「0 results」和空态说的是同一件事，画两遍是噪音
        head = head.push(parts::count_head(ui, catalog, model));
    }

    let body: Element<'a, PageMessage> = if let Some(state) = parts::status_body(ui, catalog, model)
    {
        state
    } else {
        // **必须给间距**（复审 P2-1）：这一列原先漏了 `.spacing(..)`，而列表自身的纵向内边距是 0，
        // 于是「列表视口底边」与「加载更多」贴在一起，末行正好被切在半行上。
        let mut list = column![parts::list(ui, catalog, model, rails, mode)]
            .spacing(ui.space(Space::X2))
            .width(Length::Fill);
        // 列表高度：`Split` 吃掉剩余高度（自己滚）；`ScrollWhole` 按内容铺开（外层滚）。
        list = if mode.list_scrolls() {
            list.height(Length::Fill)
        } else {
            list.height(Length::Shrink)
        };
        // 紧凑档没有独立详情列：详情压在列表下面（整页替换的成本比「多一块」高）
        if !rails.tier.has_detail() {
            list = list
                .push(divider::full(ui))
                .push(parts::detail(ui, catalog, model));
        }
        list.push(parts::more(ui, catalog, model)).into()
    };

    // `ScrollWhole` 档：主列整列一个滚动区（列表按自然高度）。**只有这一处**加滚动条，
    // 不在列表里再套一个（双滚动是「到处加滚动条糊」的典型症状）。
    let mut column = column![]
        .spacing(ui.space(Space::X2))
        .width(Length::Fixed(rails.list));

    // M113: import/export —— 只有整合包这一类才有「把本地的包装进来 / 把实例打出去」。
    // 其它三类（Mod / 资源包 / 光影）的导入就是把文件拖进目录，不属于本条任务。
    if model.kind == ContentKind::Modpack {
        column = column
            .push(super::packui::section(ui, catalog, &model.pack))
            .push(divider::full(ui));
    }

    column = column.push(head).push(body);
    if mode.list_scrolls() {
        return column.height(Length::Fill).into();
    }
    iced::widget::scrollable(column.height(Length::Shrink))
        .direction(iced::widget::scrollable::Direction::Vertical(
            iced::widget::scrollable::Scrollbar::new()
                .width(frame::SCROLLBAR_WIDTH)
                .scroller_width(frame::SCROLLBAR_WIDTH)
                .margin(frame::SCROLLBAR_MARGIN),
        ))
        .height(Length::Fill)
        .style(frame::scroll_style(ui))
        .into()
}

/// 详情列（只在 `Wide` / `Narrow` 存在）。
fn detail_column<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Content,
    rails: Rails,
) -> Element<'a, PageMessage> {
    container(parts::detail(ui, catalog, model))
        .width(Length::Fixed(rails.detail))
        .height(Length::Fill)
        .style(move |_theme| container::Style::default())
        .into()
}

/// 四种内容类型的界面名。
fn kind_label(kind: ContentKind, catalog: Catalog) -> &'static str {
    match kind {
        ContentKind::Modpack => catalog.text(Key::ContentKindModpack),
        ContentKind::Mod => catalog.text(Key::ContentKindMod),
        ContentKind::Resourcepack => catalog.text(Key::ContentKindResourcepack),
        ContentKind::Shader => catalog.text(Key::ContentKindShader),
    }
}
