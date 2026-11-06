//! 设置页的**跨 section** 守门测试（M100：跨 section 的留在这里，section 自己的断言**新增**在
//! `tests/<section>_section.rs`）。
//!
//! ## 为什么这里只剩「跨 section」的断言
//!
//! 设置页正文接下来由四条线并行接（java / download / language+appearance / ai+about）。每条线要能
//! **只改自己的测试**就跑通，所以「某一组自己的行文案 / 图标 / body 现状」一律放在
//! `tests/<section>_section.rs`（M100 **新增**的 12 条，不是从本文件搬过去的——本文件在 `main`
//! 上就只有下面这 7 条跨 section 断言，一条 section 断言都没有）；本文件只留**必须四组一起看**的
//! 断言——下放到任何单组文件里，都会变成「某组替其它三组说话」，四条线并行时必然互相踩。
//!
//! 判据（M100 报告里正面回答的那条）：**给 java 加一条新断言，改几个文件？** —— 1 个
//! （`tests/java_section.rs`）。本文件不动。
//!
//! 但「java 线**接真控件**那天改几个文件」是另一回事，答案是 **2 个**（M101 r2 订正：
//! M100 之后曾是 3 个，第 3 处就是本文件末尾那条聚合绊线
//! `only_the_appearance_group_carries_a_real_control_body`，它被四条 per-group 断言完全蕴含、
//! 已在 M101 r2 删除）：`settings/java.rs`（写 body）+ `tests/java_section.rs` 的
//! `the_java_group_is_still_demo_rows`（它断言 `body.is_none()`，接上就红）。download / general 同型。
//!
//! ## ai+about 线要额外改本文件的两处（M100 r1 复审 p3-4）
//!
//! 它加第 5 组时，除了 `settings/mod.rs` 的 `GROUPS`，**本文件还有两处必改**：
//! 1. 顶部 `mod ai_section;`（新测试文件要挂进来，否则永远不编译、永远不跑）；
//! 2. [`the_split_kept_the_left_column_order`] 断言的是**字面量** `["General","Appearance",
//!    "Java","Downloads"]`，加第 5 组必红——这是刻意的：左列顺序变了就是观感变了，要人确认。
//!    另外 `the_appearance_group_is_where_the_module_says_it_is` 依赖 `APPEARANCE_GROUP == 1`，
//!    **把新组插在 Appearance 前面会连带红**（所以新组应该排最后）。
//!
//! ## 站位
//!
//! 本文件是 `pages::settings::tests`：`super` = `pages::settings`，`super::super` = `pages`。
//!
//! ## 命名
//!
//! 子模块带 `_section` 后缀，是为了不和 `use super::*` 带进来的分组模块（`settings::java` 等）
//! 撞名——撞名时显式 `mod` 会静默盖掉 glob，读代码的人会以为 `java` 指的是分组的那个。

mod appearance_section;
mod download_section;
mod general_section;
mod java_section;

use super::super::layout::{self, frame_metrics, measure_cap_px, PaneKit, SIDE_NAV_WIDTH};
use super::*;
use crate::pages::appearance::{APPEARANCE_GROUP, GROUP_LABEL};

// --------------------------------------------------------------- 版面（不属于任何一组）

/// 守的是**版面契约**：分组导航走 drawer 240（D33），不是 rail 的 88。
///
/// 不能下放到分组：它不是「某一组」的属性，而是整个 supporting-pane 的几何。
#[test]
fn side_nav_reuses_the_drawer_width() {
    assert_eq!(SIDE_NAV_WIDTH, 240.0);
    let metrics = frame_metrics(PaneKit::Supporting, 1180.0);
    assert_eq!(metrics.secondary, Some(240.0));
    assert_ne!(metrics.secondary, Some(layout::rail_width()));
}

/// 守的是**行宽上限**（逐行 67 字符 ≤72）在所有宽度档下都成立。
///
/// 不能下放：它是四个分组共用的正文列，任何一组把列撑宽都会连带别人。
#[test]
fn form_column_never_exceeds_the_measure_cap() {
    for width in [720.0, 840.0, 1180.0, 1920.0] {
        let metrics = frame_metrics(PaneKit::Supporting, width);
        assert!(
            metrics.column <= measure_cap_px() + f32::EPSILON,
            "{width} 宽下表单列 {:?} 超过行宽上限",
            metrics.column
        );
        assert!(metrics.column >= layout::measure_floor_px());
    }
}

/// 守的是**窄窗退化**：<720 时侧列消失、分组改为按组堆叠。
///
/// 不能下放：退化是整页行为，四组一起变（单组文件里没法验证「侧列没了」）。
#[test]
fn narrow_window_stacks_the_groups_instead_of_the_side_nav() {
    let narrow = frame_metrics(PaneKit::Supporting, 700.0);
    assert_eq!(narrow.layout, PageLayout::SingleColumn);
    assert!(narrow.secondary.is_none(), "窄窗不许留侧列");
}

// --------------------------------------------------------------- 注册表（四组一起看）

/// 守的是**注册表的结构不变量**：每组都有名字、图标、演示行，且演示下标在范围内。
///
/// 下放不了：它遍历的是 `GROUPS`（四组拼出来的数组），单组文件拿不到其它三组，
/// 而「新增一组忘了填 rows」正是这条要抓的。
#[test]
fn every_group_has_a_name_icon_and_rows() {
    assert!(GROUPS.len() >= 3);
    assert!(SELECTED < GROUPS.len());
    for group in &GROUPS {
        assert!(!group.name.is_empty());
        assert!(!group.icon.icon().path.is_empty(), "{} 缺图标", group.name);
        assert!(!group.rows.is_empty(), "{} 没有设置行", group.name);
        for (label, value) in group.rows {
            assert!(!label.is_empty() && !value.is_empty());
        }
    }
}

/// M93：设置页「外观」分组的下标与 `pages::appearance` 声明的常量必须一致。
///
/// 抓得住：谁把 `GROUPS` 的顺序调了却忘了改 `APPEARANCE_GROUP`（那样真控件会画到别的分组上，
/// 而支持列显示的分组名也对不上）。
/// 抓不住：控件本身画得对不对（那由截图与 app 侧测试管）。
///
/// 下放不了：它同时读 `GROUPS`（注册表）与 `pages::appearance` 的常量，横跨两处；
/// 放进 `appearance_section.rs` 会让「外观那条线」替注册表顺序作证。
#[test]
fn the_appearance_group_is_where_the_module_says_it_is() {
    assert_eq!(GROUPS[APPEARANCE_GROUP].name, GROUP_LABEL);
    assert!(SELECTED < GROUPS.len(), "演示下标仍在范围内");
}

/// M99：拆分后**左列顺序**必须还是拆分前那四项（顺序变了就是观感变了，不是「纯搬移」）。
///
/// 抓得住：① 注册表顺序被调换（`GROUPS` 是四条线各自文件拼出来的，最容易在合并时错位）；
/// ② 某组被误删/误加（长度不等）。
/// 抓不住：组内那些行还在不在、译文对不对（各组自己的文件与 `catalog` 金表各管一段）。
///
/// 下放不了：它是一句「四组顺序 = 这四个名字」的断言，任何单组文件都只能看到自己那一个名字。
#[test]
fn the_split_kept_the_left_column_order() {
    let names: Vec<&str> = GROUPS.iter().map(|group| group.name).collect();
    assert_eq!(names, vec!["General", "Appearance", "Java", "Downloads"]);
}

/// M101 r2：`settings/mod.rs` 里**不许出现任何一组的 `State` 类型**（r1 复审 P1 的判据钉在这里）。
///
/// 抓得住：谁把状态漏斗拆回去——让 `view` / `form` 直接收 `appearance::State` 或某一组的 `State`
/// （那正是四条线「加一个字段就要改本文件」的形状，本次复审要求修掉的就是它）。
/// 抓不住：各组的**字段**（那些活在各组自己的文件里，本文件本来就不该知道，也就没有可断言的东西）。
///
/// **M106 的例外要读清楚**：General 已经接了真控件（语言行），但它**不走 `Group.body`**——
/// 那个槽位的签名是 `fn(&Ui, States) -> Option<Element>`，`States` 只有 `appearance` + `groups`
/// 两格，装不下「当前语言档位 + 生效语言」。它走 `settings::view` 的 `language` 形参 +
/// `GENERAL_GROUP` 那条按下标分发，`body` 仍是 `None`，所以本断言不动。
/// 判据没变：**`body: Some(..)` 仍然只有外观组**。
#[test]
fn the_settings_module_does_not_name_any_group_state() {
    let source = include_str!("../mod.rs");
    let forbidden = [
        "appearance::State",
        "java::State",
        "download::State",
        "language_appearance::State",
        "ai_about::State",
    ];

    let hits: Vec<&str> = forbidden
        .into_iter()
        .filter(|needle| source.contains(needle))
        .collect();

    assert!(
        hits.is_empty(),
        "`settings/mod.rs` 里出现了具体的组状态类型 {hits:?}：状态漏斗被打回去了——\
         四条填页线又要改本文件。状态应该只在 `settings/state.rs` 的 `Groups` 与各组自己的\
         `State` 定义里出现，本文件只转发 `States`"
    );
}

///
/// 为什么能删：`tests/appearance_section.rs` 断言外观组 `body.is_some()`，
/// `tests/{general,java,download}_section.rs` 各自断言本组 `body.is_none()`——四条合起来与
/// 原来那条聚合断言等价（成员与顺序另有 [`the_split_kept_the_left_column_order`] 钉住）。
/// 删掉之后每条线「接真控件」只需改自己那 2 个文件，不必来本文件报到。
///
/// ## 删掉之后**抓不住什么**（tower 要求写明）
///
/// 那四条都只看**本组**，没有任何一条再断言「唯一性」——「四组里恰好一组挂了真控件」这件事
/// **现在没有守卫**。具体漏法：
///
/// | 情形 | 还红吗 |
/// |---|---|
/// | 某条线接上 `body` 却忘了翻自己那条 `is_none()` | **红**（本组断言还在） |
/// | 两条线的断言被同时改成 `is_some()`，页面上两组同时长真控件 | **绿着过去**（没人指认「唯一」） |
/// | 谁把 `body: Some(..)` 挂到一个不该有的组、又顺手改了它那条断言 | **绿着过去** |
///
/// 接受这个损失的理由：接真控件是**每组各自的一次性动作**，谁做谁翻自己那一条；而「唯一」在
/// 四条线并行的阶段是最容易假红的约束（一条线接上就替另外三条红）。想要回这条守卫，等四条线
/// 都落地后在这里加一条**计数**断言（`GROUPS.iter().filter(|g| g.body.is_some()).count()`），
/// 而不是现在钉死某个下标。
///
/// 这条 `#[test]` 本体已删；保留这段文档是为了让后来的人知道「这里原本有一条绊线」。
#[test]
fn every_line_can_be_wired_alone() {
    // 判据是「每条线只改自己的文件」，不是任何一条具体断言：这里只钉住架子本身的两件事——
    // ① 每组都有 `GROUP`（成员齐）；② 演示下标合法。真正的 per-group 断言在各自文件里。
    assert_eq!(GROUPS.len(), 4, "四条线各一个分组");
    assert!(SELECTED < GROUPS.len(), "演示下标仍在范围内");
}
