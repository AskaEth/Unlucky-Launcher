//! 实例页的**搜索框**（M457）：一条过滤条件，两种视图共用。
//!
//! ## 为什么是「一条」而不是每视图一个
//!
//! 本文件只产三样东西：**判据**（[`matches`]）、**控件**（[`row`]）、**空态**（[`empty`]）。
//! 行 / 格子本身由 `list.rs` 的 [`super::list::drawn`] 筛，所以「列表过滤了、磁贴没过滤」
//! 这类分叉在结构上就不存在——两种摆放**读同一个行源**（M455 定的口径：同一份序，只换摆放）。
//!
//! ## 控件不手搓
//!
//! 复用组件库现成的 `search_bar`（`components/inputs/search.rs`：56 高药丸、
//! `SurfaceContainerHighest`、聚焦 1px `Secondary`）与 `empty_state`（96 图标块 + 标题 + 说明 +
//! 主按钮）。与下载页的版本搜索（`pages/download/rows.rs::search_row`）**同一个形状**：
//! 药丸 + 命中数 + 尾随「清空」图标按钮。
//!
//! ## 关键词是**会话内**状态
//!
//! 它在 `App::instance_query`（`app/state.rs`），**不落盘**、也不进 `pages::Model`（后者每来一条
//! 事件就整棵重建）。切视图、进出详情都**保留**（那是同一屏上的两种摆放）；离开实例页
//! （导航轨去别的页）才清——判据与落点在 `app/state/refresh.rs` 的 `refresh_view`（M461 从
//! 「导航轨 + 页面内 `Navigate` 两条通路」挪过来的：`Route::Agent` / `Route::Content` /
//! 弹窗里「去设置」三处直接 `state.navigate`，挂在那两条上就会漏）。理由：搜索是
//! 「这一屏上我在找什么」，不是偏好；而「从别的页回来还留着一个看不见的过滤条件、列表只剩
//! 两行」是更坏的状态。
//!
//! ## 判据取样口（`#[cfg(test)]`）
//!
//! iced 0.14 的 `Element` 没有几何内省，「这一屏画了几行 / 几个格子」只能由画的那一段自己说
//! （M455 的 `grid::footprint` 是同一个做法）。本文件补的是「空态**真的画了**」——光有
//! 「行数 = 0」区分不开「一句人看得懂的空态」和「一片空白」：后者同样画 0 行。

use iced::widget::{container, row};
use iced::{Alignment, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::app::components::feedback::{empty_state, EmptyState};
use crate::app::components::inputs::{search_bar, SearchBar};
use crate::app::components::interaction::{Base, Interaction};
use crate::theme::Ui;

use super::super::frame;
use super::super::Message;
use super::{InstanceMessage, Texts};

/// 关键词是否**生效**（去掉首尾空白之后非空）。
pub(super) fn is_active(query: &str) -> bool {
    !query.trim().is_empty()
}

/// 一条实例名是否命中关键词：**大小写不敏感的子串**（不是前缀）。
///
/// 为什么是子串：实例名多是「Redstone Lab」这种两段式，用户记得的常是后半段（`lab`），
/// 前缀匹配会让「明明看得见的名字搜不到」。为什么不匹配副文案（`1.20.4 · Fabric`）：
/// 那是版本号搜索的活（下载页 `VersionSearchPlaceholder` 那条），混进来就说不清
/// 「这一行为什么命中」，也让「按名字过滤」这句承诺变成半句。
pub(super) fn matches(name: &str, query: &str) -> bool {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }
    name.to_lowercase().contains(&needle)
}

/// 搜索那一行占的高度 = 药丸自己的 **56**（`LayoutMetrics::search_bar_height`）。
///
/// 间隙不算进来：药丸下面紧跟的那个列头（列表列 / 网格列头）自带 `Space::X3` 12 的上内边距，
/// 那就是「药丸与列头之间」的间距——同一个数只在一处（列头的 padding），这里再加一次会变成
/// 24。所以页面把这条算进「卡片/正文少掉的高度」时只需要减 56：
/// `580 − 56(药丸) − 32(列头) − 8(底部内缩) = 484 ⇒ 8 整行`（`pages/instances/tests.rs`
/// 那条几何判据引用本函数现算，改这里立刻红）。
pub(super) fn row_height(ui: &Ui) -> f32 {
    ui.metrics().search_bar_height
}

/// 搜索条那一行：药丸（内含尾随清空叉）+ （有关键词时）命中数。
///
/// 名字带 `search_`：本文件同时 `use iced::widget::row`（函数与宏），函数叫 `row` 会与那条
/// `use` 撞名（E0255）。下载页同一个控件叫 `rows::search_row`，这里跟着它。
///
/// 命中数只在**有关键词**时出现：没有过滤时它等于列头那个计数，同屏两个一样的数字读起来
/// 像两个不同的量。
///
/// **清空叉在药丸里面**（M460）：M457 把它画在这一行的尾巴上，两种档位下都与输入框脱开一段
/// （列表档 250 宽的药丸、磁贴档 660 宽的药丸，叉都在右外侧），看着像「飘在旁边的按钮」。
/// MD3 搜索栏的惯例是**输入框内尾随位**，所以改由 [`SearchBar::trailing_action`] 接这条消息，
/// 本文件不再自己造按钮（组件把它画在 `Filter` 图标之后、同一个输入行里）。
///
/// `base` = 药丸**脚下那一层**（M234）：列表视图里它坐在卡片上（`frame::BASE`），磁贴视图里
/// 坐在页画布上（`Base::CANVAS`）。只有禁用态的合成用它（那 12% 是叠在父底上的），但接错过
/// 一次就再也看不出来——所以做成必填形参，不默认。
///
/// `width` = 这一行多宽：列表卡片里 `Fill`（卡片只有 336），磁贴视图里
/// `Fixed(SEARCH_MAX_WIDTH)`——那里整页可用宽 1044，让药丸吃满会把命中数甩到页面的最右边
/// （离药丸 300px，读起来不像同一个控件）。
pub(super) fn search_row<'a>(
    ui: &'a Ui,
    texts: Texts,
    query: &'a str,
    matched: usize,
    forced: Option<Interaction>,
    base: Base,
    width: Length,
) -> Element<'a, Message> {
    #[cfg(test)]
    note_empty(false);

    let mut spec = SearchBar::new(query)
        .placeholder(texts.search_placeholder())
        .base(base);
    // 非空时才有尾随动作：空药丸上没有东西可清（MD3 同一条）。
    //
    // 禁用态这一支沿用 M457 的口径：**只有禁用态**才拿掉落点（`--force-state` 的
    // `rest`/`hover`/`pressed` 是取证用的视觉档，不是「按不动」；写成 `forced.is_none()` 会让
    // 截图通道默认的 `rest` 里叉永远灰着——M457 出图时就是这么发现的）。组件侧的表现是
    // 「不画这个按钮」，而不是画一个按不动的图标。
    if is_active(query) && !forced.is_some_and(|state| state.is_disabled()) {
        spec = spec.trailing_action(clear_message());
    }
    let bar = search_bar(ui, spec, |text| {
        Message::Instances(InstanceMessage::Search(text))
    });

    let mut line = row![container(bar).width(Length::Fill)]
        .spacing(ui.space(Space::X2))
        .align_y(Alignment::Center)
        .width(width);

    if is_active(query) {
        line = line.push(frame::text_role_owned(
            ui,
            TypeRole::LabelMedium,
            false,
            matched.to_string(),
            ui.color(Role::OnSurfaceVariant),
        ));
    }

    line.into()
}

/// 清空叉按下去发的那条消息（M460 判据 ③ 读它：页面接哪条消息不是靠读源码猜的）。
pub(super) fn clear_message() -> Message {
    Message::Instances(InstanceMessage::ClearSearch)
}

/// 「一个都不命中」那一屏：一句说清的空态 + 一个**真能清掉关键词**的主按钮。
///
/// 高度是 `Fill`：调用方把它放进「搜索行下面的那块」里（列表视图 = 卡片里让出来的可视区，
/// 磁贴视图 = 网格那一块），空态在它里面居中（`empty_state` 自己的口径，与
/// `download/states.rs::no_match` 同一条）。
///
/// 为什么空态里还要一个「清空搜索」按钮（药丸右边已经有一个叉）：用户在这一屏看到的是
/// 「什么都没有」，眼下的下一步必须是**一个写着字**的按钮，而不是让他回去找那个小图标。
pub(super) fn empty<'a>(
    ui: &'a Ui,
    texts: Texts,
    forced: Option<Interaction>,
) -> Element<'a, Message> {
    #[cfg(test)]
    note_empty(true);

    let mut spec = EmptyState::new(
        IconKind::Search,
        texts.search_empty_title(),
        texts.search_empty_body(),
    );
    if !forced.is_some_and(|state| state.is_disabled()) {
        spec = spec.action(
            texts.search_clear(),
            Message::Instances(InstanceMessage::ClearSearch),
        );
    }

    container(empty_state(ui, spec))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::Center)
        .into()
}

// ---------------------------------------------------------------- 判据取样口

// 上一次渲染里「空态画了吗」。**线程局部**而不是 `static`：用例并行跑，进程级的槽会让两条
// 用例互相覆盖（`instances/grid.rs` 的 `LAST` 是同一个教训）。
#[cfg(test)]
thread_local! {
    static LAST_EMPTY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn note_empty(drawn: bool) {
    LAST_EMPTY.with(|cell| cell.set(drawn));
}

/// 上一次渲染是否画了「没有匹配」空态（[`row`] 每次渲染都先清成 `false`）。
#[cfg(test)]
pub fn empty_drawn() -> bool {
    LAST_EMPTY.with(std::cell::Cell::get)
}

#[cfg(test)]
mod tests;
