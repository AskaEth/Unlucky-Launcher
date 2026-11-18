//! M273 · 「导入已有版本」向导的**入口接线**（空态那一半）。
//!
//! ## 为什么要单独一组
//!
//! 这条功能的失败形态不是「算错了」，而是「界面上没有入口」（M223 之后用户实测：
//! `instances=0` 的空态只有一句「未选择实例」，他找不到任何能接管 `.minecraft` 的地方）。
//! 逻辑测试抓不住这种洞——`on_adopt(Open)` 的用例一直是绿的，因为**没有人从界面派发它**。
//!
//! | 用例 | 抓得住 | 抓不住 |
//! |---|---|---|
//! | [`the_empty_state_hands_out_the_adopt_intent`] | 空态主行动发的是**哪一条**意图（被换成新建 / 删除 / 复制就红） | 那条意图真的从按钮里发出来（`Element` 没有内省接口，见 `pages::layout::source`） |
//! | [`the_empty_state_wires_the_cta_to_that_intent`] | 空态里那枚按钮**被删掉**、被注释掉、或接到别的消息上 | 按钮画出来好不好看 / 真的可点（出图 + 人工） |
//! | [`the_intent_still_reaches_the_adopt_wizard`] | `pages::Action::Import` → `AdoptMessage::Open` 这一跳被拆掉（那正是「UI 存在但没人派发」的复发形态） | `Open` 之后向导自己的四段（`app/update/adopt.rs` 的用例覆盖） |
//!
//! 三条合起来是一条链：空态按钮 ⇒ [`adopt_action`] ⇒ `Action::Import` ⇒ `AdoptMessage::Open`。
//! 任何一段断开，这里至少红一条。

use crate::pages::layout::source;

use super::super::adopt_action;
use super::super::Texts;
use crate::pages::{Action, Message};
use unlucky_i18n::Lang;

/// 空态主行动的发的是**导入**意图（不是新建、不是删除、不是占位动作）。
#[test]
fn the_empty_state_hands_out_the_adopt_intent() {
    assert_eq!(
        format!("{:?}", adopt_action()),
        format!("{:?}", Message::Action(Action::Import)),
        "空态主行动必须发「导入已有版本」，否则向导依旧打不开"
    );
    let text = format!("{:?}", adopt_action());
    for wrong in ["NewInstance", "Delete", "Duplicate", "OpenFolder"] {
        assert!(!text.contains(wrong), "空态主行动接到了 {wrong} 上：{text}");
    }
}

/// 空态的按钮**在代码里**接 `adopt_action()`（判据是代码行：注释掉等于没有）。
///
/// 两种空态还必须分开：一个实例都没有时用 [`Texts::empty_none_title`]，不能再拿
/// 「未选择实例」敷衍——那句话在告诉用户「你已经有的东西只是没点」。
#[test]
fn the_empty_state_wires_the_cta_to_that_intent() {
    let detail = include_str!("../detail.rs");
    let body = source::function_body(detail, "fn empty_no_instances");
    let cta = source::code_line(body, "Button::new(texts.adopt_cta()")
        .unwrap_or_else(|| panic!("空态里没有一行**代码**建那枚「导入已有版本」按钮：\n{body}"));
    assert!(
        cta.contains("adopt_action()"),
        "空态按钮没接 `adopt_action()`（接成别的消息 = 入口又断了）：{cta}"
    );

    let en = Texts::of(Lang::En.catalog());
    assert_ne!(
        en.empty_none_title(),
        en.empty_title(),
        "「一个实例都没有」与「没选中」必须是两句话，否则空态又变成误导"
    );
    assert!(
        !en.adopt_cta().is_empty(),
        "空态主行动必须有可读标签（空标签 = 看不见的按钮）"
    );
}

/// **CTA 只属于「一个都没有」那一支**：`has_instances == true` 的空态不许有任何动作。
///
/// r1 复审实测的缺口：把 `!block.rows.is_empty()` 改成 `block.rows.is_empty()`，于是「0 个实例」
/// 反而走「没选中」那一支、**没有按钮**，而当时的三条用例全绿。这条与下一条就是为它补的：
/// 这里钉「每个分支自己长什么样」，下一条钉「传参有没有传反」。
///
/// 抓得住：CTA 被挪到「没选中」那一支（用户有实例时反而看见「导入已有版本」）；
/// 「没选中」那一支被塞进动作（`Vec::new()` 被换掉）；两个分支被写成一模一样。
/// 抓不住：`frame::empty_state` 真的把那枚按钮画出来（`Element` 无内省，见 `pages::layout::source`）。
#[test]
fn the_cta_lives_only_in_the_no_instances_branch() {
    let detail = include_str!("../detail.rs");
    let selected = source::code_text(source::function_body(detail, "fn empty_not_selected"));
    let none = source::code_text(source::function_body(detail, "fn empty_no_instances"));

    assert!(
        selected.contains("Vec::new()"),
        "「有实例但没选中」那一支不该有任何动作（动作在左列）：\n{selected}"
    );
    for needle in ["adopt_cta", "adopt_action"] {
        assert!(
            !selected.contains(needle),
            "「有实例但没选中」那一支混进了 `{needle}`（CTA 挪支了）：\n{selected}"
        );
    }
    for needle in ["adopt_cta()", "adopt_action()"] {
        assert!(
            none.contains(needle),
            "「一个实例都没有」那一支缺 `{needle}`（用户又找不到入口）：\n{none}"
        );
    }

    // 分派本身：`if has_instances` 走「没选中」，`else` 走带 CTA 的那一支。
    // 行首判据 ⇒ 写成 `if !has_instances` 立刻红（那种取反翻转下面两条顺序断言看不出来）。
    let dispatch = source::function_body(detail, "fn detail_empty");
    assert!(
        source::has_code_line(dispatch, "if has_instances {"),
        "`detail_empty` 必须以 `if has_instances {{` 分派（取反 = 两个空态互换）：\n{dispatch}"
    );
    let code = source::code_text(dispatch);
    let if_at = code.find("if has_instances").expect("分派条件");
    let else_at = code.find("} else {").expect("else 分支");
    let selected_at = code
        .find("empty_not_selected(ui, texts)")
        .expect("没选中分支");
    let none_at = code
        .find("empty_no_instances(ui, texts)")
        .expect("一个都没有分支");
    assert!(
        if_at < selected_at && selected_at < else_at,
        "`has_instances` 为真必须走 `empty_not_selected`：\n{code}"
    );
    assert!(
        else_at < none_at,
        "`has_instances` 为假必须走 `empty_no_instances`：\n{code}"
    );
}

/// **调用点**：实例页把「没选中时画什么」交给三态分派，宽窗 / 窄窗各一处。
///
/// 历史：M273 r1 复审的变异是把 `!block.rows.is_empty()` 改成 `block.rows.is_empty()`——分支
/// 断言照样绿，但**用户看到的是错的那一支**，所以判据必须落在调用点上。M302 之后「有没有实例」
/// 那一句从调用点挪进了 [`detail_side`]，于是这条判据一分为二：**调用点**（两处、都传 `block`）
/// 与**分派点**（`Ready` 那一支自己带 `!`）。
///
/// 抓得住：只改掉一处（宽窗接了、窄窗忘了）；新增第三条调用点却传了别的值；`!` 从分派点丢掉。
/// 抓不住：等价写法（`block.rows.len() == 0`）——本仓构建链一行一条，判据按行首取。
#[test]
fn the_page_asks_the_three_state_dispatch_from_both_call_sites() {
    let page = include_str!("../../instances.rs");
    let calls: Vec<String> = source::code_lines(page)
        .into_iter()
        .filter(|line| line.contains("detail_side("))
        .map(|line| line.trim().to_string())
        .collect();

    assert_eq!(
        calls.len(),
        2,
        "实例页应当只有两条「没选中时画什么」的调用点（宽窗 + 窄窗），实际：{calls:?}"
    );
    for line in &calls {
        assert!(
            line.contains("(ui, block, texts)"),
            "调用点要把整块模型交给三态分派（自己判 `rows` 就又把三态塌成两态了）：{line}"
        );
    }

    // 分派点：「有没有实例」那一位带着 `!`，且只在 `Ready` 那一支
    let detail = source::code_text(source::function_body(
        include_str!("../detail/scan.rs"),
        "fn detail_side",
    ));
    assert!(
        detail.contains("!block.rows.is_empty()"),
        "`detail_side` 的 `Ready` 那一支必须带 `!`（取反丢了 ⇒ 一个都没有时反而没有按钮）：
{detail}"
    );
}

/// `pages::Action::Import` → `AdoptMessage::Open`：跨层的这一跳不许被拆。
///
/// 判据走 `code_text`（注释已剥）：把那条 match 臂注释掉、或改成只打一行日志，这里就红——
/// 那正是「功能做了但没接线」的原始形态（M223 之前 `Action::Import` 就是一条占位动作）。
#[test]
fn the_intent_still_reaches_the_adopt_wizard() {
    let router = source::code_text(include_str!("../../../app/update/page_messages.rs"));
    assert!(
        source::has_code_line(
            include_str!("../../../app/update/page_messages.rs"),
            "pages::Message::Action(pages::Action::Import) =>"
        ),
        "`Action::Import` 在页面消息路由里没有分支了"
    );
    assert!(
        router.contains("self.on_adopt(crate::app::adopt::AdoptMessage::Open)"),
        "`Action::Import` 不再派发 `AdoptMessage::Open`（向导从界面上又变得不可达）"
    );
}
