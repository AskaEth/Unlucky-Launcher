//! 实例页：**list-detail** 的唯一用例，也是 M90 唯一打通到「真组件 + 真数据 + 真命令」的页。
//!
//! ## 两种列表形态（M455）：`ui.instances.view`
//!
//! | 视图 | 画法 | 页头控件 |
//! |---|---|---|
//! | `Tiles`（产品出厂，D16） | 整块宽度的**真网格**（`instances/grid.rs`，一行 `tile_size.columns()` 个） | 视图 chip 组 + 尺寸 chip 组 |
//! | `List` | `instances/column.rs` 的 list-detail 左列（336 固定） | 只有视图 chip 组 |
//!
//! ## 搜索框（M457）：一条条件，两种视图都生效
//!
//! | 落点 | 列表视图 | 磁贴视图 |
//! |---|---|---|
//! | 控件 | 左列卡片**里**、列头之上（`column.rs`） | 网格之上、`ALL INSTANCES` 列头之上（本文件 `tiles_body`） |
//! | 过滤 | `list::drawn(block, query)`（两种摆放**同一个行源**） | 同左 |
//! | 一个都不命中 | 卡里换成空态（`search::empty`：一句话 + 清空按钮） | 网格位置换成同一个空态 |
//!
//! 关键词住在 `App::instance_query`（**不落盘**；会话内状态），从 `ViewMode.query` 借进来。
//! 切视图、进出详情都保留；离开实例页才清（`app/update/instances/search.rs` 的模块文档写了
//! 「谁清它」的三条）。
//!
//! 为什么搜索行**不算进**页头：页头只有 84 高且右半边被 chip / 两个按钮占满，塞不下 360 宽的
//! 药丸；而把药丸放进正文、再从卡片里减掉 56，正好是「少一整行 60」（`search::row_height`
//! 与 `tests.rs` 的整行判据同源），裁切线仍落在行边界上。
//!
//! 「点一张格子」= 压一层 `Route::Detail`：磁贴视图没有第二列，详情走 `detail::detail_page`
//! （与窄窗那一屏同构，只是不写窄窗提示）。
//!
//! ## 版式（由 [`super::layout`] 决议，页面不自己算宽度）
//!
//! | 版面 | 结构 | 退化行为 |
//! |---|---|---|
//! | `ListDetail`（窗口 ≥840） | 左列 336 固定、**独立滚动**；右列详情，散文 ≤424 | — |
//! | `SingleColumn`（<840） | 列表与详情**整页切换**：本页只渲染详情列（块贴左吃满页宽，散文 ≤424） | 顶部给「Back to instances」，列表退成上一屏 |
//!
//! 为什么窄窗不是「上下堆」：官方 compact 下 list-detail 的口径是一次只显示一个 pane（列表 → 详情，
//! 整页切换）。上下堆会把详情挤到折叠线以下，等于详情不存在。
//!
//! ## M90：占位控件 → 真组件（逐项对照）
//!
//! | 位置 | M84（占位） | M90（真组件） |
//! |---|---|---|
//! | 列表行 | 手搓 `container` + `text_role`（行高 56） | `navigation::list_item`（一/两/三行 44/60/76，五态状态层） |
//! | 页头动作 | `frame::text_action`（纯 `text` + 手搓 hover） | `buttons::text`（`Button` 五式 + 状态层） |
//! | 「Play」 | `frame::text_action` | `buttons::filled`（**全屏唯一实心 accent**，rubric K2） |
//! | 分隔线 | `frame::divider` | `divider::full` |
//! | 数据 | `DEMO` 常量 | [`super::model::Instances`]（真页面由 presenter 快照转，演示由 `DEMO` 转） |
//!
//! 判据没有因此放松：列宽仍由 [`super::layout`] 给（见 `pages/tests.rs` 的 424 断言）。
//!
//! ## 行高为什么从 56 变成 60（M90 登记）
//!
//! M84 的行高是「`row_height` 44 + `Space::X3` 12 = 56」的手工值；真 `ListItem` 的两行档是
//! **60**（令牌 `row_height` 44 + 每多一行 16，A1 是 72）。差异来自组件自己的几何真源，
//! 页面不许覆盖——这正是「接真组件」的应有结果。列表仍高于可视区，独立滚动条照样可见。
//!
//! ## 行距归零 + 底部内缩（M102，视觉复审 ②）
//!
//! 视觉复审在 1180×760 的截图里抓到「末行 Redstone Lab 被卡片底边切掉半行（副标题消失）」。
//! 根因是**两道页级附加量**：`Space::X1` 的行距让行节奏变成 64，与可视高度除不尽；
//! 列表滚动区贴到卡片的 1px 圆角描边上，切点正好落在描边线上。修法两件：
//!
//! | 改什么 | 值 | 结果 |
//! |---|---|---|
//! | 页级行距 | 4 → **0**（真源 [`ROW_SPACING`] = `None`） | 行节奏回到组件的 60 |
//! | 滚动区底部内缩 | 0 → **`Space::X2` 8**（真源 [`LIST_BOTTOM_INSET`]） | 切点落在卡片的内边距带里，不压描边 |
//!
//! 于是可视区 = 580 − 列头 32 − 内缩 8 = **540 = 9 × 60**，正好 9 整行：末行完整可见，
//! 第 10 行整体落在裁切线之外。两个数都**只有一个真源**，页面与护栏都从它取
//! （`instances/tests.rs` 引 [`LIST_BOTTOM_INSET`].`dp()` 与 [`ROW_SPACING`] 算 pitch，
//! 另有一条源码结构断言盯着 `list_column()` 不许再叠字面量间距）。
//! 列表内容 12×60 = 720 仍高过可视区，独立滚动条照样可见。

// M416: 文案 / 腔调漏斗、入口意图、强制视觉态映射（贴线拆分：本文件只剩几行余量）
mod api;
mod data;
mod demo;
mod detail;
// M455: 磁贴视图的网格（一行 `tile_size.columns()` 个格子）+ `View`（视图模式 / 尺寸）
mod chips;
mod grid;
mod launch_args;
// M457: 搜索框（两种视图共用的一条过滤条件；判据 / 控件 / 空态都在那一卷）
mod search;
// M457: 列表视图的左列卡片（搜索行 + 列头 + 滚动区/空态），从本文件拆出守 400 行
mod column;
// M313: 实例列表的行序与增删动效（复用 M305 的唯一入口，不另写一套）
mod list;
// M224: 详情页的 MODS 区（本地 mod 列表 + 启停开关）
mod mods;
// M235: 详情页的 SAVES 区（存档 / 截图的只读列表 + 打开文件夹）
mod saves;
mod state;
mod text;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use data::{DEMO, NOTE, SELECTED};
pub(crate) use demo::{demo, demo_form};
pub use mods::{from_state as mods_from_state, Mods};
pub use saves::{from_state as saves_from_state, Saves};

pub use state::{LaunchForm, Message as InstanceMessage};
pub(super) use text::Texts;

/// 视图模式 + 磁贴尺寸（M455）+ 搜索关键词（M457）：`pages::ViewMode` 从这里再导出
/// （`pages/mod.rs` 负责那条路径）。带生命周期形参是因为关键词是**借来**的
/// （`App::instance_query`，不落盘），见 [`grid::View`]。
pub use grid::View as ViewMode;

// M416：以下三条是搬去 `api` 之后的**原路径**（`pub` 面逐字不变：`pages::*` 的那几条由
// `pages/mod.rs` 继续再导出，`super::forced_state` / `super::adopt_action` 的调用点一行没动）。
pub(crate) use api::adopt_action;
pub(in crate::pages) use api::forced_state;
pub use api::{
    discovered_run_toast, launch_tone, mods_failure_line, saves_delete_prompt, saves_failure_line,
};

use iced::widget::column;
use iced::{Element, Length};

use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_presenter::prefs::InstanceView;

use crate::app::components::buttons::{self, Button};
use crate::app::components::interaction::{Base, Interaction};
use crate::app::components::navigation::list_item::ListItem;
use crate::theme::Ui;

use super::frame::{self, Header};
use super::layout::{FrameMetrics, PageLayout};
use super::model::{Instances, Model, Row};
use super::{Action, Message};

/// 列表行距：**`None` = 这一层不加间距**（页面与护栏**共用这一个真源**）。
///
/// M102 的 ② 就是页级附加量：`ListItem` 自己的 60 高已含上下 8px 内边距，页面再叠一道
/// `Space::X1`(4) 就把行节奏变成 64、与可视高度除不尽（1180×760 下末行被切半行）。
/// 所以行距不写成字面量、也不散落在 `list_column` 里，而是这个常量——
/// 页面 `match` 它，`instances/tests.rs` 也从它算 pitch（改这里，测试立刻红）。
pub(super) const ROW_SPACING: Option<Space> = None;

/// 列表滚动区的**底部内缩**：让裁切线落在卡片的内边距带里，而不是压在 1px 描边上。
///
/// 与 [`ROW_SPACING`] 是一对（行距管「切在哪」、内缩管「切在什么底色上」），
/// 两个数都由 `instances/tests.rs` 的几何断言直接引用。
pub(super) const LIST_BOTTOM_INSET: Space = Space::X2;

/// 整页（`block.detail = None` 时详情区显示空态）。
///
/// 文案全部走 [`Texts`]（`model.lang` 的语言）：这个函数是实例页唯一的取词入口，
/// 页面里不留任何可见英文字面量（守门见 `instances/tests/i18n.rs`）。
pub fn view<'a>(
    ui: &'a Ui,
    metrics: FrameMetrics,
    model: &'a Model,
    form: &'a LaunchForm,
    view: ViewMode<'a>,
) -> Element<'a, Message> {
    let forced = model.forced;
    let texts = Texts::of(model.lang.catalog());
    let mut header = Header::new(texts.title())
        .subtitle(texts.subtitle())
        // M455：视图 chip 组在两种模式下都在（它切的就是模式）；尺寸组只在磁贴模式下出现——
        // 列表模式里尺寸没有任何可见效果，摆一组点了不变的东西比不摆更糟。
        .action(chips::view_chips(ui, texts, view.mode, forced));
    if view.mode == InstanceView::Tiles {
        header = header.action(chips::size_chips(ui, texts, view.tile_size, forced));
    }
    let header = header
        .action(buttons::text(
            ui,
            forced_state(
                Button::new(texts.new_instance(), Message::Action(Action::NewInstance)),
                forced,
            ),
        ))
        .action(buttons::text(
            ui,
            forced_state(Button::new(texts.import(), adopt_action()), forced),
        ));

    let body = match view.mode {
        // M455：磁贴视图——网格吃满整块宽度；有选中项时整页换成详情（没有第二列可放它）。
        InstanceView::Tiles => tiles_body(ui, metrics, model, form, view, texts),
        InstanceView::List => list_body(ui, metrics, model, form, forced, texts, view.query),
    };

    frame::page(ui, metrics, header, body)
}

/// 磁贴视图的正文：网格 / 详情整页 / 空态四选一（M457 多了「有实例但一个都不命中」）。
fn tiles_body<'a>(
    ui: &'a Ui,
    metrics: FrameMetrics,
    model: &'a Model,
    form: &'a LaunchForm,
    view: ViewMode<'a>,
    texts: Texts,
) -> Element<'a, Message> {
    let block = &model.instances;
    let forced = model.forced;
    match &block.detail {
        Some(detail) => detail::detail_page(
            ui,
            metrics,
            detail,
            form,
            &model.mods,
            &model.saves,
            forced,
            texts,
        ),
        // 一条实例都没有（还没扫完 / 超时 / 确实为空）：网格是空的，那一屏该说的话在
        // `detail_side` 里（三态判据见 `pages/instances/tests/cold_boot.rs`），不另写一份。
        //
        // M457：**这一支排在搜索之前**——「还没有实例」与「有实例、但这个词一个都不命中」
        // 是两句不同的话（前者给导入出路，后者给清空关键词），不能因为有关键词就混成一句。
        None if block.rows.is_empty() && block.leaving.is_empty() => {
            frame::body(metrics, detail::detail_side(ui, block, texts))
        }
        // 不套 `frame::body`：那个盒子的宽度是「主列 pane」（list-detail 下 684），而网格要
        // 整页可用宽（见 `grid::tiles` 里 width 的注释）。
        None => {
            let matched = list::drawn(block, view.query).len();
            // 磁贴视图的搜索行坐在**页画布**上（网格没有卡片），所以底是 `Base::CANVAS`。
            // 行宽按药丸自己的上限（G20：`SEARCH_MAX_WIDTH` = 720）定死，不是 `Fill`：
            // 整页 1044 宽时 `Fill` 会把命中数与清空叉甩到页面最右边，离药丸 300px。
            let row = search::search_row(
                ui,
                texts,
                view.query,
                matched,
                forced,
                Base::CANVAS,
                Length::Fixed(crate::app::components::inputs::geometry::SEARCH_MAX_WIDTH),
            );
            // 搜索行**永远在**（空结果时也在）：它是这一屏唯一的清空入口，藏起来等于把用户
            // 关在一张说「没有匹配」的纸上。
            if search::is_active(view.query) && matched == 0 {
                #[cfg(test)]
                grid::note_no_cells();
                return column![row, search::empty(ui, texts, forced)]
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into();
            }
            let height =
                frame::body_height(ui, metrics, model.window_height, true) - search::row_height(ui);
            column![
                row,
                grid::tiles(ui, metrics, block, forced, texts, view, height)
            ]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        }
    }
}

/// 列表视图的正文（M90 起的样子，一行没动）。
fn list_body<'a>(
    ui: &'a Ui,
    metrics: FrameMetrics,
    model: &'a Model,
    form: &'a LaunchForm,
    forced: Option<Interaction>,
    texts: Texts,
    query: &'a str,
) -> Element<'a, Message> {
    let block = &model.instances;
    match metrics.layout {
        PageLayout::ListDetail => frame::panes(
            metrics,
            column::list_column(
                ui,
                frame::body_height(ui, metrics, model.window_height, true),
                block,
                forced,
                texts,
                query,
            ),
            match &block.detail {
                Some(detail) => detail::detail_column(
                    ui,
                    metrics,
                    detail,
                    form,
                    &model.mods,
                    &model.saves,
                    forced,
                    texts,
                ),
                None => detail::detail_side(ui, block, texts),
            },
        ),
        _ => match &block.detail {
            Some(detail) => detail::narrow_detail(
                ui,
                metrics,
                detail,
                form,
                &model.mods,
                &model.saves,
                forced,
                texts,
            ),
            None => frame::body(metrics, detail::detail_side(ui, block, texts)),
        },
    }
}

pub(super) fn is_selected(block: &Instances, entry: &Row) -> bool {
    block
        .detail
        .as_ref()
        .is_some_and(|detail| detail.id == entry.id)
}

/// 一行的 `ListItem` **描述**（不外框）：行高要从它自己的 `lines()` 数出来，所以先建描述、
/// 由 `list` 模块量高再画（`list::column` 里那一句 `geometry.height(item.lines())`）。
///
/// 点一行 = **压一层路由栈**（`Route::Detail(id)`），页面自己不留「选中项」副本——于是
/// 「当前选中」与「返回栈」永远是同一份真相（`presenter::router`）。
///
/// **M313**：这一段的实现从本文件的 `list_row` 搬过来（只改返回值：`Element` → `ListItem`），
/// 所以「卡片内的控件必须把 `frame::BASE` 传下去」这条接线**仍然在本文件**（见
/// `pages/tests/base_wiring.rs` 的计数）。
pub(super) fn row_item<'a>(
    entry: &'a Row,
    selected: bool,
    forced: Option<Interaction>,
) -> ListItem<'a, Message> {
    let mut item = ListItem::new(
        entry.name.as_str(),
        Message::Navigate(unlucky_presenter::router::Route::Detail(entry.id.clone())),
    )
    .supporting(entry.supporting.as_str())
    .leading(IconKind::Instances)
    .trailing(IconKind::ChevronRight);

    if let Some(status) = &entry.status {
        item = item.overline(status.as_str());
    }
    if selected {
        item = item.selected();
    }
    if let Some(state) = forced {
        item = item.state(state);
    }
    // M237 复审 P2-1：列表卡画的是 `MaterialLayer::Card`（`instances/column.rs` 的 style），
    // 行的 hover / 按下状态层按 `base.role()` 合成（`components/navigation/list_item.rs::palette`），
    // 不给底就落回 `Base::CANVAS` —— 卡片里的行抬起时会叠在画布色上，与本轮修的禁用态同一类错。
    item.base(frame::BASE)
}
