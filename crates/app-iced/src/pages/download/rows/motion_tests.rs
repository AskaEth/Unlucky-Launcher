//! 列表增删动效的**页面侧**判据（M305）：只影响变化项、`NONE` 落到最终态、接线没被悄悄摘掉。
//!
//! 动效的算术（差分 / 时钟 / 收束）在 `components/motion/tests/{rows,progress}.rs`；这里管的是
//! **页面怎么把采样结果接到树上**——那正是「动效把重排成本乘上帧数」这个风险所在。
//!
//! ## 变异账（T41：只改一处 ⇒ 期望红在哪条用例哪句断言）
//!
//! | 变异（只改这一处） | 期望红在哪 |
//! |---|---|
//! | `rows/list.rs` 把 `keyed::Column` 换回 `Column`（按**下标**比对） | `inserting_a_row_keeps_every_unchanged_rows_node`（a/b/c 的树节点会全换一遍） |
//! | `rows/list.rs` 的 `row_key` 里改成 `0`（key 恒等） | 同一条用例（新行会顶掉 a 的节点） |
//! | `pump/rows.rs` 的 `observe_rows` 删掉 `is_off()` 分支 | `no_scale_lands_on_the_final_state_without_frames`（`needs_frames()` 会为真） |
//! | `rows/list.rs` 的遮罩底色从 `MaterialLayer::Canvas` 改回 `Card` | `the_veil_is_canvas_coloured_and_only_exists_while_moving`（r1 P2-2） |
//! | `rows/list.rs` 里把 `(visibility < 1.0).then(…)` 改成无条件 `Some(…)` | 同一条用例的「静息帧不画」（r1 P2-4） |
//! | `rows/list.rs` 里不再按 `motion::rows::drawn(motion::rows::List::Versions)` 取行（改回遍历 `visible`） | `the_list_is_fed_by_the_pump_order` |

use std::any::Any;

use iced::advanced::widget::{tree, Tree};
use unlucky_design::motion::MotionScale;
use unlucky_i18n::Lang;
use unlucky_presenter::dto::VersionKind;

use crate::app::components::motion;
use crate::pages::download::model::{Category, Download, VersionRow};
use crate::theme::{Families, Ui};

use super::{list, visible};

/// 与本页其余用例同一个配色口径（浅色 + Windows 字体栈）。
fn ui() -> Ui {
    Ui::with_platform(
        unlucky_design::color::SchemeFlavor::Light,
        unlucky_design::typography::Platform::Windows,
        Families {
            text: "Segoe UI",
            display: "Segoe UI",
            mono: "Consolas",
        },
    )
}

/// 一行（只填画得出来的字段：文案走目录，这里给空串）。
fn row(id: &str) -> VersionRow {
    VersionRow {
        id: id.to_string(),
        released: "2026-09-20".to_string(),
        kind: VersionKind::Release,
        installed: false,
        supporting: String::new(),
    }
}

fn model(ids: &[&str]) -> Download {
    Download {
        rows: ids.iter().map(|id| row(id)).collect(),
        category: Category::All,
        loaded: true,
        ..Download::default()
    }
}

/// 一行子树的**状态盒指针**（复用 = 同一个 `Box`，重建 = 换一个）。
///
/// 这就是「帧内节点标识」的可观测形态：`Tree` 只有 `tag` / `state` / `children` 三样，
/// 其中 `state` 是那个 `Box`——它被重建，行内的 hover / 焦点 / 滚动锚点就都丢了。
///
/// 行本身是 `Row` / `Revealed` 这种**无状态**容器（`State::None`），所以往下找到第一个
/// 带状态的节点（行里那个 `button`）当标识：同一行的树形状每帧一样，落点也就一样。
fn identity(tree: &Tree) -> Option<*const dyn Any> {
    match &tree.state {
        tree::State::Some(state) => Some(state.as_ref() as *const dyn Any),
        tree::State::None => tree.children.iter().find_map(identity),
    }
}

/// 按当前泵状态构建**键控列**（与页面走同一个函数，跳过外层滚动区）。
fn column_of<'a>(ui: &'a Ui, model: &'a Download) -> iced::Element<'a, crate::pages::Message> {
    let rows = visible(model);
    list::rows_column(ui, Lang::En.catalog(), model, &rows)
}

/// **插一行在前头，a/b/c 的树节点一个都不许重建**（这就是「只影响变化项」）。
///
/// 抓得住：把键控列换成普通 `Column`（按下标比对 ⇒ a 会去对新建那一行的状态）；
/// 或者 key 不再由 id 决定（新行会顶掉别人的状态盒）。
/// 抓不住：帧内的**布局**成本（逐行重排一次是 iced 的布局算法，不是这条用例的范围）。
#[test]
fn inserting_a_row_keeps_every_unchanged_rows_node() {
    motion::reset();
    motion::configure_for_test(MotionScale::STANDARD, None);
    let ui = ui();

    // 起点：a / b / c
    motion::rows::observe(motion::rows::List::Versions, &["a", "b", "c"]);
    let before = model(&["a", "b", "c"]);
    let element = column_of(&ui, &before);
    let mut live = Tree::new(element.as_widget());
    let ids = motion::rows::drawn(motion::rows::List::Versions);
    assert_eq!(ids, vec!["a", "b", "c"], "前提：画的就是这三行");
    let states: Vec<_> = live.children.iter().filter_map(identity).collect();
    assert_eq!(states.len(), 3, "前提：三行三个状态盒");

    // 变化：x 插到最前面（a/b/c 全体下标 +1）
    assert!(motion::rows::observe(
        motion::rows::List::Versions,
        &["x", "a", "b", "c"]
    ));
    let after = model(&["x", "a", "b", "c"]);
    let next = column_of(&ui, &after);
    let ids = motion::rows::drawn(motion::rows::List::Versions);
    assert_eq!(
        ids,
        vec!["x", "a", "b", "c"],
        "新行必须落在它该在的位置（不是追加到末尾）"
    );
    live.diff(next.as_widget());
    let states_after: Vec<_> = live.children.iter().filter_map(identity).collect();
    assert_eq!(states_after.len(), 4);

    // 未变的三行：同一个状态盒（下标挪了但节点没重建）
    for (offset, id) in ["a", "b", "c"].iter().enumerate() {
        assert!(
            std::ptr::eq(states_after[offset + 1], states[offset]),
            "{id} 的树节点被重建了：键控列没生效（下标比对会让整表都换一遍）"
        );
    }
    // 新行是新的节点（一个旧盒子都没被它抢走）
    assert!(
        !states
            .iter()
            .any(|state| std::ptr::eq(*state, states_after[0])),
        "新行的节点不该复用任何旧行的状态盒"
    );
    motion::reset();
}

/// 删一行时，**退场那一行还在列里**（淡出没走完），而且没变的行不动。
#[test]
fn a_removed_row_is_still_in_the_column_while_it_fades() {
    motion::reset();
    motion::configure_for_test(MotionScale::STANDARD, None);
    let ui = ui();

    motion::rows::observe(motion::rows::List::Versions, &["a", "b", "c"]);
    assert!(motion::rows::observe(
        motion::rows::List::Versions,
        &["a", "c"]
    ));
    let model = model(&["a", "c"]);
    let ids = motion::rows::drawn(motion::rows::List::Versions);
    assert_eq!(ids, vec!["a", "b", "c"], "退场行必须还在画的顺序里");

    // 退场行的数据从 `leaving` 池里来：没进池子就画不出来（这一行就是那条接线的判据）
    let mut with_pool = model.clone();
    with_pool.leaving = vec![row("b")];
    let element = column_of(&ui, &with_pool);
    let tree = Tree::new(element.as_widget());
    assert_eq!(
        tree.children.len(),
        3,
        "列里必须还是三行（b 正在淡出），否则下面那些行会当场跳上来"
    );
    motion::reset();
}

/// **泵没被喂过清单时，列表照样画满**（截图 harness 的 `ready` 演示 / 单测直接构造模型）。
///
/// 抓得住：`list` 里只认 `motion::rows::drawn(motion::rows::List::Versions)` 而没有兜底 —— 那些路径会画出一个空列表，
/// 而计数头照样写「8 versions」（M305 出图时实测踩到：图上是空的）。
#[test]
fn a_model_built_without_the_pump_still_draws_every_row() {
    motion::reset();
    let ui = ui();
    let untouched = model(&["a", "b", "c"]);
    let element = column_of(&ui, &untouched);
    let tree = Tree::new(element.as_widget());
    assert_eq!(
        tree.children.len(),
        3,
        "泵还没见过任何清单时，列表必须退回模型自己的顺序"
    );
    motion::reset();
}

/// 列表的行序**由泵给**（不是直接遍历模型）：源码结构断言，盯着接线别被悄悄改回去。
///
/// 抓得住：把 `motion::rows::drawn(motion::rows::List::Versions)` 换回 `visible` 的直接遍历（退场行会缺位、淡出就没了）。
/// 抓不住：drawn 的内容对不对（那是 `components/motion/tests/rows.rs` 的事）。
#[test]
fn the_list_is_fed_by_the_pump_order() {
    let source = include_str!("list.rs");
    for needle in [
        "motion::rows::drawn(motion::rows::List::Versions)",
        "motion::rows::frame(",
        "keyed::Column::new()",
        "motion::revealed(",
        "leaving",
    ] {
        assert!(
            source.contains(needle),
            "`rows/list.rs` 里少了 `{needle}`：列表的行序/动效接线被摘掉了"
        );
    }
}

/// 遮罩的两条口径（r1 P2-2 / P2-4）：**底色取 Canvas**、**静息帧不递**。
///
/// 为什么底色必须是 Canvas：版本行坐在页面的画布上（行静息 `fill = None`，脚下是 `frame::page`
/// 的 Canvas）。取成 Card（`SurfaceContainerLow`）会让遮罩比底色亮一档——浅色 249 vs 247 几乎
/// 看不出，深色 tone 6 vs 10 会露馅。
///
/// 为什么静息帧不许递：`veil = Some(..)` 的 α 是「交给渲染器的半透明」，是全仓唯一一处
/// （口径见 `motion/fade.rs`）；`visibility >= 1` 时必须一个 quad 都不画，否则静息像素与改造前不同。
#[test]
fn the_veil_is_canvas_coloured_and_only_exists_while_moving() {
    let source = include_str!("list.rs");
    assert!(
        source.contains("MaterialLayer::Canvas"),
        "遮罩底色必须是画布层（行脚下那一层）"
    );
    assert!(
        !source.contains("MaterialLayer::Card"),
        "遮罩底色取成 Card 会比底色亮一档（深色档露馅）"
    );
    assert!(
        source.contains("(visibility < 1.0).then("),
        "静息帧（visibility = 1）不许递遮罩：那是一个交给渲染器的半透明 quad"
    );
}

/// `MotionScale::NONE`：列表变化**当场就是最终态**，而且一帧都不要。
#[test]
fn no_scale_lands_on_the_final_state_without_frames() {
    motion::reset();
    motion::configure_for_test(MotionScale::NONE, None);

    motion::rows::observe(motion::rows::List::Versions, &["a"]);
    assert!(!motion::rows::observe(
        motion::rows::List::Versions,
        &["a", "b"]
    ));
    assert!(!motion::needs_frames(), "关掉动效 ⇒ 一帧都不订阅");
    assert_eq!(
        motion::rows::drawn(motion::rows::List::Versions),
        vec!["a", "b"],
        "不许留退场幽灵"
    );

    motion::rows::observe(motion::rows::List::Versions, &["b"]);
    assert_eq!(motion::rows::drawn(motion::rows::List::Versions), vec!["b"]);
    for id in ["a", "b"] {
        let frame = motion::rows::frame(motion::rows::List::Versions, id);
        assert_eq!(frame.visibility, 1.0, "{id} 必须是最终态（不许停在中间）");
        assert_eq!(frame.offset, 0.0, "{id} 不许留偏移");
    }

    // 进度条同理：值严格等于目标、没有收束
    motion::progress::observe("k", 0.4);
    assert!(!motion::progress::observe("k", 1.0));
    let progress = motion::progress::frame("k", 1.0);
    assert_eq!(progress.value, 1.0);
    assert_eq!(progress.settle, 0.0);
    motion::reset();
}
