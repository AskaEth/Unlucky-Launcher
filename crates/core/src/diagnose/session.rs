//! 会话收尾的规则结论：**在原始行还热着的时候**算一次（M241）。
//!
//! ## 为什么不能等读回 JSONL 再算
//!
//! 会话事件流是「写前先脱敏」的（[`crate::launch::redact`]，`落盘即脱敏`），而脱敏的两条
//! 形状规则是**有损**的：`≥64` 无点号不透明串、带点 token。于是「读回 JSONL → 喂规则引擎」
//! 这条路看到的不是游戏说的话，而是**被抹过的**游戏说的话。M239 实测：`fabric.loader_0_12`
//! （critical / priority 80）的唯一证据行
//! `java.lang.NoClassDefFoundError: org/spongepowered/…/FabricMixinTransformerProxy`
//! 里的类路径 66 字符、无点号，正好撞上 `≥64` 那条规则 → 变成 `<redacted:opaque>` →
//! 规则在会话路上**永不命中**。
//!
//! ## 分工（`DevDocs/SubAgentsTalk/launch-接口约定.md` §3）
//!
//! 「launch 报事实、M15 下结论」没有变：launch 把**原始事实**（退出码、存活时长、原始
//! stdout/stderr 尾窗）交过来，结论在 `diagnose` 里算，产出的就是 `diagnosis` 事件的
//! payload（[`DiagnosisReport::to_record`]）。落盘仍然只留**已脱敏**的结论：写会话的
//! `SessionWriter::diagnosis` 会再走一遍 `Redactor::scrub_json`。
//!
//! 换句话说：**原始文本只活在内存里**，盘上永远只有脱敏文本——不是「再存一份原文」。

use serde_json::Value;

use crate::diagnose::collectors::{ExitSignal, SignalOrigin};
use crate::diagnose::engine::{Diagnoser, DiagnosisInput};
use crate::diagnose::loader::RuleLoader;

/// 会话收尾时交给规则引擎的**原始**事实（未脱敏，只在本进程内存里）。
#[derive(Debug, Clone, Copy)]
pub struct RawFacts<'a> {
    /// 退出码（`None` = 没有退出判定）。
    pub exit_code: Option<i32>,
    /// 进程存活毫秒数。
    pub duration_ms: u64,
    /// 全量错误行数（不受尾窗影响）。
    pub error_lines: u32,
    /// 原始 stdout 尾窗（按时间顺序）。
    pub stdout: &'a [String],
    /// 原始 stderr 尾窗（按时间顺序）。
    pub stderr: &'a [String],
}

impl RawFacts<'_> {
    /// 有没有任何可匹配的文本。
    fn has_text(&self) -> bool {
        !self.stdout.is_empty() || !self.stderr.is_empty()
    }
}

/// 跑一遍规则引擎，返回 `diagnosis` 事件的 payload；没有结论时返回 `None`。
///
/// 返回 `None` 的两种情况都**不写事件**：没有任何文本可匹配（无从判断），或规则引擎没认出
/// 已知问题（此时读回 JSONL 的那条兜底路径看到的是同一份证据，重跑也是空手）。
#[must_use]
pub fn conclude(facts: &RawFacts<'_>) -> Option<Value> {
    if !facts.has_text() {
        return None;
    }
    let stderr_text = facts.stderr.join("\n");
    let exit = ExitSignal::classify(
        facts.exit_code,
        Some(facts.duration_ms),
        facts.error_lines,
        &stderr_text,
    );
    let mut input = DiagnosisInput::new().with_exit(exit);
    if !facts.stdout.is_empty() {
        input.push_text(SignalOrigin::Stdout, facts.stdout.join("\n"));
    }
    if !facts.stderr.is_empty() {
        input.push_text(SignalOrigin::Stderr, facts.stderr.join("\n"));
    }
    let report = Diagnoser::new(rules()).analyze(&input);
    report.primary()?;
    serde_json::to_value(report.to_record()).ok()
}

/// 内嵌规则集编译一次的那份规则（每次重新取；`RuleLoader` 自带缓存语义）。
fn rules() -> Vec<crate::diagnose::rules::CompiledRule> {
    RuleLoader::embedded().rule_set().rules().to_vec()
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
