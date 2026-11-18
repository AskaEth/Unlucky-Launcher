//! 实例列表增删动效的**页面侧**判据（M313）：只影响变化项、退场行还在列里、接线没被悄悄摘掉。
//!
//! 动效的算术（差分 / 时钟 / 错位 / 到期）在 `components/motion/tests/{rows,theme}.rs`；这里管的是
//! **页面怎么把采样结果接到树上**——那正是「动效把重排成本乘上帧数」这个风险所在。
//!
//! ## 变异账（T41：只改一处 ⇒ 期望红在哪条用例哪句断言）
//!
//! | 变异（只改这一处） | 期望红在哪 |
//! |---|---|
//! | `pages/instances/list.rs` 把 `keyed::Column` 换回 `Column`（按**下标**比对） | `inserting_a_row_keeps_every_unchanged_rows_node`（a/b/c 的树节点会全换一遍） |
//! | `pages/instances/list.rs` 把 `motion::rows::key(id)` 改成常量 `0`（key 恒等） | 同一条用例（新行会顶掉 a 的节点） |
//! | `pages/instances/list.rs` 里不再按 `motion::rows::drawn(List::Instances)` 取行（改回遍历 `blocks.rows`） | `the_list_is_fed_by_the_pump_and_the_page_observes_it` 的源码断言 + `a_removed_row_is_still_in_the_column_while_it_fades`（退场行当拍消失） |
//! | `pages/instances/list.rs` 的遮罩底色从 `MaterialLayer::Card` 改成 `Canvas` | `the_list_is_fed_by_the_pump_and_the_page_observes_it` 的遮罩底色断言 |
//! | `app/view.rs::model` 里删掉 `motion::rows::observe(List::Instances, …)`（默认模板） | 同上那条源码断言；**再**让 `leaving` 恒空就同时红 `a_removed_row_…` |

use std::any::Any;

use iced::advanced::widget::{tree, Tree};
use iced::Element;
use unlucky_design::color::SchemeFlavor;
use unlucky_design::motion::MotionScale;
use unlucky_design::typography::Platform;

use crate::app::components::motion;
use crate::pages::instances::list;
use crate::pages::model::{Instances, Row, ScanPhase};
use crate::theme::{Families, Ui};

/// 与本页其余用例同一个配色口径（浅色 + Windows 字体栈）。
fn ui() -> Ui {
    Ui::with_platform(
        SchemeFlavor::Light,
        Platform::Windows,
        Families {
            text: "Segoe UI",
            display: "Segoe UI",
            mono: "Consolas",
        },
    )
}

fn row(id: &str) -> Row {
    Row {
        id: id.to_string(),
        name: format!("实例 {id}"),
        supporting: "1.20.4 · Fabric 0.16.5".to_string(),
        status: None,
    }
}

fn model(ids: &[&str]) -> Instances {
    Instances {
        rows: ids.iter().map(|id| row(id)).collect(),
        detail: None,
        scan: ScanPhase::Ready,
        leaving: Vec::new(),
    }
}

/// 一行子树的**状态盒指针**（复用 = 同一个 `Box`，重建 = 换一个）——与下载页同一条可观测形态。
fn identity(tree: &Tree) -> Option<*const dyn Any> {
    match &tree.state {
        tree::State::Some(state) => Some(state.as_ref() as *const dyn Any),
        tree::State::None => tree.children.iter().find_map(identity),
    }
}

/// 按当前泵状态构建**键控列**（与页面走同一个函数）。
fn column_of<'a>(ui: &'a Ui, block: &'a Instances) -> Element<'a, crate::pages::Message> {
    list::column(ui, block, None)
}

/// **插一行在前头，a/b/c 的树节点一个都不许重建**（=「未变行是同一个状态盒」）。
///
/// 抓得住：键控列换成普通 `Column`（按下标比对 ⇒ a 会去对新建那一行的状态）；或 key 不再由
/// id 决定（新行顶掉别人的状态盒）。
/// 抓不住：帧内的布局成本（逐行重排一次是 iced 自己的布局算法，不在这条用例范围）。
#[test]
fn inserting_a_row_keeps_every_unchanged_rows_node() {
    motion::reset();
    motion::configure_for_test(MotionScale::STANDARD, None);
    let ui = ui();

    motion::rows::observe(motion::rows::List::Instances, &["a", "b", "c"]);
    let before = model(&["a", "b", "c"]);
    let mut live = Tree::new(column_of(&ui, &before).as_widget());
    let states: Vec<_> = live.children.iter().filter_map(identity).collect();
    assert_eq!(states.len(), 3, "前提：三行三个状态盒");

    assert!(motion::rows::observe(
        motion::rows::List::Instances,
        &["x", "a", "b", "c"]
    ));
    let after = model(&["x", "a", "b", "c"]);
    assert_eq!(
        motion::rows::drawn(motion::rows::List::Instances),
        vec!["x", "a", "b", "c"],
        "新行必须落在它该在的位置（不是追加到末尾）"
    );
    live.diff(column_of(&ui, &after).as_widget());
    let states_after: Vec<_> = live.children.iter().filter_map(identity).collect();
    assert_eq!(states_after.len(), 4);

    for (offset, id) in ["a", "b", "c"].iter().enumerate() {
        assert!(
            std::ptr::eq(states_after[offset + 1], states[offset]),
            "{id} 的树节点被重建了：键控列没生效（下标比对会让整表都换一遍）"
        );
    }
    assert!(
        !states
            .iter()
            .any(|state| std::ptr::eq(*state, states_after[0])),
        "新行的节点不该复用任何旧行的状态盒"
    );
    motion::reset();
}

/// 删一行时，**退场那一行还在列里**（淡出没走完），且它占的是原来的位置。
///
/// 抓得住：把「画哪些行」改回遍历 `block.rows`（退场行当拍消失 = 闪跳）；或把 `leaving`
/// 数据池摘掉（那一行只剩空壳，节点数量对不上）。
/// 抓不住：淡出**看起来**怎么样（那要出图）；也**不在这里**跑时间轴——「退场走完才真移除」
/// 是泵的算术，判据在 `components/motion/tests/rows.rs`（页面层不许碰时长 / 时间轴，
/// 见 `app/subscription.rs::pages_cannot_reach_for_a_duration`）。
#[test]
fn a_removed_row_is_still_in_the_column_while_it_fades() {
    motion::reset();
    motion::configure_for_test(MotionScale::STANDARD, None);
    let ui = ui();

    motion::rows::observe(motion::rows::List::Instances, &["a", "b", "c"]);
    let before = model(&["a", "b", "c"]);
    let mut live = Tree::new(column_of(&ui, &before).as_widget());
    assert_eq!(live.children.len(), 3);

    // 删掉 b：模型里它没了，退场池里还有它（`app/view.rs::model` 就是这么填的）。
    assert!(motion::rows::observe(
        motion::rows::List::Instances,
        &["a", "c"]
    ));
    let mut after = model(&["a", "c"]);
    after.leaving = vec![row("b")];
    assert_eq!(
        motion::rows::drawn(motion::rows::List::Instances),
        vec!["a", "b", "c"],
        "退场行必须钉在原位"
    );
    live.diff(column_of(&ui, &after).as_widget());
    assert_eq!(
        live.children.len(),
        3,
        "淡出没走完时这一行还在（不是当拍消失）"
    );
    motion::reset();
}

/// **接线**：行序来自泵、列是键控列、一层裁切 + Card 色的收尾遮罩；页面在 update 侧观察清单。
///
/// 抓得住：把上面任何一处摘掉（改回遍历模型 / 换普通列 / 去掉 `revealed` / 底色取成画布 /
/// `app/view.rs` 忘了 `observe`）——源码里那一段就不在了。
/// 抓不住：这些接线**画出来**对不对（行为由上面两条 + 出图守）。
#[test]
fn the_list_is_fed_by_the_pump_and_the_page_observes_it() {
    let page = include_str!("../list.rs");
    for needle in [
        "motion::rows::drawn(motion::rows::List::Instances)",
        "motion::rows::frame(motion::rows::List::Instances",
        "motion::rows::key(id)",
        "keyed::Column::new()",
        "motion::revealed(",
        "block.leaving",
        // 实例行坐在 `MaterialLayer::Card` 的卡里（遮罩底色必须与脚下那层同色）
        "ui.layer(MaterialLayer::Card)",
    ] {
        assert!(
            page.contains(needle),
            "`pages/instances/list.rs` 里少了 `{needle}`：列表动效接线被摘掉了"
        );
    }

    // **观察点**在 app 侧（update 侧）：`pages::Instances` 就是 `app/view.rs::model` 构造的，
    // 所以清单与退场池也在那里报（与下载页在 `Download::from_snapshot` 里报同一条纪律）。
    let app_view = include_str!("../../../app/view.rs");
    for needle in [
        "motion::rows::observe(motion::rows::List::Instances",
        "motion::rows::drawn(motion::rows::List::Instances",
    ] {
        assert!(
            app_view.contains(needle),
            "`app/view.rs::model` 里少了 `{needle}`：实例列表的动效没有起跑点"
        );
    }
}
