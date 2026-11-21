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
//! ## 两个「按尺寸换版面」的判据都在别处（本条只消费）
//!
//! | 判据 | 在哪 | 换什么 |
//! |---|---|---|
//! | 正文 ≥ 460 | [`super::layout::BodyMode`] | 列表自己滚（`Split`）↔ 整列一个滚动区（`ScrollWhole`） |
//! | 正文 < 常规列头 + 一张整卡（= 361） | [`super::parts::head::plan`] | 筛选四排 ↔ 一排横向滚动；整合包的导入区还在矮窗里排到结果之后 |
//!
//! 两条门槛都是「令牌相加」算出来的，不是拍的窗口尺寸；`Split` 档（1180×900 的验收锚）
//! 在结构上碰不到第二条（361 < 460）。
//!
//! ## 详情块站哪（M361）
//!
//! 三条排法，判据在 [`super::stack::detail_in_list_scroll`]：
//!
//! | 档 | 详情在哪 |
//! |---|---|
//! | `Wide` / `Narrow` | 右侧**独立详情列**（`detail_column`），与列表互不抢高度 |
//! | `Compact` / `Tight` + `Split` | 结果行**之后**、与行同一个滚动区（`parts::detail_tail`） |
//! | `Compact` / `Tight` + `ScrollWhole` | 主列末尾（整列一个滚动区，列表按自然高度） |
//!
//! 第二行是本条修的 P1-1：修前详情和 `Fill` 列表挤在同一个 column 里，详情高随数据变、
//! 没有上界 ⇒ 880×900 列表视口只剩 45px、944×720 是 0（一条结果都看不到）。
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
use super::model::Content;
use super::parts;
use super::stack;
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
    // M357：这个数现在**扣掉了提示条**（`FrameMetrics::banner`），与页面真拿到的那块一致。
    let body_height = frame::body_height(ui, metrics, window_height, true);
    let mode = content_layout::BodyMode::of(body_height);
    let header = Header::new(catalog.text(Key::ContentTitle))
        .subtitle(catalog.text(Key::ContentSubtitle))
        .action(parts::section_switcher(ui, catalog, true));
    frame::page(
        ui,
        metrics,
        header,
        body(ui, metrics, catalog, model, mode, body_height),
    )
}

fn body<'a>(
    ui: &'a Ui,
    metrics: FrameMetrics,
    catalog: Catalog,
    model: &'a Content,
    mode: content_layout::BodyMode,
    body_height: f32,
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
            main_column(ui, catalog, model, rails, mode, body_height)
        ]
        .spacing(ui.space(Space::X2))
        .width(Length::Fill)
        .height(Length::Fill);
        columns = row![main].width(Length::Fill).height(Length::Fill);
    } else {
        columns = columns.push(main_column(ui, catalog, model, rails, mode, body_height));
        if rails.tier.has_detail() {
            columns = columns.push(detail_column(ui, catalog, model, rails));
        }
    }

    let mut stack = column![columns]
        .spacing(ui.space(Space::X3))
        .width(Length::Fill)
        .height(Length::Fill);
    // M191：有可重试的失败项时，这块**顶替**下载页那块通用失败面板（一屏只有一个失败块）；
    // 没有名单时它连构造都不存在（`None`），不是画一个灰按钮。
    let retry = parts::retry_panel(ui, catalog, model);
    if let Some(panel) = retry {
        stack = stack.push(divider::full(ui)).push(panel);
    } else if let Some(panel) = &model.panel {
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
    body_height: f32,
) -> Element<'a, PageMessage> {
    // 主列的横向内边距不在这里加：`rails.list` 已经把可用宽算干了，再加 2×X2 会让整排
    // 溢出 32px（详情列被切掉一半，复审截图上正是这条）
    //
    // M358：列头的画法与几何**一起**在 `head::plan` 里算（列头两档 + 取整视口）。
    let plan = parts::head::plan(ui, model, mode, body_height);
    let head_view = parts::head::element(ui, catalog, model, plan.kind);

    // M113: import/export —— 只有整合包这一类才有「把本地的包装进来 / 把实例打出去」。
    // 其它三类（Mod / 资源包 / 光影）的导入就是把文件拖进目录，不属于本条任务。
    //
    // **它站哪由一条纯函数一次算完**（[`stack::pack_place`]）：M358 定「前后」、M374 定「在不在
    // 主列里」，还要看**列表有没有真的画出来**。上一版把后两条写成两个布尔，于是 `Split` + 非
    // `Ready` 时那块区两头都不在（滚动区那一份只在 `Ready` 分支里创建，`pack_above` 又因为是
    // `Split` 而为假）⇒ 搜索框每敲一下就闪掉，0 结果 / 离线页上一直不在（复审 P1-1）。
    let is_modpack = model.kind == ContentKind::Modpack;
    // 列表真的在画吗：只有 `Ready` 会画（其余四态由 `status_body` 顶替列表）。
    let status = parts::status_body(ui, catalog, model);
    let ready = status.is_none();
    let place = stack::pack_place(is_modpack, model.pack.is_entry_only(), mode, ready);

    let body: Element<'a, PageMessage> = if let Some(state) = status {
        state
    } else {
        // **必须给间距**（复审 P2-1）：这一列原先漏了 `.spacing(..)`，而列表自身的纵向内边距是 0，
        // 于是「列表视口底边」与「加载更多」贴在一起，末行正好被切在半行上。
        //
        // M361（P1-1）：没有独立详情列的档（`Compact` / `Tight`）里，详情块和列表抢的是**同一列**
        // 的高度。`Split` 档的列表是 `Fill`，而 iced 的 flex 只把「非 `Fill` 兄弟之后的剩余」给它
        // ⇒ 详情（高随数据变、没有上界）一超支，列表就被挤成 0：880×900 实测视口 **45px**、
        // 944×720 实测 **0px**（修前数据见 `DevDocs/artifacts/listzero/`）。所以这一档把详情交给
        // 列表、落进**同一个**滚动区（判据与算式在 [`super::stack`]），列表于是拿到「主列 − 尾部」
        // 的全部，下限 131 ≥ 一张卡 76。代价：详情要滚到列表末尾才看到（模块文档写了取舍）。
        let detail = stack::detail_in_list_scroll(rails.tier, mode)
            .then(|| parts::detail_tail(ui, catalog, model));
        // M374：导入区同理（它是同一类的减数，只是条目 11-05 就登记过）。
        let mut lead: Vec<Element<'a, PageMessage>> = Vec::new();
        let mut tail: Vec<Element<'a, PageMessage>> = Vec::new();
        match place {
            stack::PackPlace::ScrollLead => lead.push(parts::pack_block(ui, catalog, model)),
            stack::PackPlace::ScrollTail => tail.push(parts::pack_block(ui, catalog, model)),
            // 其余落点不归滚动区管（`AboveHead` / `BelowBody` 在主列上，`Absent` 是没这块区）
            _ => {}
        }
        let mut list = column![parts::list(
            ui, catalog, model, rails, mode, lead, tail, detail
        )]
        .spacing(ui.space(Space::X2))
        .width(Length::Fill);
        // 列表高度：`Split` 吃掉剩余高度（自己滚）；`ScrollWhole` 按内容铺开（外层滚）。
        list = if mode.list_scrolls() {
            list.height(Length::Fill)
        } else {
            list.height(Length::Shrink)
        };
        // `ScrollWhole` 档（矮窗）没有独立详情列时，详情照旧压在列表下面：整列本来就在滚，
        // 详情再高也挤不出 0（列表是 `Shrink`，不是 `Fill`）。
        if !rails.tier.has_detail() && !mode.list_scrolls() {
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

    if place.is_above_head() {
        column = column
            .push(super::packui::section(ui, catalog, &model.pack))
            .push(divider::full(ui));
    }

    column = column.push(head_view).push(body);

    if place == stack::PackPlace::BelowBody {
        column =
            column
                .push(divider::full(ui))
                .push(super::packui::section(ui, catalog, &model.pack));
    }

    if mode.list_scrolls() {
        return column.height(Length::Fill).into();
    }
    // M357（P2-2）：`ScrollWhole` 档的视口按**整行**取高——裁切线落在两张结果卡之间，
    // 而不是切在第 2 张卡的首行字中间（880×560 实测的正是后者）。定高与兜底都在 `head::plan`。
    let mut view = iced::widget::scrollable(column.height(Length::Shrink))
        .direction(iced::widget::scrollable::Direction::Vertical(
            iced::widget::scrollable::Scrollbar::new()
                .width(frame::SCROLLBAR_WIDTH)
                .scroller_width(frame::SCROLLBAR_WIDTH)
                .margin(frame::SCROLLBAR_MARGIN),
        ))
        .style(frame::scroll_style(ui));
    view = match plan.viewport {
        Some(height) => view.height(Length::Fixed(height)),
        // 一张整卡都装不下 ⇒ 与修前逐像素一致（裸 `Fill`），不掺假
        None => view.height(Length::Fill),
    };
    view.into()
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
