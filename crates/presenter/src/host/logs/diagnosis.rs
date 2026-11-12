//! 会话里落盘的诊断结论（M241）：读回时优先用它，不要拿脱敏文本重跑规则。
//!
//! ## 为什么不能重跑
//!
//! 会话 JSONL 是**写前脱敏**的，脱敏的两条形状规则有损：`≥64` 无点号不透明串会把证据行
//! 一起抹掉（`fabric.loader_0_12` 的 `org/spongepowered/…/FabricMixinTransformerProxy`
//! 66 字符 → `<redacted:opaque>`）。在脱敏文本上重跑规则必然空手——M239 实测那条 critical
//! 规则在会话路上「永不命中」。
//!
//! 修法是**换证据**而不是换判据：写会话时原始行还在内存里，那时已经算过一份结论并落进
//! `diagnosis` 事件（`launch::session::SessionWriter::finish` → `diagnose::session::conclude`），
//! 那份结论在盘上仍然是脱敏过的（`Redactor::scrub_json`）。展示侧只读它。
//!
//! 老会话（本次改动之前写的 JSONL）没有 `diagnosis` 事件 → 调用方回落到
//! [`super::diagnose_lines`]（在脱敏文本上重跑），行为与今天一致，不会凭空少一张卡片。

use serde_json::Value;
use unlucky_core::launch::session_index::SessionIndexEntry;

use crate::dto::{Diagnosis, DiagnosisReport};

use super::confidence_percent;

/// 事件流里最后一条 `diagnosis` 事件 → 日志页的诊断卡片 DTO。
///
/// 正常只会有一条（挂钩写的，或收尾补的，二选一——见 `SessionWriter::conclude_from_raw` 的门槛）。
/// 真出现多条时取**最后一条**：收尾补的那份证据最新鲜。
pub(super) fn stored(entry: &SessionIndexEntry, events: &[Value]) -> Option<DiagnosisReport> {
    let payload = events
        .iter()
        .rev()
        .find(|event| event.get("kind").and_then(Value::as_str) == Some("diagnosis"))?
        .get("payload")?;
    let candidate = payload
        .get("candidates")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .or_else(|| {
            payload
                .get("secondary")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
        })?;
    let rule = candidate.get("id").and_then(Value::as_str)?;
    let title = candidate.get("message").and_then(Value::as_str)?;
    if title.is_empty() {
        return None;
    }
    let confidence = candidate
        .get("confidence")
        .and_then(Value::as_f64)
        .unwrap_or_default();
    let hint = candidate
        .get("detail")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            payload
                .get("exit")
                .and_then(|exit| exit.get("exit_type"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "未知退出".to_string());
    let fix = candidate
        .get("fix_actions")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|action| action.get("tool"))
        .and_then(Value::as_str)
        .map(str::to_string);
    Some(DiagnosisReport {
        session: entry.session_id.clone(),
        diagnosis: Diagnosis {
            rule: rule.to_string(),
            title: title.to_string(),
            confidence: confidence_percent(confidence as f32),
            hint,
            fix,
        },
    })
}
