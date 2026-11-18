//! 规则引擎：全量匹配 → 去重 → 抑制关系 → 置信度排序 → 结构化报告。
//!
//! 与 HMCL `CrashReportAnalyzer.analyze` + `GameCrashWindow.merge` 的差异（analyse/02 §7.6 §7.10）：
//! 1. 同一条规则只留**最优证据来源**的那次命中（崩溃报告 > hs_err > 日志 > stdout > stderr）；
//! 2. 结论带 `priority`/`confidence`，主因在 `findings[0]`，被抑制的次生结论进 `secondary`；
//! 3. 输出是纯数据 [`DiagnosisRecord`]（可序列化、可跨线程），不再泄漏匹配器状态；
//! 4. 退出码 / hs_err 这类**结构化信号**由内置结论补位（见 `synthetic.rs`）。
//!
//! 性能取舍：`scope: line` 逐行扫（59 条 × 行数，弱机可接受）；`scope: file` 的跨行规则
//! 只在 ≤1 MiB 文本上跑，超长日志取首尾各 512 KiB。

use std::collections::BTreeMap;

use crate::diagnose::collectors::{ExitSignal, SignalOrigin};
use crate::diagnose::hs_err::HsErrReport;
use crate::diagnose::rules::{CompiledRule, MatchScope};

#[path = "finding.rs"]
mod finding;
#[path = "synthetic.rs"]
mod synthetic;

pub use finding::{
    Candidate, DiagnosisRecord, DiagnosisReport, Evidence, ExitRecord, Finding,
    CONCLUSIVE_CONFIDENCE,
};
pub use synthetic::synthetic_findings;

/// 超过这个字节数就不再整份扫描 `scope: file` 规则（取首尾窗口）。
const FILE_SCOPE_LIMIT: usize = 1024 * 1024;
/// 超长文本的扫描窗口（首尾各一份）。
const FILE_SCOPE_WINDOW: usize = 512 * 1024;
/// 证据行截断长度（避免把整份栈塞进诊断输出）。
const EVIDENCE_MAX_CHARS: usize = 300;

/// 待匹配的一段文本（一个信号来源一份）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Haystack {
    /// 来源（决定证据前缀与取证优先级）。
    pub origin: SignalOrigin,
    /// 文本。
    pub text: String,
}

impl Haystack {
    /// 构造。
    #[must_use]
    pub fn new(origin: SignalOrigin, text: impl Into<String>) -> Self {
        Self {
            origin,
            text: text.into(),
        }
    }
}

/// 引擎输入：退出信号 + 若干待匹配文本 +（可选）解析好的 hs_err。
#[derive(Debug, Clone, Default)]
pub struct DiagnosisInput {
    /// 退出信号。
    pub exit: Option<ExitSignal>,
    /// 待匹配文本（按 `SignalOrigin::evidence_rank` 优先取证据）。
    pub haystacks: Vec<Haystack>,
    /// 解析好的 `hs_err_pid*.log`（产生内置结论，其摘要也已进 `haystacks`）。
    pub hs_err: Option<HsErrReport>,
}

impl DiagnosisInput {
    /// 空输入。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 带退出信号。
    #[must_use]
    pub fn with_exit(mut self, exit: ExitSignal) -> Self {
        self.exit = Some(exit);
        self
    }

    /// 追加一段待匹配文本。
    pub fn push_text(&mut self, origin: SignalOrigin, text: impl Into<String>) {
        self.haystacks.push(Haystack::new(origin, text));
    }

    /// 追加解析好的 hs_err：摘要进待匹配文本，结构留给内置结论。
    pub fn push_hs_err(&mut self, report: HsErrReport) {
        self.haystacks
            .push(Haystack::new(SignalOrigin::HsErr, report.digest()));
        self.hs_err = Some(report);
    }

    /// 是否什么信号都没有。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.exit.is_none() && self.haystacks.is_empty()
    }
}

/// 规则引擎（规则只编译一次，可跨线程共享）。
#[derive(Debug, Clone, Default)]
pub struct Diagnoser {
    rules: Vec<CompiledRule>,
}

impl Diagnoser {
    /// 用一组已编译的规则构造。
    #[must_use]
    pub fn new(rules: Vec<CompiledRule>) -> Self {
        Self { rules }
    }

    /// 规则表。
    #[must_use]
    pub fn rules(&self) -> &[CompiledRule] {
        &self.rules
    }

    /// 跑一遍诊断。
    #[must_use]
    pub fn analyze(&self, input: &DiagnosisInput) -> DiagnosisReport {
        let mut ordered = input.haystacks.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|haystack| haystack.origin.evidence_rank());

        let mut findings = Vec::new();
        for rule in &self.rules {
            for haystack in &ordered {
                if let Some(found) = scan(rule, haystack) {
                    findings.push(build_finding(rule, haystack, &found));
                    break;
                }
            }
        }
        findings.extend(synthetic_findings(input));

        let (mut findings, mut secondary) = split_suppressed(findings);
        findings.sort_by(Finding::rank_cmp);
        secondary.sort_by(Finding::rank_cmp);

        DiagnosisReport {
            findings,
            secondary,
            exit: input.exit.clone(),
            rules_evaluated: self.rules.len(),
            hs_err: input.hs_err.clone(),
        }
    }
}

/// 一次正则命中（绝对偏移 + 命名捕获组）。
#[derive(Debug)]
struct RawMatch {
    groups: BTreeMap<String, String>,
    offset: usize,
}

fn scan(rule: &CompiledRule, haystack: &Haystack) -> Option<RawMatch> {
    match rule.scope() {
        MatchScope::Line => scan_lines(rule, &haystack.text),
        MatchScope::Block => scan_blocks(rule, &haystack.text),
        MatchScope::File => scan_file(rule, &haystack.text),
    }
}

/// 逐行匹配（返回命中行的起始偏移，行号即证据行号）。
fn scan_lines(rule: &CompiledRule, text: &str) -> Option<RawMatch> {
    let mut offset = 0;
    for chunk in text.split_inclusive('\n') {
        let line = chunk.trim_end_matches(['\r', '\n']);
        if let Some(caps) = rule.regex().captures(line) {
            return Some(capture_groups(rule, &caps, offset));
        }
        offset += chunk.len();
    }
    None
}

/// 按空行切块匹配（一段异常 + 其上下文）。
///
/// 分隔符是**空行**而不是字面 `"\n\n"`：日志文件可能是 CRLF（Windows 上 `latest.log` 常见），
/// 按字节找 `\n\n` 会把整份日志当成一个块，`scope: block` 的规则就退化成整份扫描。
fn scan_blocks(rule: &CompiledRule, text: &str) -> Option<RawMatch> {
    let mut block_start = 0;
    let mut offset = 0;
    for chunk in text.split_inclusive('\n') {
        offset += chunk.len();
        if chunk.trim().is_empty() {
            if let Some(caps) = rule.regex().captures(&text[block_start..offset]) {
                return Some(capture_groups(rule, &caps, block_start));
            }
            block_start = offset;
        }
    }
    if block_start < text.len() {
        let caps = rule.regex().captures(&text[block_start..])?;
        return Some(capture_groups(rule, &caps, block_start));
    }
    None
}

/// 整份匹配；超长文本只看首尾窗口（跨行贪婪规则的性能边界）。
fn scan_file(rule: &CompiledRule, text: &str) -> Option<RawMatch> {
    if text.len() <= FILE_SCOPE_LIMIT {
        let caps = rule.regex().captures(text)?;
        let offset = caps.get(0).map_or(0, |found| found.start());
        return Some(capture_groups(rule, &caps, offset));
    }
    let head_end = ceil_boundary(text, FILE_SCOPE_WINDOW);
    if let Some(caps) = rule.regex().captures(&text[..head_end]) {
        let offset = caps.get(0).map_or(0, |found| found.start());
        return Some(capture_groups(rule, &caps, offset));
    }
    let tail_start = floor_boundary(text, text.len() - FILE_SCOPE_WINDOW);
    let caps = rule.regex().captures(&text[tail_start..])?;
    let offset = tail_start + caps.get(0).map_or(0, |found| found.start());
    Some(capture_groups(rule, &caps, offset))
}

fn capture_groups(rule: &CompiledRule, caps: &regex::Captures<'_>, offset: usize) -> RawMatch {
    let mut groups = BTreeMap::new();
    for name in rule.regex().capture_names().flatten() {
        if let Some(found) = caps.name(name) {
            groups.insert(name.to_owned(), found.as_str().trim().to_owned());
        }
    }
    RawMatch { groups, offset }
}

fn build_finding(compiled: &CompiledRule, haystack: &Haystack, found: &RawMatch) -> Finding {
    let rule = compiled.rule();
    let (message, detail) = rule.cause.render(&found.groups);
    Finding {
        rule_id: rule.id.clone(),
        category: compiled.category(),
        severity: rule.severity,
        confidence: rule.confidence.clamp(0.0, 1.0),
        priority: rule.priority,
        message_key: rule.cause.key.clone(),
        message,
        detail,
        evidence: vec![Evidence::new(
            haystack.origin,
            line_of(&haystack.text, found.offset),
            snippet(&haystack.text, found.offset),
        )],
        fix_actions: rule.fix_action.clone(),
        groups: found.groups.clone(),
        suppress: rule.suppress.clone(),
        suppressed_by: None,
    }
}

/// 把被 suppress 的结论拆到次生列表（不丢弃，UI 折叠展示）。
fn split_suppressed(findings: Vec<Finding>) -> (Vec<Finding>, Vec<Finding>) {
    let suppressers = findings
        .iter()
        .map(|finding| {
            (
                finding.rule_id.clone(),
                finding.priority,
                finding.suppress.clone(),
            )
        })
        .collect::<Vec<_>>();
    let mut kept = Vec::new();
    let mut suppressed = Vec::new();
    for mut finding in findings {
        let mut best: Option<(&str, u8)> = None;
        for (id, priority, suppress) in &suppressers {
            if *id == finding.rule_id || !suppress.iter().any(|item| item == &finding.rule_id) {
                continue;
            }
            if best.is_none_or(|(_, best_priority)| *priority > best_priority) {
                best = Some((id.as_str(), *priority));
            }
        }
        match best {
            Some((id, _)) => {
                finding.suppressed_by = Some(id.to_owned());
                suppressed.push(finding);
            }
            None => kept.push(finding),
        }
    }
    (kept, suppressed)
}

fn line_of(text: &str, offset: usize) -> usize {
    let offset = offset.min(text.len());
    1 + text[..offset].matches('\n').count()
}

fn snippet(text: &str, offset: usize) -> String {
    let offset = offset.min(text.len());
    let start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    let end = text[start..]
        .find('\n')
        .map_or(text.len(), |index| start + index);
    let line = text[start..end].trim();
    let mut out = line.chars().take(EVIDENCE_MAX_CHARS).collect::<String>();
    if line.chars().count() > EVIDENCE_MAX_CHARS {
        out.push('…');
    }
    out
}

fn ceil_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while index < text.len() && !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

fn floor_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}
