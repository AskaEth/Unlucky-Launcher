//! M460 的两条**观感判据**（清空叉在药丸里 / 两档列头计数同一对齐）。
//!
//! ## 为什么这两条落在这一卷
//!
//! M457 的搜索判据在 `pages/instances/tests/search.rs`（不在 M460 的 scope）里，盯的是「过滤
//! 真的生效」；M460 盯的是**同一批元素画在哪**，取样口也只有两个：`components/inputs/search`
//! 的尾随位取样与 `instances/column.rs::head_shape`。两处都是「画的那一段自己说」，
//! 与 M455 的 `grid::footprint` / M457 的 `search::empty_drawn` 同一条做法（`Element` 在 iced
//! 0.14 没有几何内省）。
//!
//! | 用例 | 钉住 |
//! |---|---|
//! | [`both_views_align_the_head_count_the_same_way`] | 判据 ②：两档列头的形态**相等**，且都是「计数紧贴标题」 |
//! | [`the_clear_action_is_handed_to_the_bar_not_drawn_beside_it`] | 判据 ①③：清空叉由药丸接过去画（页面不再外挂按钮），接的消息是 `ClearSearch` |

use unlucky_design::color::SchemeFlavor;
use unlucky_design::typography::Platform;
use unlucky_presenter::dto::LaunchArgsView;
use unlucky_presenter::prefs::{InstanceView, TileSize};
use unlucky_presenter::router::NavSection;

use crate::pages::layout::{self, PaneKit};
use crate::pages::model::{Instances, Row, ScanPhase};
use crate::pages::{LaunchForm, Model, Screen};
use crate::theme::{Families, Ui};

use super::super::grid::{self, View};
use super::{clear_message, is_active};
use crate::pages::Message;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

/// 演示清单的条数（`demo-0` … `demo-11`）。
const ROWS: usize = 12;

fn ui() -> Ui {
    Ui::with_platform(SchemeFlavor::Light, Platform::Windows, FAMILIES)
}

fn model() -> Model {
    let mut model = Model::new(
        Screen::Instances,
        1180.0,
        760.0,
        Some(NavSection::Instances),
    );
    model.instances = Instances {
        rows: (0..ROWS)
            .map(|index| Row {
                id: format!("i{index}"),
                name: format!("demo-{index}"),
                supporting: "1.20.4 · Fabric 0.16.5".to_string(),
                status: None,
            })
            .collect(),
        detail: None,
        scan: ScanPhase::Ready,
        leaving: Vec::new(),
    };
    model
}

/// 渲染一屏实例页（带关键词）。取样在构建期发生，树建完就可以丢掉。
fn render(mode: InstanceView, query: &str) {
    let ui = ui();
    let form = LaunchForm::from_view(None, &LaunchArgsView::default());
    let model = model();
    let metrics = layout::frame_metrics(PaneKit::ListDetail, model.window_width);
    let view = View {
        mode,
        tile_size: TileSize::Medium,
        query,
    };
    let element = super::super::view(&ui, metrics, &model, &form, view);
    drop(element);
}

/// 跑一帧再读列头形态。**先清取样**：不清的话「磁贴档没画列头」会读到列表档留下的值（假绿）。
fn head_shape_after(mode: InstanceView, query: &str) -> super::super::column::HeadShape {
    super::super::column::forget_head();
    render(mode, query);
    super::super::column::head_shape().expect("这一帧根本没画列头")
}

/// **判据 ②**：两种视图的列头计数**对齐方式相同**，且都不是「被 `Fill` 顶到行尾」。
///
/// 抓的是 M457 的形态：磁贴档的标题容器是 `Fill`，计数于是落在**页面右缘**（x≈1090，离
/// 「全部实例」一千像素），而列表档同样写法的计数落在卡片右缘（x≈433）——同一页两套规矩。
/// 现在两档共用 `column::header`，取样口读的正是那个「标题容器是 `Shrink` 还是 `Fill`」的
/// 决定本身。
///
/// ## 变异与期望红（每条只改一处）
///
/// | 变异 | 改哪里 | 期望红在哪句 |
/// |---|---|---|
/// | 计数被顶回行尾（M457 的形态） | `column::header` 的 `let title_width = Length::Shrink;` → `Length::Fill` | 第二条断言（`title_width_fill` 为真） |
/// | 磁贴档自己抄一份 `Fill` 列头 | `grid::tiles` 改回自建 `row![container(…).width(Fill), count]` | 第一条（两档形态不等）；且 `grid` 不再调 `column::header` ⇒ `head_shape()` 为 `None`、`expect` 直接炸 |
/// | 磁贴档干脆不画列头 | `grid::tiles` 的 `column![…]` 里删掉 `column::header(…)` | `head_shape().expect(…)` 那句（`None`） |
/// | 计数只在快照就绪时画这条被改坏 | `column::header` 的 `count` 恒 `Some` | 本用例读的是「就绪 + 有命中」一档，**抓不住**——那一条由 M457 的 `cold_boot` 用例负责 |
#[test]
fn both_views_align_the_head_count_the_same_way() {
    let list = head_shape_after(InstanceView::List, "demo-1");
    let tiles = head_shape_after(InstanceView::Tiles, "demo-1");

    assert_eq!(
        list, tiles,
        "两种视图的列头计数对齐方式不同（M460 要求同一个形态）"
    );
    assert!(
        !list.title_width_fill,
        "列头的标题容器撑满了整行——计数又被顶到行尾（磁贴档 = 页面右缘 x≈1090）"
    );
    assert!(list.count_shown, "有实例且快照就绪时列头必须写计数");
}

/// **判据 ①（结构）+ ③（消息）**：清空叉由**药丸自己**接过去画，页面不再在药丸外面另造一个
/// 按钮；交给药丸的那条消息就是 `ClearSearch`。
///
/// 「叉与搜索框属于同一个容器」在源码上只剩一个证据口：页面调的是
/// `SearchBar::trailing_action(…)`（组件把它拼进 `bar_content` 的 `contents` 行，与输入框同级），
/// 而不是 `buttons::icon(…)` 那种挂在这一行的尾巴上的写法。组件那一侧「确实拼进了输入行」
/// 由 `components/inputs/search/tests.rs::the_trailing_action_is_mounted_inside_the_bar` 的
/// 取样口证明（两半合起来才是判据 ①）。
///
/// 判据 ③（点下去仍产生 `ClearSearch`）同样两半：这里断言**页面交给药丸的消息**是它
/// （[`clear_message`] 是唯一取值口），组件那一半证明那条消息真的挂成了 `on_press`
/// （`buttons::icon(…, Some(message), …)`，见 `search/trailing.rs`）。
///
/// ## 变异与期望红（每条只改一处）
///
/// | 变异 | 改哪里 | 期望红在哪句 |
/// |---|---|---|
/// | 叉退回页面外（M457 的写法） | `search_row` 删掉 `.trailing_action(clear_message())`，改回 `buttons::icon(…)` | `禁止外挂按钮` 那条（`buttons::icon` 重新出现） |
/// | 叉还在页面里，只是挪了个位置 | `search_row` 里另 push 一个 `IconKind::Close` | `禁止外挂按钮` 第二条（`IconKind::Close` 重新出现） |
/// | 清空发出去的是另一条消息 | `clear_message()` 的 body 换成 `InstanceMessage::Search(String::new())` | `matches!` 那句 |
/// | 空药丸也带清空动作 | `search_row` 的 `is_active(query)` 条件去掉 | 组件那半的取样（`Some(true)` ≠ `Some(false)`），本文件抓不住 |
#[test]
fn the_clear_action_is_handed_to_the_bar_not_drawn_beside_it() {
    let page = crate::pages::layout::source::code_text(include_str!("../search.rs"));

    assert!(
        page.contains(".trailing_action(clear_message())"),
        "搜索行没有把清空动作交给药丸（M460：叉要画在输入框内尾随位）"
    );
    assert!(
        !page.contains("buttons::icon"),
        "搜索行还在页面里外挂图标按钮（M460 之前就是它把叉画到药丸外面的）"
    );
    assert!(
        !page.contains("IconKind::Close"),
        "搜索行还在拼 Close 图标——叉没有进药丸"
    );
    assert!(
        matches!(
            clear_message(),
            Message::Instances(crate::pages::InstanceMessage::ClearSearch)
        ),
        "清空叉发出去的不是 ClearSearch"
    );
    // 条件本身也是判据的一部分：空药丸上不该有清除动作（组件侧的取样证明另一半）。
    assert!(is_active("原版") && !is_active(""));
}

/// 磁贴档的取样口与列表档**不是同一个函数**时会走这里：`grid::footprint` 仍然要活着
/// （M457 的判据引用它，删列头不许顺带把它拆掉）。
#[test]
fn the_grid_footprint_is_still_sampled() {
    super::super::column::forget_head();
    render(InstanceView::Tiles, "demo-1");
    assert!(
        grid::footprint().is_some_and(|f| f.cells == 3),
        "磁贴档的格子取样没了（3 条命中应当画出 3 个格子）"
    );
}
