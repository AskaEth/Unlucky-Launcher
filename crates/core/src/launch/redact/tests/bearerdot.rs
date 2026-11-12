//! BEARERDOT（M243）：token 后面跟句读时也要抹，句读原位保留。
//!
//! # 漏抹的形状
//!
//! `Bearer eyJ….eyJ….c2ln….`（行尾一个英文句点）：句点自己属于 dotted 字符集
//! （[`is_dotted_token_char`]），扫描器把它算进 run，`split('.')` 多切出一个空段，
//! 「每段 ≥8」当场不成立，整串原样落盘。M240 实测 `old == new == 原文`，
//! 影响面是 core 的全部落盘/展示面（会话 JSONL、`--dry-run`、诊断规则输入）。
//!
//! # 修法
//!
//! 不是抬阈值，是**判定前把句读切出去**（[`split_trailing_punct`]）：判据只看 token 本体，
//! 句读由搬运方（`push_dotted`）原位写回。方向是对安全的那一侧：削短只会让形状判据
//! 更容易成立，不会把真 token 放过去。`(`、`)`、`,`、`;` 早就在字符集外，一并钉住，
//! 免得下次改字符集时又把这四个位置漏掉。
//!
//! # 另一半：句读不是「见到句点就抹」的入口
//!
//! 削句读之后必须证明句点分隔的普通文本没被顺手拖进来（`Service.Unavailable.today.`），
//! 这一侧由 [`sentences_ending_in_punctuation_still_survive`] 钉住；否则修法等于用
//! 「行尾有没有句点」当抹除判据，那是把 M239/M240 修掉的误杀换了个形状请回来。

use super::*;

/// 真凭据 + 句读：token 照抹，句读原位保留（不是被占位符吃掉，也不是被当噪声丢掉）。
#[test]
fn trailing_sentence_punctuation_does_not_hide_a_token() {
    let cases = [
        // form ①：行尾句点
        (
            format!("Bearer {JWT}."),
            format!("Bearer {TOKEN_PLACEHOLDER}."),
        ),
        // form ②：句点后还有正文（`"…c2ln…. then"`）
        (
            format!("the token was {JWT}. then"),
            format!("the token was {TOKEN_PLACEHOLDER}. then"),
        ),
        // form ③：逗号
        (format!("{JWT},"), format!("{TOKEN_PLACEHOLDER},")),
        // form ④：括号（左右都在字符集外，钉住说明它们从来不是判据的一部分）
        (format!("({JWT})"), format!("({TOKEN_PLACEHOLDER})")),
        // MSA/XSTS 票据被抄进日志时正是这个位置：键 + 分号
        (
            format!("xsts: {MSA};"),
            format!("xsts: {TOKEN_PLACEHOLDER};"),
        ),
        // URL 路径里的 token + 行尾句点（主机名豁免只管主机名那半）
        (
            format!("https://cdn.example.com/{JWT}."),
            format!("https://cdn.example.com/{TOKEN_PLACEHOLDER}."),
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(scrub(&input), expected, "输入：{input}");
    }
}

/// 句点分隔的普通文本：削句读不许把「行尾有句点」变成新的抹除入口。
///
/// 每一条都是 `brand_and_property_strings_survive` / `long_paths_and_class_paths_survive`
/// 里已经钉过的形状，末尾加一个句点——断言 `scrub` 逐字原样，且闸门 `has_dotted_token` 不开。
#[test]
fn sentences_ending_in_punctuation_still_survive() {
    for text in [
        "Service.Unavailable.today.",
        "the error came back. Service.Unavailable.today.",
        "sophisticatedbackpacks.upgrades.reference.",
        "xbl.xsts.entitlements.profile.",
        "Java Path: /home/alice/.minecraft/runtime/java-runtime-delta/bin/java.",
    ] {
        assert!(!has_dotted_token(text), "闸门开了：{text}");
        assert_eq!(scrub(text), text, "误杀：{text}");
    }
}

/// 两个入口在带句读的输入上仍要一致：`has_*` 开着闸门，`mask_*` 就必须真的改。
///
/// `scrub` 用 `has_dotted_token` 当闸门，闸门与搬运方对句读的看法一旦分叉（一边削、一边不削），
/// `scrub` 会静默走「闸门关着 → 什么都不抹」这条路，测试却全绿。
#[test]
fn the_has_gate_agrees_on_trailing_punctuation() {
    for case in [
        format!("Bearer {JWT}."),
        format!("{JWT},"),
        format!("({JWT})"),
        format!("the token was {MSA}. then"),
        "Service.Unavailable.today.".to_string(),
    ] {
        assert_eq!(
            has_dotted_token(&case),
            mask_dotted_tokens(&case) != case,
            "has_dotted_token 与 mask_dotted_tokens 不一致：{case}"
        );
    }
}
