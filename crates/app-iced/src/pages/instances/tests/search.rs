//! M457 的**搜索判据**：关键词真的改变了「画出来多少行 / 多少个格子」。
//!
//! ## 为什么这么写（「真过滤」与「看着像」的分界）
//!
//! 「渲染了一个搜索框」什么都证明不了：输入控件挂上去、`on_input` 接好了、`drawn()` 照样
//! 返回全量，画面上的搜索框一个像素都不会变。所以三条判据都读**画的那一段自己记下的数**：
//!
//! | 取样口 | 谁写 | 记什么 |
//! |---|---|---|
//! | `grid::footprint().list_rows` | `instances.rs::list_column` | 列表视图这一帧**真画出来的行数** |
//! | `grid::footprint().cells` | `grid.rs::tiles` / `note_no_cells` | 磁贴视图这一帧**真画出来的格子数** |
//! | `search::empty_drawn()` | `search.rs::row` / `empty` | 这一帧是否画了「没有匹配」的空态 |
//!
//! ## 三条判据（两种视图各来一遍）
//!
//! | 用例 | 抓得住 |
//! |---|---|
//! | [`a_word_nobody_matches_draws_nothing_in_either_view`] | 输 `xyz` ⇒ 画出的行 / 格子 = **0**，且**真的是那句空态**（不是一片空白） |
//! | [`a_real_prefix_draws_exactly_the_matching_rows`] | 输 `demo-1` ⇒ 恰好 3（`demo-1` / `demo-10` / `demo-11`），两种视图同一个数 |
//! | [`clearing_the_keyword_brings_every_row_back`] | 清空 ⇒ 回到全量 12；空态消失 |
//!
//! 十二条实例、四列档（`Medium`）：12 = 3 排整，而 3 个命中 = 1 排（3/4 排不满），所以
//! 「格子数」与「排数 × 列数」在这两条用例里**不是同一个数**——只看排数会漏掉「只筛了第一排」。

use unlucky_design::color::SchemeFlavor;
use unlucky_design::typography::Platform;
use unlucky_presenter::dto::LaunchArgsView;
use unlucky_presenter::prefs::{InstanceView, TileSize};
use unlucky_presenter::router::NavSection;

use crate::pages::layout::{self, PaneKit};
use crate::pages::model::{Instances, Row, ScanPhase};
use crate::pages::{LaunchForm, Model, Screen};
use crate::theme::{Families, Ui};

use super::super::grid::{self, Footprint, View};
use super::super::search;

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

/// 渲染一屏实例页（带关键词），读回**画的那一段**记下的形态。
fn render(mode: InstanceView, query: &str) -> Footprint {
    let ui = ui();
    let form = LaunchForm::from_view(None, &LaunchArgsView::default());
    let model = model();
    let metrics = layout::frame_metrics(PaneKit::ListDetail, model.window_width);
    let view = View {
        mode,
        tile_size: TileSize::Medium,
        query,
    };
    let element = super::view(&ui, metrics, &model, &form, view);
    // 树只是被建出来就走掉：取样在构建期已经发生。
    drop(element);
    grid::footprint().expect("渲染过一屏之后必须有取样（`grid::note` / `note_list_rows` 的接线）")
}

/// **判据 ①**：一个谁都命不中的词 ⇒ 两种视图都**一个都不画**，而且画的是那句空态。
///
/// ## 变异与期望红（每条只改一处）
///
/// | 变异 | 改哪里 | 期望红在哪句 |
/// |---|---|---|
/// | 关键词没进过滤（只接了消息、没接 `drawn`） | `instances/list.rs::drawn` 里删掉那行 `.filter(…)` | 列表 12 行、磁贴 12 个格子，两句都红 |
/// | 只筛了列表，磁贴仍画全量 | `instances/grid.rs::tiles` 的 `super::list::drawn(block, view.query)` → `drawn(block, "")` | `cells` = 12 ≠ 0 |
/// | 空结果仍然画一张空网格（不画空态） | `instances.rs::tiles_body` 删掉 `matched == 0` 那一支 | `cells` = 0 但 `empty_drawn()` 为假 ⇒ 空态那句红 |
/// | 空态画了，但用户没法清掉关键词 | `instances/search.rs::empty` 删掉 `.action(…)` | 空态按钮那句源码断言红（见最后一段） |
#[test]
fn a_word_nobody_matches_draws_nothing_in_either_view() {
    let list = render(InstanceView::List, "xyz");
    assert_eq!(list.list_rows, 0, "列表视图：没命中的词仍然画出了行");
    assert_eq!(list.cells, 0, "列表视图不该有网格格子");
    assert!(search::empty_drawn(), "列表视图：空结果画成了一片空白");

    let tiles = render(InstanceView::Tiles, "xyz");
    assert_eq!(tiles.cells, 0, "磁贴视图：没命中的词仍然画出了格子");
    assert_eq!(tiles.grid_rows, 0, "磁贴视图：一排都没画");
    assert!(search::empty_drawn(), "磁贴视图：空结果画成了一片空白");
}

/// **判据 ②**：一个真实前缀 ⇒ 画出来的行 / 格子**恰好是命中数**。
///
/// 为什么是 `demo-1`：它命中 3 条（`demo-1` / `demo-10` / `demo-11`），既不是 0 也不是 12，
/// 所以「完全没过滤」与「全过滤掉」两种退化都被这一条挡住；而 3 < 4（列数）意味着磁贴视图只
/// 有**一排**且排不满——「格子数」与「排数 × 列数」在这里不同值（只按排数断言会漏掉半排）。
///
/// ## 变异与期望红（每条只改一处）
///
/// | 变异 | 改哪里 | 期望红在哪句 |
/// |---|---|---|
/// | 过滤词被当成整名匹配（`lab` 搜不到 `demo-lab`） | `instances/search.rs::matches` 的 `.contains(&needle)` → `== needle` | `cells` / `list_rows` = 0 ≠ 3 |
/// | 大小写敏感 | 同上，删掉两处 `.to_lowercase()` | 大写那条断言红（本用例末尾补一条 `DEMO-1`） |
/// | 采样记的是入参而不是真画出来的 | `instances/grid.rs::tiles` 的 `note(…, drawn.len())` → `note(…, 12)` | `cells` = 12 ≠ 3 |
/// | 命中后网格退回一列宽卡 | `instances/grid.rs::tiles` 的 `let columns = size.columns();` → `let columns = 1;` | `grid_rows` = 3 ≠ 1（一排一个 = 三排） |
#[test]
fn a_real_prefix_draws_exactly_the_matching_rows() {
    let list = render(InstanceView::List, "demo-1");
    assert_eq!(list.list_rows, 3, "列表视图：3 条命中的实例画成了别的行数");
    assert_eq!(list.cells, 0, "列表视图不该有网格格子");
    assert!(!search::empty_drawn(), "有命中却画了空态");

    let tiles = render(InstanceView::Tiles, "demo-1");
    assert_eq!(tiles.cells, 3, "磁贴视图：3 条命中的实例画成了别的格子数");
    assert_eq!(tiles.grid_rows, 1, "3 个格子（4 列档）只占一排");
    // `columns` 记的是**第一排真放了几个格子**（`grid.rs::note` 的口径），不是配置的列数：
    // 4 列档只命中 3 条 ⇒ 第一排就是 3 个。写成 4 会把「取样口径」与「配置」混为一谈。
    assert_eq!(
        tiles.columns, 3,
        "第一排只放了 3 个命中格子（4 列档填不满一排）"
    );
    assert!(!search::empty_drawn(), "有命中却画了空态");

    // 判据本身要能区分「3」与「0 / 全量」：这两个退化在上面每句都已经红了，这里再钉一次
    // 「区分得开」——命中数既不等于全量，也不等于零。
    assert!(list.list_rows > 0 && list.list_rows < ROWS);

    // 大小写不敏感（同一个词换个写法必须给同一个数）。
    let upper = render(InstanceView::List, "DEMO-1");
    assert_eq!(upper.list_rows, 3, "关键词的匹配区分了大小写");
}

/// **判据 ③**：清空关键词 ⇒ 回到全量（且空态消失）。
///
/// 判据读的是**同一条渲染路径**上的两次取样：关键词在会话状态里（`App::instance_query`），
/// 清空后页面必须按「没有过滤」重画，而不是留着上一次的筛子。
///
/// ## 变异与期望红（每条只改一处）
///
/// | 变异 | 改哪里 | 期望红在哪句 |
/// |---|---|---|
/// | 空关键词仍走过滤（`matches` 对空串返回 `false`） | `instances/search.rs::matches` 删掉 `if needle.is_empty() { return true }` | 清空后 `list_rows` / `cells` = 0 ≠ 12 |
/// | 清空后仍画空态 | `instances.rs::list_column` 的 `search::is_active(query) && matched == 0` → 删掉 `search::is_active(query)` | `empty_drawn()` 仍为真 |
#[test]
fn clearing_the_keyword_brings_every_row_back() {
    let filtered = render(InstanceView::List, "demo-1");
    assert_eq!(filtered.list_rows, 3);

    let cleared = render(InstanceView::List, "");
    assert_eq!(cleared.list_rows, ROWS, "清空关键词后列表没有回到全量");
    assert!(!search::empty_drawn(), "清空关键词后还挂着空态");

    let tiles = render(InstanceView::Tiles, "");
    assert_eq!(tiles.cells, ROWS, "清空关键词后网格没有回到全量");
    assert_eq!(
        tiles.grid_rows,
        ROWS.div_ceil(4),
        "12 条实例（4 列档）= 3 排"
    );
}

/// **接线判据**（源码结构）：搜索行在**两种视图**的树上都挂了，且空态带一个清空动作。
///
/// 为什么还需要这条（取样不是已经证明了过滤吗）：取样证明「过滤生效」，但**搜索框本身**是
/// 那一屏唯一的输入入口——它一旦从某一支里掉出去，用户就再也改不了关键词（而两种视图的取样
/// 仍然全绿，因为 App 里的关键词还在）。这条盯的就是「控件挂没挂」。
#[test]
fn the_search_row_is_mounted_in_both_views() {
    // 两处挂载点：列表卡片在 `instances/column.rs`，磁贴正文在 `instances.rs`（M457 拆卷之后的
    // 真实落点）。两处都数，所以「某一支掉了一个」会立刻红。
    let column = crate::pages::layout::source::code_text(include_str!("../../instances/column.rs"));
    let page = crate::pages::layout::source::code_text(include_str!("../../instances.rs"));
    assert_eq!(
        column.matches("search::search_row(").count(),
        1,
        "列表卡片的搜索行没挂（或者挂了不止一处）：
{column}"
    );
    assert_eq!(
        page.matches("search::search_row(").count(),
        1,
        "磁贴正文的搜索行没挂（或者挂了不止一处）：
{page}"
    );

    let module = crate::pages::layout::source::code_text(include_str!("../../instances/search.rs"));
    assert!(
        module.contains("InstanceMessage::ClearSearch"),
        "空态里没有能清掉关键词的动作（用户只能一个字一个字删）"
    );
}
