//! 结论与报告的数据结构（纯数据，可序列化，不持有匹配器状态）。
//!
//! HMCL 的 `CrashReportAnalyzer.Result` 直接把 `Matcher` 传出去（不可序列化、不能跨线程），
//! 这里改成「证据 + 已渲染文案 + 物化动作」的结构化记录（analyse/02 §7.10-6）。
//!
//! 由 `engine.rs` 用 `#[path]` 声明并统一导出；拆分只为守单文件 400 行上限。

use std::collections::BTreeMap;

use serde::Serialize;

use crate::diagnose::collectors::{ExitSignal, SignalOrigin};
use crate::diagnose::hs_err::HsErrReport;
use crate::diagnose::rules::{FixAction, MaterializedFixAction, RuleCategory, Severity};

/// 「结论足够确定」的可信度门槛：低于它就该交给 AI 深化（PlanDoc/06 §2.3）。
pub const CONCLUSIVE_CONFIDENCE: f32 = 0.8;

/// 一条证据（哪个来源的第几行的哪段文本）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evidence {
    /// 来源。
    pub origin: SignalOrigin,
    /// 行号（1 起；结构化信号为 0）。
    pub line: usize,
    /// 文本片段（≤300 字符）。
    pub text: String,
}

impl Evidence {
    /// 构造。
    #[must_use]
    pub fn new(origin: SignalOrigin, line: usize, text: impl Into<String>) -> Self {
        Self {
            origin,
            line,
            text: text.into(),
        }
    }

    /// 诊断输出里的字符串形式：`"<origin>:<line> <text>"`。
    #[must_use]
    pub fn render(&self) -> String {
        format!("{}:{} {}", self.origin, self.line, self.text)
    }
}

/// 一条结论。
#[derive(Debug, Clone)]
pub struct Finding {
    /// 规则 id（内置结论用 `signal.*` / `hs_err.*`）。
    pub rule_id: String,
    /// 分类。
    pub category: RuleCategory,
    /// 严重度。
    pub severity: Severity,
    /// 可信度。
    pub confidence: f32,
    /// 主因权重。
    pub priority: u8,
    /// i18n / AI 引用 id。
    pub message_key: String,
    /// 渲染后的中文结论（M16 `diagnose_crash` 直接给模型看）。
    pub message: String,
    /// 细化说明 / 处理建议。
    pub detail: Option<String>,
    /// 证据。
    pub evidence: Vec<Evidence>,
    /// 六维修复动作。
    pub fix_actions: Vec<FixAction>,
    /// 规则捕获组（供动作模板替换）。
    pub groups: BTreeMap<String, String>,
    /// 本结论会抑制哪些规则 id。
    pub suppress: Vec<String>,
    /// 被哪条结论抑制（`Some` 即次生结论）。
    pub suppressed_by: Option<String>,
}

impl Finding {
    /// 一条只有骨架的结论（内置结论的构造起点，其余字段由调用方填）。
    #[must_use]
    pub fn skeleton(
        rule_id: &str,
        category: RuleCategory,
        severity: Severity,
        confidence: f32,
        priority: u8,
        message_key: &str,
        message: &str,
    ) -> Self {
        Self {
            rule_id: rule_id.to_owned(),
            category,
            severity,
            confidence,
            priority,
            message_key: message_key.to_owned(),
            message: message.to_owned(),
            detail: None,
            evidence: Vec::new(),
            fix_actions: Vec::new(),
            groups: BTreeMap::new(),
            suppress: Vec::new(),
            suppressed_by: None,
        }
    }

    /// 物化后的修复动作（模板已替换）。
    #[must_use]
    pub fn materialized_actions(&self) -> Vec<MaterializedFixAction> {
        self.fix_actions
            .iter()
            .map(|action| action.materialize(&self.groups))
            .collect()
    }

    /// 转成诊断输出里的候选。
    #[must_use]
    pub fn to_candidate(&self) -> Candidate {
        Candidate {
            id: self.rule_id.clone(),
            severity: self.severity.as_str(),
            category: self.category.as_str(),
            confidence: self.confidence,
            message_key: self.message_key.clone(),
            message: self.message.clone(),
            detail: self.detail.clone(),
            evidence: self.evidence.iter().map(Evidence::render).collect(),
            fix_actions: self.materialized_actions(),
        }
    }

    /// 排序：priority 高 → confidence 高 → id 字典序（保证确定性）。
    #[must_use]
    pub fn rank_cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .priority
            .cmp(&self.priority)
            .then_with(|| other.confidence.total_cmp(&self.confidence))
            .then_with(|| self.rule_id.cmp(&other.rule_id))
    }
}

/// 诊断输出里的一条候选结论（字段名与 `launch-接口约定.md` §3 `diagnosis` 对齐）。
#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    /// 规则 id。
    pub id: String,
    /// 严重度。
    pub severity: &'static str,
    /// 分类。
    pub category: &'static str,
    /// 可信度。
    pub confidence: f32,
    /// i18n / AI 引用 id。
    pub message_key: String,
    /// 渲染后的中文结论。
    pub message: String,
    /// 细化说明。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// 证据字符串。
    pub evidence: Vec<String>,
    /// 六维修复动作（M16/UI 一键修复入口）。
    pub fix_actions: Vec<MaterializedFixAction>,
}

/// 退出信号摘要（诊断输出用）。
#[derive(Debug, Clone, Serialize)]
pub struct ExitRecord {
    /// 退出码。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<i32>,
    /// 退出类型（与 launch 约定同值）。
    pub exit_type: &'static str,
    /// 疑似静默失败。
    pub silent_failure: bool,
    /// 存活毫秒数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime_ms: Option<u64>,
}

/// 交给 launch 写入 `diagnosis` 事件的结构化结果。
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosisRecord {
    /// 结论来源（`rules` / `ai`）。
    pub source: &'static str,
    /// 规则没把握、需要 AI 或用户补充信息。
    pub needs_more_info: bool,
    /// 结论（`candidates[0]` 是主因）。
    pub candidates: Vec<Candidate>,
    /// 被抑制的次生结论。
    pub secondary: Vec<Candidate>,
    /// 本次判定用的退出信号摘要。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit: Option<ExitRecord>,
}

/// 一次诊断的完整结果。
#[derive(Debug, Clone, Default)]
pub struct DiagnosisReport {
    /// 结论（已排序，主因在前）。
    pub findings: Vec<Finding>,
    /// 被抑制的次生结论。
    pub secondary: Vec<Finding>,
    /// 本次判定用的退出信号。
    pub exit: Option<ExitSignal>,
    /// 评估了多少条规则（可观测性）。
    pub rules_evaluated: usize,
    /// 解析出的 hs_err（若有）。
    pub hs_err: Option<HsErrReport>,
}

impl DiagnosisReport {
    /// 主因（可能没有）。
    #[must_use]
    pub fn primary(&self) -> Option<&Finding> {
        self.findings.first()
    }

    /// 规则层有没有把握：没有任何结论，或主因可信度 < [`CONCLUSIVE_CONFIDENCE`]。
    #[must_use]
    pub fn needs_more_info(&self) -> bool {
        self.primary()
            .is_none_or(|finding| finding.confidence < CONCLUSIVE_CONFIDENCE)
    }

    /// 去重后的修复动作（主因优先，供 UI 一键修复 / agent 直接执行）。
    #[must_use]
    pub fn fix_actions(&self) -> Vec<MaterializedFixAction> {
        let mut out: Vec<MaterializedFixAction> = Vec::new();
        for action in self.findings.iter().flat_map(Finding::materialized_actions) {
            if !out.contains(&action) {
                out.push(action);
            }
        }
        out
    }

    /// 转成可序列化的诊断记录。
    #[must_use]
    pub fn to_record(&self) -> DiagnosisRecord {
        DiagnosisRecord {
            source: "rules",
            needs_more_info: self.needs_more_info(),
            candidates: self.findings.iter().map(Finding::to_candidate).collect(),
            secondary: self.secondary.iter().map(Finding::to_candidate).collect(),
            exit: self.exit.as_ref().map(|exit| ExitRecord {
                code: exit.code,
                exit_type: exit.exit_type.as_str(),
                silent_failure: exit.silent_failure,
                uptime_ms: exit.uptime_ms,
            }),
        }
    }
}
