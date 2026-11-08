//! 审计日志：**每一次工具调用**都留痕（02 文档 §3.1 末条 / mission M16 任务 4）。
//!
//! 落盘位置由装配处给（`<data_root>/agent/audit/audit-<日期>.jsonl`，按天分文件，
//! 见 `PlanDoc/04-data-and-config.md` §2）。一行一条 JSON，只追加不修改。
//!
//! 三条纪律：
//! 1. **参数先脱敏再落盘**（[`crate::redact::redact_args`]）——`set_config` 能改 AI 端点，
//!    参数里会有 api_key；
//! 2. 写失败**不打断**工具执行（审计是旁路），但要把失败打到 stderr 留痕；
//! 3. YOLO 与「是否经确认」分别记录（`yolo=true, confirmed=false` 与
//!    `yolo=false, confirmed=true` 是两件事，事故复盘的差别就在这）。

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::clock::{date_of_unix_ms, format_unix_ms, SharedClock};
use crate::redact::redact_args;

/// 一次工具调用的结局（审计用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    /// 成功返回。
    Ok,
    /// 工具返回错误（含端口失败、未找到会话等）。
    ToolError,
    /// 参数没过 schema 校验（模型给错了参数）。
    InvalidArgs,
    /// 用户/策略拒绝。
    Denied,
    /// 只发出了确认请求，尚未执行（编排层在把请求交给闸门之前先记一条，
    /// 这样「AI 想改 X 但被拒」和「用户点了允许」在审计里是两条独立的证据）。
    AwaitingConfirmation,
    /// 模型调了不存在的工具。
    UnknownTool,
}

/// 一条审计记录。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditRecord {
    /// RFC3339 UTC。
    pub ts: String,
    /// 会话 id（agent 对话框的会话，不是游戏会话）。
    pub agent_session_id: String,
    /// 工具名。
    pub tool: String,
    /// 参数（已脱敏）。
    pub args: Value,
    /// 结局。
    pub outcome: AuditOutcome,
    /// 一句话结果摘要。
    pub summary: String,
    /// 是否运行在 YOLO 模式。
    pub yolo: bool,
    /// 是否经过用户确认（YOLO 下为 false——自动放行不是"确认"）。
    pub confirmed: bool,
    /// 工具执行耗时（毫秒）。
    pub duration_ms: u64,
}

/// 审计出口。
pub trait AuditSink: Send + Sync {
    /// 记录一条（实现方不应阻塞工具执行链；失败自行吞掉并留痕）。
    fn record(&self, record: &AuditRecord);
}

/// 按天滚动的 JSONL 审计（生产实现）。
pub struct JsonlAudit {
    dir: PathBuf,
    clock: SharedClock,
    guard: Mutex<()>,
}

impl JsonlAudit {
    /// `dir` 通常是 `<data_root>/agent/audit`；目录按需创建。
    #[must_use]
    pub fn new(dir: impl Into<PathBuf>, clock: SharedClock) -> Self {
        Self {
            dir: dir.into(),
            clock,
            guard: Mutex::new(()),
        }
    }

    /// 给定时刻对应的文件路径（测试与 UI「查看审计」共用）。
    #[must_use]
    pub fn path_for(&self, unix_ms: u64) -> PathBuf {
        self.dir
            .join(format!("audit-{}.jsonl", date_of_unix_ms(unix_ms)))
    }

    /// 追加一行；失败返回 `std::io::Error`（由 [`AuditSink::record`] 吞掉并打日志）。
    fn append(&self, record: &AuditRecord) -> std::io::Result<()> {
        let path = self.path_for(self.clock.now_unix_ms());
        let line = serde_json::to_string(record)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let _guard = self.guard.lock();
        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")
    }
}

impl AuditSink for JsonlAudit {
    fn record(&self, record: &AuditRecord) {
        if let Err(err) = self.append(record) {
            // 审计是旁路：写不进去也不能让工具调用失败，但要留下痕迹。
            eprintln!("[unlucky-agent] 审计写入失败: {err}");
        }
    }
}

/// 内存审计（测试 + 「P0 不落盘」的降级路径）。
#[derive(Debug, Default)]
pub struct MemoryAudit {
    records: Mutex<Vec<AuditRecord>>,
}

impl MemoryAudit {
    /// 空审计。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 已记录条数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.lock().map(|r| r.len()).unwrap_or(0)
    }

    /// 是否一条都没有。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 取快照。
    #[must_use]
    pub fn records(&self) -> Vec<AuditRecord> {
        self.records.lock().map(|r| r.clone()).unwrap_or_default()
    }

    /// 按工具名筛出记录（测试断言用）。
    #[must_use]
    pub fn records_for(&self, tool: &str) -> Vec<AuditRecord> {
        self.records()
            .into_iter()
            .filter(|r| r.tool == tool)
            .collect()
    }
}

impl AuditSink for MemoryAudit {
    fn record(&self, record: &AuditRecord) {
        if let Ok(mut records) = self.records.lock() {
            records.push(record.clone());
        }
    }
}

/// 丢弃审计（不推荐，只给"连内存都不想留"的极端场景/基准测试）。
#[derive(Debug, Default, Clone, Copy)]
pub struct NullAudit;

impl AuditSink for NullAudit {
    fn record(&self, _record: &AuditRecord) {}
}

/// 组装一条记录时的"其余字段"（时间戳从时钟取，参数先脱敏）。
#[derive(Debug, Clone, Copy)]
pub struct AuditDraft<'a> {
    /// 工具名。
    pub tool: &'a str,
    /// 原始参数（内部会脱敏）。
    pub args: &'a Value,
    /// 结局。
    pub outcome: AuditOutcome,
    /// 一句话结果摘要。
    pub summary: &'a str,
    /// 是否 YOLO。
    pub yolo: bool,
    /// 是否经用户确认。
    pub confirmed: bool,
    /// 耗时（毫秒）。
    pub duration_ms: u64,
}

/// 组装一条审计记录（时间戳从时钟取、参数脱敏，调用方不用自己拼）。
#[must_use]
pub fn make_record(
    clock: &dyn crate::clock::Clock,
    agent_session_id: &str,
    draft: AuditDraft<'_>,
) -> AuditRecord {
    AuditRecord {
        ts: format_unix_ms(clock.now_unix_ms()),
        agent_session_id: agent_session_id.to_string(),
        tool: draft.tool.to_string(),
        args: redact_args(draft.args),
        outcome: draft.outcome,
        summary: draft.summary.to_string(),
        yolo: draft.yolo,
        confirmed: draft.confirmed,
        duration_ms: draft.duration_ms,
    }
}

/// 便捷：给出数据根下的审计目录（`<data_root>/agent/audit`）。
#[must_use]
pub fn audit_dir(data_root: &Path) -> PathBuf {
    data_root.join("agent").join("audit")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FixedClock;
    use serde_json::json;
    use std::sync::Arc;

    #[test]
    fn record_redacts_args_before_store() {
        let audit = MemoryAudit::new();
        let clock = FixedClock::new(1_793_304_753_123);
        let args = json!({ "path": "ai.api_key", "value": "sk-x" });
        let rec = make_record(
            &clock,
            "s1",
            AuditDraft {
                tool: "set_config",
                args: &args,
                outcome: AuditOutcome::Ok,
                summary: "ok",
                yolo: false,
                confirmed: true,
                duration_ms: 3,
            },
        );
        audit.record(&rec);
        let stored = &audit.records()[0];
        assert_eq!(stored.args["value"], json!("***"), "敏感路径的值必须掩码");
        assert_eq!(stored.ts, "2026-10-29T20:12:33.123Z");
    }

    #[test]
    fn jsonl_audit_writes_one_line_per_record_per_day() {
        let dir = std::env::temp_dir().join(format!("unlucky-agent-audit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let clock: SharedClock = Arc::new(FixedClock::new(1_793_304_753_123));
        let audit = JsonlAudit::new(&dir, Arc::clone(&clock));
        let args = json!({});
        let rec = make_record(
            clock.as_ref(),
            "s1",
            AuditDraft {
                tool: "get_config",
                args: &args,
                outcome: AuditOutcome::Ok,
                summary: "ok",
                yolo: false,
                confirmed: false,
                duration_ms: 1,
            },
        );
        audit.record(&rec);
        audit.record(&rec);
        let path = dir.join("audit-2026-10-29.jsonl");
        let content = std::fs::read_to_string(&path).expect("审计文件应存在");
        assert_eq!(content.lines().count(), 2);
        let first: AuditRecord = serde_json::from_str(content.lines().next().unwrap()).unwrap();
        assert_eq!(first.tool, "get_config");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
