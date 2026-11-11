//! `diagnose::session` 的单测（拆文件守 400 行上限）。

use super::*;
use crate::launch::redact::Redactor;

/// `fabric.loader_0_12`（critical / priority 80）在 `rules.d/fabric.yaml` 里的唯一证据行。
const EVIDENCE: &str = "java.lang.NoClassDefFoundError: \
                        org/spongepowered/asm/mixin/transformer/FabricMixinTransformerProxy";

fn facts<'a>(stdout: &'a [String], stderr: &'a [String]) -> RawFacts<'a> {
    RawFacts {
        exit_code: Some(1),
        duration_ms: 4_200,
        error_lines: 2,
        stdout,
        stderr,
    }
}

fn record_id(record: &Value) -> Option<&str> {
    record
        .get("candidates")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("id"))
        .and_then(Value::as_str)
}

/// **判据 ①**：原始证据行能命中 `fabric.loader_0_12`；M240 收窄脱敏之后，同一行在脱敏文本里**也**能命中。
///
/// 这半边在 M240 之前是「命不中」（那是 M239 报的 bug）。M240 把词形长路径从「不透明串」里放了出来，
/// 于是这条证据不再被抹——**证据能留在盘上正是 M240 的目的**，所以这里改成断言「两边都能命中」。
/// 原始窗口（`RawWindow`）对「证据本身含凭据、被迫抹掉」的行依然必需，但那种行需要一条
/// 真的依赖凭据字面量的规则才能演示，本仓当前没有这样的规则。
#[test]
fn raw_evidence_hits_the_fabric_rule_and_m240_keeps_it_visible_after_scrubbing() {
    let raw = [EVIDENCE.to_string()];
    let empty: [String; 0] = [];

    let record = conclude(&facts(&empty, &raw)).expect("原始证据应当有结论");
    assert_eq!(record_id(&record), Some("fabric.loader_0_12"), "{record}");
    assert_eq!(record["source"], serde_json::json!("rules"));
    assert_eq!(record["needs_more_info"], serde_json::json!(false));

    let scrubbed = [Redactor::new().scrub(EVIDENCE).into_owned()];
    assert!(
        !scrubbed[0].contains("<redacted:opaque>"),
        "夹具前提（M240 之后）：词形长路径不再被抹成不透明串：{}",
        scrubbed[0]
    );
    assert!(
        scrubbed[0].contains("FabricMixinTransformerProxy"),
        "夹具前提：类名应原样留在脱敏文本里：{}",
        scrubbed[0]
    );
    let after = conclude(&facts(&empty, &scrubbed));
    assert_eq!(
        after.as_ref().and_then(record_id),
        Some("fabric.loader_0_12"),
        "M240 之后脱敏文本也能命中；若这条变红，说明长类名又被抹了——那时请改用 raw 窗口的用例重新说明它为什么必要"
    );
}

/// 没有文本就没有结论（不写空事件）。
#[test]
fn no_text_means_no_conclusion() {
    let empty: [String; 0] = [];
    assert!(conclude(&facts(&empty, &empty)).is_none());
}

/// 认不出来的日志也不写事件（让读回 JSONL 的兜底路径去跑，避免两条路给出不同结论）。
#[test]
fn unrecognized_output_means_no_conclusion() {
    let stdout = ["[12:00:00] [main/INFO]: Setting user".to_string()];
    let empty: [String; 0] = [];
    assert!(conclude(&facts(&stdout, &empty)).is_none());
}

/// 结论里带的是**已渲染**的文案与物化动作，展示侧不必再自己拼。
#[test]
fn the_record_carries_rendered_text_and_actions() {
    let raw = ["Fabric loader failed".to_string(), EVIDENCE.to_string()];
    let empty: [String; 0] = [];
    let record = conclude(&facts(&empty, &raw)).expect("应有结论");
    let candidate = &record["candidates"][0];
    assert!(!candidate["message"].as_str().unwrap_or_default().is_empty());
    assert_eq!(
        candidate["fix_actions"][0]["kind"],
        serde_json::json!("reinstall")
    );
    assert!(
        candidate["evidence"][0]
            .as_str()
            .unwrap_or_default()
            .contains("NoClassDefFoundError"),
        "{candidate}"
    );
}
