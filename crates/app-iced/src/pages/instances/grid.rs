//! 实例页的**磁贴网格**（M455）：`ui.instances.view = tiles` 时列表那一块的画法。
//!
//! ## 为什么单独一卷（以及它与 `list.rs` 的分工）
//!
//! | 模式 | 画什么 | 行/格的序与动效 |
//! |---|---|---|
//! | `InstanceView::List` | `list.rs` 的**一列** `ListItem`（list-detail 的左列，336 固定） | 泵给的序 + 增删动效 |
//! | `InstanceView::Tiles` | 本文件：同一批 `ListItem`，**一行 `tile_size.columns()` 个** | 同一份序（[`super::list::drawn`]），按格宽分行 |
//!
//! 「用 `ListItem` 当格子」是有意的：实例页已有的行语汇就是它（`navigation::list_item`），
//! 卡片/磁贴另造一套会让同一份数据在同一页有两种字号、两种状态层。所以网格**不新造控件**，
//! 只把**摆放**从「一列」换成「N 列」——宽度、行高、状态层全部仍由组件自己的几何说话。
//!
//! ## 列数只有一个真源：[`TileSize::columns`]
//!
//! `Small 6 / Medium 4 / Large 3` 写在 `presenter::prefs::TileSize`（Q24-3 / D16），本文件**不夹取、
//! 不按可用宽二次计算**：容器宽度不够时格子变窄（`ListItem` 的文本会裁切），**不会**偷偷少画一列。
//! 少了这一条，「6 列」就变成「宽的时候 6 列」，而 `tile_size` 的语义是**用户选的密度**。
//! 判据在 `pages/instances/tests/view_mode.rs`：渲染一遍再读 [`footprint`]，列数必须逐档等于
//! `columns()`（6 / 4 / 3 三档各不相同）。
//!
//! ## 三档尺寸的**内容**差（枚举文档的那三行，不是新设计）
//!
//! | 档 | 列数 | 格子里的行 | 行高（组件几何） |
//! |---|---|---|---|
//! | `Small` | 6 | 图标 + 名字 | 44 |
//! | `Medium` | 4 | + 副文案（`1.20.4 · Fabric`） | 60 |
//! | `Large` | 3 | + 运行状态 overline | 76 |
//!
//! 行高与「哪几行」都从 [`ListItemGeometry`] 与 `ListItem::lines()` 取，不写死数字；**格子高度
//! 按档统一**（`Large` 里没有运行状态的那几条只有两行，不统一高度的话同一排格子高低不齐）。
//!
//! ## 动效（M461）：逐格淡入，**复用列表那一套原语**
//!
//! | 这件事 | 落点 |
//! |---|---|
//! | 这一格此刻多不透明 | `motion::rows::frame(List::Instances, id)`（M305/M313 的同一份采样） |
//! | 怎么画 | `motion::revealed`（下载页 / 列表页同一个控件）：一层**与画布同色**的遮罩，`α = 1 − 可见度` |
//! | 什么时候起跑 | 行增删走 `observe_rows`（M313 已有）；切视图 / 换尺寸走 `motion::rows::relayout`（M461） |
//!
//! **网格与列表的差在哪**（原语是按单列设计的，这里说清为什么还能直接用）：列表的 `revealed`
//! 同时收**高度**（裁切框从 0 长到行高，下面那些行随之逐帧滑上来）；网格是**一行多个格子**，
//! 收高度会让同一排的格子高低不齐（`Row` 的高度取最高那一个），滚动区总高也跟着抖。所以磁贴档
//! 只取这个原语的**淡**那一半：`visible_height == content_height`（不裁切、不重排），只让遮罩走
//! `α = 1 − 可见度`。控件、遮罩口径、D27 的 α 纪律全是同一个 `revealed`，**没有新造第二套**；
//! `offset_y` 给 `0` 也是同一个理由（格高定死时纵向位移会把格子内容切掉）。
//!
//! 静息（可见度严格 `1.0`）时 `veil = None` ⇒ **一个 quad 都不画**，出图与 M455 逐像素相同。

// 子模块落在同名的 `grid/` 目录（与 `pages/instances/tests.rs` 同一形状，不用 `#[path]`）。
#[cfg(test)]
mod probe;

use iced::widget::{column, container, scrollable, Row};
use iced::{Alignment, Element, Length};

use unlucky_design::elevation::MaterialLayer;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_presenter::prefs::{InstanceView, TileSize};
use unlucky_presenter::router::Route;

use crate::app::components::interaction::Interaction;
use crate::app::components::motion;
use crate::app::components::navigation::geometry::ListItemGeometry;
use crate::app::components::navigation::list_item::{list_item, ListItem};
use crate::theme::Ui;

use super::super::frame;
use super::super::layout::block::BODY_MAX_WIDTH;
use super::super::layout::FrameMetrics;
use super::super::model::{Instances, Row as InstanceRow, ScanPhase};
use super::super::Message;
use super::is_selected;
use super::Texts;

/// 页面视图模式 + 磁贴尺寸（M455）+ 搜索关键词（M457）：`ui.instances.view` /
/// `ui.instances.tile_size` 的**当帧值**，以及这一帧生效的过滤条件。
///
/// 为什么是页面层自己的一对值而不是直接读 presenter：页面层的入参是 [`super::super::Model`]
/// （`pages/model.rs` 是页面与 presenter 之间的展示契约），而这两个键是**用户档位**，与
/// `pages::settings::States` 同一类（运行期状态，由 `App` 持有）。所以它跟着页面入口递进来。
///
/// **M457 的 `query` 是借来的**（`&'a str`）：它指向 `App::instance_query`，**不落盘**、不进
/// `Model`。三个字段一起递进来而不是另开一个形参：`pages/mod.rs` 恰好 400 行零余量，加一条
/// 形参要动 `page` / `page_with_settings` / `page_with_settings_transition` / `render` / `body`
/// 五处（而它们只做转发）。字段类型仍是 `Copy`（`&str` 是 Copy），所以 `render` 里给退场幽灵
/// 再传一次不用克隆。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct View<'a> {
    /// 视图模式（`Tiles` = 网格，`List` = 一列）
    pub mode: InstanceView,
    /// 磁贴尺寸（只有 `Tiles` 有可见效果）
    pub tile_size: TileSize,
    /// 搜索关键词（空串 = 不过滤；**两种视图都生效**，见 `instances/search.rs`）
    pub query: &'a str,
}

impl Default for View<'_> {
    /// **`List` + `Medium` + 无关键词**：harness（`pages::app` / `pages_demo`）与页面单测走这一档，
    /// 于是「不传这个参数」= M90 起的 list-detail 基线，出图基线一像素不动。
    ///
    /// 产品路径**不用**这个默认值：`app/view.rs` 每次都把盘上的档位（出厂 `Tiles` / `Medium`，
    /// 见 `UiKeys::default`）与 `App::instance_query` 显式递进来。两者不同是有意的——D16 的
    /// 产品默认是磁贴网格。
    fn default() -> Self {
        Self {
            mode: InstanceView::List,
            tile_size: TileSize::Medium,
            query: "",
        }
    }
}

/// 格与格、排与排之间的间距（`Space::X3` 12；与元信息网格同档，不另立一个数）。
const GRID_GAP: Space = Space::X3;

/// 网格列头的高度（`ALL INSTANCES` + 计数那一行）：上 `X3` 12 + `LabelMedium` 16 + 下 `X1` 4 = **32**。
const HEAD_HEIGHT: f32 = 32.0;

/// 这一帧**真的画出来的**形态（M455 的判据取样口）。
///
/// 为什么要有它：iced 0.14 的 `Element` 没有几何内省（`pages/instances/tests.rs` 里那条
/// 「为什么读源码」解释过同一件事），所以「这一屏画了几列」只能由**画的那一段自己**说出来。
/// 它不是「配置回显」——[`note`] 记的是**分行之后第一排的格子数**与排数；把 `chunks(columns)`
/// 改成别的数、或者在 `List` 模式里偷画网格，取样立刻变。
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Footprint {
    /// `Tiles` 模式：第一排的格子数（= 真画出来的列数）；`List` 模式：**0**
    pub columns: usize,
    /// `Tiles` 模式：画了几排
    pub grid_rows: usize,
    /// `List` 模式：画了几行（`Tiles` 模式为 0）
    pub list_rows: usize,
    /// 这一屏真画出来的**格子总数**（`Tiles` 模式；`List` 模式为 0）。
    ///
    /// M457 加的：搜索的判据要「格子数 = 0」，而 `grid_rows` 在「一排填不满」时对不上
    /// （12 条实例 / 4 列 ⇒ 3 排，格子却只有 12 个）。它和 `list_rows` 是同一件事的两种摆放。
    pub cells: usize,
}

// 上一帧的取样（写入口只在本文件；读端只有用例）。
//
// **线程局部**而不是 `static Mutex`：用例是并行跑的，一个进程级的取样口会让四条版式用例互相
// 覆盖（M455 第一版就是 `static`：`List` 那条写下的 `columns: 0` 把 `Tiles` 那条读到的列数冲掉）。
// 渲染与读取样都在同一个测试线程里发生，所以线程局部正好是「这一次渲染」的语义。
#[cfg(test)]
thread_local! {
    static LAST: std::cell::RefCell<Option<Footprint>> = const { std::cell::RefCell::new(None) };
}

/// 记下这一帧真画出来的网格形态（只在测试构建里存在：产品路径不留任何状态）。
#[cfg(test)]
fn note(columns: usize, grid_rows: usize, cells: usize) {
    LAST.with_borrow_mut(|last| {
        *last = Some(Footprint {
            columns,
            grid_rows,
            list_rows: 0,
            cells,
        });
    });
}

/// 记下 `List` 模式画出来的行数（列数按 0 记——这一屏没有网格）。
#[cfg(test)]
pub(super) fn note_list_rows(rows: usize) {
    LAST.with_borrow_mut(|last| {
        *last = Some(Footprint {
            columns: 0,
            grid_rows: 0,
            list_rows: rows,
            cells: 0,
        });
    });
}

/// 记下「这一帧一个格子都没画」（M457：搜索空结果那一屏走的就是这一支——它根本不建网格）。
///
/// 为什么不靠「`note` 没被调用」来表示 0：上一个用例的取样会留在槽里，于是「0 个格子」与
/// 「这次没画网格」分不开（`tests/view_mode.rs` 的空快照那条正是靠这个「没被改写」的语义）。
#[cfg(test)]
pub(super) fn note_no_cells() {
    note(0, 0, 0);
}

/// 上一帧画出来的形态（用例的取样口）。`None` = 还没渲染过。
#[cfg(test)]
#[must_use]
pub fn footprint() -> Option<Footprint> {
    LAST.with_borrow(|last| *last)
}

/// 磁贴网格整块（列头 + 可滚动的格子区）。
///
/// 名字不叫 `column`：`iced::widget::column` 那个绑定同时**引入同名宏**（本文件与 `instances.rs`
/// 都靠 `column![…]` 建树），函数重名会把宏遮掉（E0255 之外还会让 `column![head, scroller]`
/// 解析成函数）。
pub(super) fn tiles<'a>(
    ui: &'a Ui,
    metrics: FrameMetrics,
    block: &'a Instances,
    forced: Option<Interaction>,
    texts: Texts,
    view: View<'_>,
    body_height: f32,
) -> Element<'a, Message> {
    let geometry = ListItemGeometry::of(ui);
    let gap = ui.space(GRID_GAP);
    let size = view.tile_size;
    let columns = size.columns();
    let cell_height = geometry.height(lines_of(size));
    // 整块宽度 = **整页可用宽**（`available`，封顶 1440），格子宽度按列数均分。
    //
    // 为什么不是 `frame::body_width`：那个量在 list-detail 版面下是**主列 pane**（1180 窗宽下
    // 684 = 可用 1044 − 列表列 336 − 间隙 24），磁贴视图里没有列表列，拿它就等于把 6 列挤进
    // 684px（M455 第一版正是这么画的：格宽 106、名字被裁到两个字）。`frame::body_width` 的
    // 口径属于「哪块 pane」，而网格占的是**整页**。
    let width = metrics.available.min(BODY_MAX_WIDTH);
    let cell_width = ((width - gap * (columns.saturating_sub(1)) as f32) / columns as f32).max(1.0);

    // M457：行源与列表视图**同一个**（`list::drawn` 已经按关键词筛过），这里只负责分行。
    let drawn = super::list::drawn(block, view.query);
    #[cfg(test)]
    probe::reset();
    let mut grid = column![].spacing(gap).width(Length::Fill);
    for chunk in drawn.chunks(columns) {
        let mut line = Row::new()
            .spacing(gap)
            .align_y(Alignment::Start)
            .width(Length::Fill);
        for entry in chunk {
            line = line.push(cell(
                ui,
                entry,
                is_selected(block, entry),
                forced,
                size,
                (cell_width, cell_height),
            ));
        }
        grid = grid.push(line);
    }
    // 判据取样口：记的是**分行之后**第一排的格子数，不是 `columns` 这个入参；格子总数另记一格
    // （M457：搜索的判据要「画出来的格子数」，一排填不满时它和 `grid_rows × columns` 不等）。
    #[cfg(test)]
    note(
        drawn
            .chunks(columns)
            .next()
            .map_or(0, <[&InstanceRow]>::len),
        drawn.chunks(columns).count(),
        drawn.len(),
    );

    let scroller = scrollable(grid)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new()
                .width(frame::SCROLLBAR_WIDTH)
                .scroller_width(frame::SCROLLBAR_WIDTH)
                .margin(frame::SCROLLBAR_MARGIN),
        ))
        .height(Length::Fixed(viewport_height(
            body_height,
            cell_height + gap,
        )))
        .style(frame::scroll_style(ui));

    column![
        // 列头是**两档共用的那一个**（M460）：磁贴档不再自己拼 `Fill` 标题 + 计数——
        // 那个写法把计数顶到页面右缘（x≈1090），与列表档像两套规矩。函数在 `column.rs`。
        super::column::header(
            ui,
            texts,
            (block.scan == ScanPhase::Ready).then_some(drawn.len())
        ),
        scroller
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// 网格可视区的定高：正文可用高 − 列头，再按**整排**取整（M357 / P2-2 的同一条纪律：
/// 裁切线不许落在某一排的格子里）。
fn viewport_height(body_height: f32, pitch: f32) -> f32 {
    frame::whole_rows((body_height - HEAD_HEIGHT).max(pitch), pitch)
}

/// 这一档尺寸在格子里画几行（见模块文档那张表）。
fn lines_of(size: TileSize) -> usize {
    match size {
        TileSize::Small => 1,
        TileSize::Medium => 2,
        TileSize::Large => 3,
    }
}

/// 一个格子：与列表行**同一个组件**（`ListItem`），只是位面不同。
///
/// 与 `instances.rs::row_item` 的差别只有两处，都是「格子放不下」造成的：
/// ① 没有尾随的 `ChevronRight`（格宽窄，箭头会把名字挤出裁切线）；
/// ② `Large` 的 overline 在没有运行状态时给空串——它仍占一行，于是同一档的格子高度一致。
fn cell<'a>(
    ui: &'a Ui,
    entry: &'a InstanceRow,
    selected: bool,
    forced: Option<Interaction>,
    size: TileSize,
    (width, height): (f32, f32),
) -> Element<'a, Message> {
    let mut item = ListItem::new(
        entry.name.as_str(),
        Message::Navigate(Route::Detail(entry.id.clone())),
    )
    .leading(IconKind::Instances);

    if size != TileSize::Small {
        item = item.supporting(entry.supporting.as_str());
    }
    if size == TileSize::Large {
        item = item.overline(entry.status.as_deref().unwrap_or_default());
    }
    if selected {
        item = item.selected();
    }
    if let Some(state) = forced {
        item = item.state(state);
    }

    // 格宽靠外层定死（`ListItem` 自己是 Fill），高度按档统一（见模块文档最后一段）。
    //
    // M461：**每一格**按泵给的可见度淡入。`visible_height == height`（不裁切、不重排），
    // 只让遮罩走 `α = 1 − 可见度`；静息时 `veil = None` ⇒ 一个 quad 都不画（见模块文档「动效」）。
    // 遮罩颜色取**这一格脚下的画布**（磁贴没有卡片，`tiles_body` 直接把网格放在页画布上）。
    let visibility = motion::rows::frame(motion::rows::List::Instances, entry.id.as_str())
        .visibility
        .clamp(0.0, 1.0);
    let veil = (visibility < 1.0).then(|| iced::Color {
        a: 1.0 - visibility,
        ..ui.layer(MaterialLayer::Canvas)
    });
    #[cfg(test)]
    probe::note(visibility);

    container(motion::revealed(
        list_item(ui, item),
        height,
        height,
        0.0,
        veil,
    ))
    .width(Length::Fixed(width))
    .height(Length::Fixed(height))
    .align_y(Alignment::Center)
    .into()
}

#[cfg(test)]
mod tests;
