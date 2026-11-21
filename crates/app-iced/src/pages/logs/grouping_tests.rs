//! **分组轴**的判据（M394 `SEVGROUP`）：分组由 `severity` 决定、不由置信度决定。
//!
//! 为什么单独一份：`pages/logs/tests.rs` 已经 380 行上下，贴着 400 行硬线（`guards.yml` 的
//! `file-size`）。这里只放「分组轴 + 组内排序 + 旧维度还在不在」三条判据，其余页面断言仍在
//! `tests.rs`（索引见 `pages/logs.rs` 的 `mod` 列表）。
//!
//! | 用例 | 抓得住 |
//! |---|---|
//! | [`grouping_follows_severity_and_not_confidence`] | 分组轴被换回置信度：只改 `severity` 而不改置信度时，结论**必须**换组 |
//! | [`within_a_group_confidence_decides_the_order`] | 组内排序丢了（同一档里最强的沉底）/ 排序改成不稳定的 |
//! | [`the_card_still_shows_confidence`] | 「换分组把旧维度删了」：卡片末行不再画置信度 |

use std::collections::BTreeMap;

use crate::pages::layout::source;

use super::model::{Confidence, Finding, Logs, Severity, GROUPS};

/// 造一条结论：`severity` 与置信度**各自独立**给——判据要的就是「两个轴能分别拨」。
fn finding(rule: &str, severity: Severity, confidence: u8) -> Finding {
    Finding {
        rule: rule.to_owned(),
        cause_key: format!("diagnose.cause.{rule}"),
        severity,
        confidence_band: Confidence::of(confidence),
        confidence,
        groups: BTreeMap::new(),
        system_text: None,
        action: None,
    }
}

fn logs_of(findings: Vec<Finding>) -> Logs {
    Logs {
        findings,
        ..Logs::default()
    }
}

/// **本 mission 的主判据**：把 `severity` 改一个值，分组就必须变——置信度一个字节没动。
///
/// 抓得住：
///
/// - 分组判据从 `finding.severity` 漂回 `finding.confidence_band` / `Confidence::of(confidence)`
///   （那样「只改 severity」的结论留在原组，下面每一条都会红）；
/// - 有人把两个轴揉成一个（比如 `severity` 只当配色、分组仍看置信度）——两个方向都试了：
///   高置信度 + 低严重度（`other.debug_crash` 的 0.95/info）不许冒到第一档，
///   低置信度 + 高严重度（`mod.mixin_failure` 的 0.70/critical）不许沉到最后一档；
/// - `GROUPS` 漏掉某一档（漏掉的那档结论就永远画不出来）。
///
/// 抓不住：分组标题的**文案**对不对（那是 `tests.rs::every_key_the_page_uses_is_translated_in_both_languages`
/// 与金表的事），也不管屏幕上的组头有没有被裁掉（那要看截图）。
#[test]
fn grouping_follows_severity_and_not_confidence() {
    // ① 同一条结论、同一份置信度（95 ⇒ 高档），只把 severity 从 warning 改成 info。
    let before = logs_of(vec![finding(
        "graphics.gl_operation_failure",
        Severity::Warning,
        95,
    )]);
    let after = logs_of(vec![finding(
        "graphics.gl_operation_failure",
        Severity::Info,
        95,
    )]);

    assert_eq!(
        before.group(Severity::Warning).len(),
        1,
        "95% 置信度 + warning 严重度 ⇒ 落在 warning 组"
    );
    assert!(before.group(Severity::Info).is_empty());
    assert_eq!(
        after.group(Severity::Info).len(),
        1,
        "只把 severity 改成 info（置信度仍是 95%），它必须换到 info 组"
    );
    assert!(
        after.group(Severity::Warning).is_empty(),
        "换了组就得离开原组（否则同一条结论会被画两遍）"
    );

    // ② 反向：低置信度 + 高严重度不许被置信度拖下去。
    let low_confidence_critical =
        logs_of(vec![finding("mod.mixin_failure", Severity::Critical, 40)]);
    assert_eq!(
        low_confidence_critical.group(Severity::Critical).len(),
        1,
        "40% 置信度 + critical 严重度 ⇒ 仍在第一档（按置信度分组时它会沉到最后一档）"
    );
    assert!(low_confidence_critical.group(Severity::Info).is_empty());

    // ③ 置信度档相同、severity 不同 ⇒ 落在不同组（置信度若还参与分组，这两条会挤在一起）。
    let shared_band = logs_of(vec![
        finding("integrity.unsatisfied_link", Severity::Critical, 85),
        finding("other.debug_crash", Severity::Info, 85),
    ]);
    assert_eq!(shared_band.group(Severity::Critical).len(), 1);
    assert_eq!(shared_band.group(Severity::Info).len(), 1);
    assert_eq!(
        shared_band.findings[0].confidence_band, shared_band.findings[1].confidence_band,
        "这两条的置信度档本就相同——只有 severity 能分开它们"
    );

    // ④ 每一档都收得下自己的结论（`GROUPS` 覆盖模型里全部 severity 值）。
    for severity in GROUPS {
        let single = logs_of(vec![finding("a.b", severity, 50)]);
        assert_eq!(
            single.group(severity).len(),
            1,
            "{} 组收不到自己的结论",
            severity.slug()
        );
    }
}

/// **组内排序**（M394 对齐的第二样）：同一档严重度里按置信度**降序**，同分保持 presenter 顺序。
///
/// 抓得住：排序被删掉（回到 presenter 原序：最强的沉底）、方向反了（最弱的排最前）、
/// 换成不稳定排序（同分的三条被调换——`sort_unstable_by` 在三条以上的实现里会重排）。
///
/// 抓不住：排序到底画在屏幕哪一行（那要看截图）。
#[test]
fn within_a_group_confidence_decides_the_order() {
    let logs = logs_of(vec![
        finding("weakest", Severity::Warning, 55),
        finding("strongest", Severity::Warning, 90),
        finding("middle", Severity::Warning, 70),
    ]);
    let order: Vec<&str> = logs
        .group(Severity::Warning)
        .iter()
        .map(|finding| finding.rule.as_str())
        .collect();
    assert_eq!(
        order,
        ["strongest", "middle", "weakest"],
        "组内必须按置信度降序（presenter 的顺序不是判据）"
    );

    // 同分：稳定排序 ⇒ 保持 presenter 给的相对顺序（页面不编第二判据）。
    let ties = logs_of(vec![
        finding("first", Severity::Critical, 70),
        finding("second", Severity::Critical, 70),
        finding("third", Severity::Critical, 70),
    ]);
    let order: Vec<&str> = ties
        .group(Severity::Critical)
        .iter()
        .map(|finding| finding.rule.as_str())
        .collect();
    assert_eq!(order, ["first", "second", "third"], "同分时不许重排");
}

/// **换分组不等于删旧维度**：置信度仍要画在卡片上（M394 的第三条硬要求）。
///
/// 抓得住：有人「顺手清理」掉 `finding_block` 末行那段——置信度从界面上彻底消失
/// （摘要里那一行不是每屏都看，卡片才是每屏都在的）；也抓得住「组头又换回置信度 key」
/// （两套 vocabulary 并存 ⇒ 下一个人不知道该信哪个）。
///
/// 判据走 `source::code_line`（T29）：把接线**注释掉**而字符串还留在注释里时，`contains`
/// 形式的断言仍会绿，这里不会。
///
/// 抓不住：那一行有没有被滚动区裁掉（那要看窄窗截图）。
#[test]
fn the_card_still_shows_confidence() {
    let block = source::function_region(include_str!("diagnostic.rs"), "fn finding_block");
    assert!(
        source::code_line(&block, "Key::LogsConfidence,").is_some(),
        "诊断卡片不再画置信度（换分组把旧维度删了）：\n{block}"
    );

    let key_of = source::function_region(include_str!("diagnostic.rs"), "fn key_of");
    for line in [
        "Severity::Critical => Key::LogsSeverityCritical,",
        "Severity::Warning => Key::LogsSeverityWarning,",
        "Severity::Info => Key::LogsSeverityInfo,",
    ] {
        assert!(
            source::code_line(&key_of, line).is_some(),
            "分组头没按严重度取 key（缺 `{line}`）：\n{key_of}"
        );
    }
    assert!(
        !source::code_text(&key_of).contains("LogsConfidence"),
        "分组头还挂着置信度的 key（两套分组词汇并存）：\n{key_of}"
    );
}
