//! 实例页**列表视图的左列卡片**（M457 从 `instances.rs` 拆出：那边加完搜索行后到 450 行）。
//!
//! ## 这一卷画什么
//!
//! 一张 Card 面：**搜索行** + 列头（`ALL INSTANCES` + 计数）+ 滚动区（键控列，行来自
//! `list::drawn` = 已经按关键词筛过的那一份）。
//!
//! | 情形 | 画什么 | 为什么 |
//! |---|---|---|
//! | 有命中 | 搜索行 + 列头 + 滚动区 | — |
//! | **一个都不命中** | 搜索行 + 列头 + **空态**（`search::empty`） | 一片空白既不说「没有匹配」，也不给清空的出路 |
//! | 快照里一条实例都没有 | 搜索行 + 列头 + 空滚动区 | 「还没有实例」那一屏的话在详情列（`detail_side` 的三态），这里不抢它的台词 |
//!
//! ## 两处几何真源（改这里会让判据红）
//!
//! 1. **行距**只从 [`ROW_SPACING`] 来（当前 `None` = 不加间距）。别在这条链上写字面量
//!    `.spacing(…)`：那正是 M102 视觉复审 ② 的页级附加量（`tests.rs` 有一条源码结构断言
//!    盯着本文件的 `list_column`）。
//! 2. **滚动视口**按整行取高（`list::viewport_height`），其中「卡片高度 − 搜索行 − 列头 −
//!    底部内缩」的算式全在那一处；本文件只把它当 `Length::Fixed` 用。
//!
//! 卡片自己画的是 `MaterialLayer::Card`（M237），所以卡里的行把 `frame::BASE` 传下去
//! （`instances.rs::row_item` 的 `item.base(frame::BASE)`）——同一层不出现两个「真源」。

use iced::widget::{column, container, row, scrollable};
use iced::{Alignment, Background, Border, Element, Length, Padding};

use unlucky_design::color::Role;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::app::components::interaction::Interaction;
use crate::theme::Ui;

use super::super::frame;
use super::super::model::{Instances, ScanPhase};
use super::super::Message;
use super::{list, search, Texts, LIST_BOTTOM_INSET, ROW_SPACING};

/// 列表视图的左列卡片（list-detail 的左 pane；窄窗下退成上一屏）。
pub(super) fn list_column<'a>(
    ui: &'a Ui,
    body_height: f32,
    block: &'a Instances,
    forced: Option<Interaction>,
    texts: Texts,
    query: &'a str,
) -> Element<'a, Message> {
    // M457：这一屏真画出来的行（= 命中数）。列头的计数与画出来的行**同一份来源**，
    // 于是「列头写 12、屏上 3 行」这种自相矛盾不可能出现。
    let matched = list::drawn(block, query).len();
    // 判据取样口（M455 / M457）：**这一帧画出来的行数**。放在这里而不是 `list::column` 里，
    // 因为「一个都不命中」时那一列根本不进树（空态替掉了它）——取样若在那边，会停在上一次
    // 渲染的数字上，于是「输 xyz ⇒ 0 行」这条判据假绿。
    #[cfg(test)]
    super::grid::note_list_rows(matched);
    // M302：**未就绪时不许写 0**（复审 r1 P2-1：计数 0 与「还没有实例」是同一句假话的两半）。
    // 藏起来而不是写「—」：破折号读起来像「零个」，而这一刻根本没有数可给。
    let count = (block.scan == ScanPhase::Ready).then(|| {
        if search::is_active(query) {
            matched
        } else {
            block.rows.len()
        }
    });
    // 列头是**两档共用的那一个**（M460，见 [`header`]）：列表档只是把标题换成它、宽度交给卡片。
    let head = header(ui, texts, count);

    let mut rows = column(vec![list::column(ui, block, forced, query)])
        .padding(Padding::from([0.0, ui.space(Space::X2)]))
        .width(Length::Fill);

    // 行距只从 `ROW_SPACING` 来（当前是 None = 不加间距）。别在这里写字面量 `.spacing(…)`：
    // 那正是视觉复审 ② 的那道页级附加量（`tests.rs` 有一条源码结构断言盯着这段）。
    if let Some(spacing) = ROW_SPACING {
        rows = rows.spacing(ui.space(spacing));
    }

    // **独立滚动**：只有列表列这一条 scrollable，详情列不跟着滚。
    // M357（P2-2）：高度按**整行**取（`list::viewport_height`，抄版本管理页的做法）；修前是
    // `Fill`，880×560 的裁切线落在第 5 行中间。代价：不足一行的余量留在卡片底部。
    let viewport = list::viewport_height(ui, block, forced, body_height, query);
    let scroller = scrollable(rows)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new()
                .width(frame::SCROLLBAR_WIDTH)
                .scroller_width(frame::SCROLLBAR_WIDTH)
                .margin(frame::SCROLLBAR_MARGIN),
        ))
        .height(if viewport > 0.0 {
            Length::Fixed(viewport)
        } else {
            Length::Fill
        })
        .style(frame::scroll_style(ui));

    // 底部内缩：裁切线落在卡片的内边距带里，而不是压在圆角描边上（M102 / 视觉复审 ②）。
    //
    // M237：这一层画的是 `MaterialLayer::Card`（= `frame::BASE` 的同一个层），所以卡里的
    // 列表行把底传 `frame::BASE`（见 `instances.rs::row_item`）——不另立常量，避免同一层出现
    // 两个「真源」。
    // 搜索行在**卡片之内、列头之上**（与下载页 `column![search_row, divider, head, body]`
    // 同一个位置感）：它筛的就是这张卡里的行。左右内边距与列头同为 `Space::X3`，两者的左缘
    // 对齐；药丸与列头之间的间距是列头自己的上内边距（见 `search::row_height`）。
    //
    // **顺序有判据含义**：取样口（`search::empty_drawn`）由这一行清成 `false`、由下面的空态
    // 置成 `true`，所以搜索行必须先建。反过来写会让「画了空态」被清成「没画」。
    let search_row = container(search::search_row(
        ui,
        texts,
        query,
        matched,
        forced,
        frame::BASE,
        Length::Fill,
    ))
    .padding(Padding {
        top: 0.0,
        right: ui.space(Space::X3),
        bottom: 0.0,
        left: ui.space(Space::X3),
    });

    // M457：**一个都不命中**时，滚动区换成一句说清的空态（卡片留在原位、列头照画）。
    // 空态里那个「清空搜索」按钮是这一屏的主出路；药丸与它的清空叉还在上面一行。
    let body: Element<'a, Message> = if search::is_active(query)
        && matched == 0
        && !(block.rows.is_empty() && block.leaving.is_empty())
    {
        search::empty(ui, texts, forced)
    } else {
        scroller.into()
    };

    container(column![search_row, head, body].height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding::default().bottom(ui.space(LIST_BOTTOM_INSET)))
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

/// 列头：`全部实例` + 计数——**列表档与磁贴档共用这一个**（M460）。
///
/// ## 为什么必须是一个函数
///
/// M457 有两份手抄的列头：列表档那份的行只有 336 宽（卡片），`Fill` 的标题容器把计数顶到卡片
/// 右缘；磁贴档那份的行是整页（1044），同一个 `Fill` 把计数顶到**页面右缘**（x≈1090），离
/// 「全部实例」一千像素。于是同一页的两个视图看起来像两套规矩。收成一处之后，
/// 「两档对齐方式不同」在结构上不可能——除非有人再抄一份。
///
/// ## 对齐方式：计数紧贴标题（左对齐的标题组）
///
/// 标题容器是 **`Shrink`**，不是 `Fill`：`row` 里没有撑开的空隙，`全部实例` 与计数就是一个
/// 紧凑的组合。这是任务书允许的两选之一，选它的理由是**两档都要好看**：磁贴档的行宽等于整页，
/// 若选「右对齐到行尾」，计数就必然回到 x≈1090（那就是被投诉的现状）；列表档同理会被推到
/// 卡片右缘（x≈433，M457 的图里就是这个位置）。`Shrink` 在两种行宽下读数一致。
///
/// 取样口 [`head_shape`] 记的正是「标题容器是 `Shrink` 还是 `Fill`」这个决定本身（M460 判据 ②）。
pub(super) fn header<'a>(ui: &'a Ui, texts: Texts, count: Option<usize>) -> Element<'a, Message> {
    // 这一行是「撑开」还是「紧凑」的唯一开关：取样口读它，改它会让判据 ② 立刻红。
    let title_width = Length::Shrink;
    #[cfg(test)]
    note_head(HeadShape {
        title_width_fill: matches!(title_width, Length::Fill),
        count_shown: count.is_some(),
    });

    let mut line = row![container(frame::text_role(
        ui,
        TypeRole::LabelMedium,
        false,
        texts.all_title(),
        ui.color(Role::OnSurfaceVariant),
    ))
    .width(title_width)
    .align_y(Alignment::Center)]
    .width(Length::Fill)
    .align_y(Alignment::Center)
    // 紧凑组里两者要有一点呼吸：`Shrink` 之后标题与计数会贴在一起（「全部实例1」）。
    .spacing(ui.space(Space::X2));
    if let Some(count) = count {
        line = line.push(frame::text_role_owned(
            ui,
            TypeRole::LabelMedium,
            false,
            count.to_string(),
            ui.color(Role::OnSurfaceVariant),
        ));
    }
    container(line)
        .width(Length::Fill)
        .padding(Padding {
            top: ui.space(Space::X3),
            right: ui.space(Space::X3),
            bottom: ui.space(Space::X1),
            left: ui.space(Space::X3),
        })
        .into()
}

// ---------------------------------------------------------------- 判据取样口

// 上一次 [`header`] 调用的形态。**线程局部**：用例并行跑（`grid.rs` 的 `LAST` 是同一个教训）。
#[cfg(test)]
thread_local! {
    static LAST_HEAD: std::cell::Cell<Option<HeadShape>> = const { std::cell::Cell::new(None) };
}

/// 这一帧列头的形态（M460 判据 ②）：两档渲染出来的它必须**相等**。
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeadShape {
    /// 标题容器是不是 `Fill`（`true` = 它把计数顶到行尾，也就是 M457 磁贴档的形态）
    pub title_width_fill: bool,
    /// 这一帧有没有画计数（`ScanPhase::Ready` 之外没有数可给）
    pub count_shown: bool,
}

#[cfg(test)]
fn note_head(shape: HeadShape) {
    LAST_HEAD.with(|cell| cell.set(Some(shape)));
}

/// 上一次 [`header`] 的形态；`None` = 这一帧**根本没画列头**。
#[cfg(test)]
#[must_use]
pub fn head_shape() -> Option<HeadShape> {
    LAST_HEAD.with(std::cell::Cell::get)
}

/// 丢掉上一次的取样（用例逐帧读：不清的话「磁贴档没画列头」会读到列表档留下的值）。
#[cfg(test)]
pub fn forget_head() {
    LAST_HEAD.with(|cell| cell.set(None));
}
