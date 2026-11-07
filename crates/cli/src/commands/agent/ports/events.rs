//! 会话数据的读取底座：索引（`index.jsonl`）+ 事件流（`<id>.jsonl`）。
//!
//! 三个端口（[`super::session::CoreSessions`] 的列表/切片、[`super::diagnose::CoreDiagnose`]
//! 的规则跑批）共用这一份读取逻辑——「同一份 jsonl 两种读法」若各写一遍，
//! 迟早出现「列表说有 5 条事件、诊断只看到 3 条」这种对不上的账。
//!
//! 形状依据 `DevDocs/SubAgentsTalk/launch-接口约定.md` §2/§5。

use std::path::{Path, PathBuf};

use serde_json::Value;
use unlucky_core::launch::session_index::{self, SessionIndexEntry};

use unlucky_agent::port::PortResult;

/// 一个数据根下的会话索引 + 事件流。
#[derive(Debug, Clone)]
pub struct SessionSource {
    root: PathBuf,
}

impl SessionSource {
    /// `root` = `<数据根>/.ULL/data/agent/sessions`。
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// 索引文件路径。
    #[must_use]
    pub fn index(&self) -> PathBuf {
        session_index::index_path(&self.root)
    }

    /// 全部会话（按 `started_at` 倒序，同 id 取最后一行）。
    ///
    /// # Errors
    /// 索引读不了（权限/IO）时返回用户可读错误；不存在视为空列表。
    pub fn entries(&self) -> PortResult<Vec<SessionIndexEntry>> {
        let index = self.index();
        let mut entries = session_index::read_all(&index)
            .map_err(|error| format!("读会话索引 {} 失败：{error}", index.display()))?;
        entries.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(entries)
    }

    /// 按完整 id 或唯一前缀找会话。
    ///
    /// # Errors
    /// 找不到 / 前缀歧义时返回用户可读错误。
    pub fn find(&self, key: &str) -> PortResult<SessionIndexEntry> {
        let entries = self.entries()?;
        if let Some(entry) = entries.iter().find(|entry| entry.session_id == key) {
            return Ok(entry.clone());
        }
        let matches: Vec<&SessionIndexEntry> = entries
            .iter()
            .filter(|entry| entry.session_id.starts_with(key))
            .collect();
        match matches.as_slice() {
            [only] => Ok((*only).clone()),
            [] => Err(format!(
                "找不到会话 `{key}`（索引：{}）",
                self.index().display()
            )),
            _ => Err(format!(
                "`{key}` 匹配到 {} 个会话，请给更长的前缀",
                matches.len()
            )),
        }
    }

    /// 一个会话的原始事件行（解析失败的行跳过，容忍被 kill 时截断的尾行）。
    ///
    /// # Errors
    /// 事件流文件读不了时返回用户可读错误。
    pub fn events(&self, entry: &SessionIndexEntry) -> PortResult<Vec<Value>> {
        let path = self.event_path(entry);
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("读会话事件流 {} 失败：{error}", path.display()))?;
        Ok(text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .collect())
    }

    /// 事件流文件路径。
    #[must_use]
    pub fn event_path(&self, entry: &SessionIndexEntry) -> PathBuf {
        self.root.join("game").join(&entry.log_path)
    }

    /// 事件里的 stdout / stderr 文本（诊断与日志阅读共用）。
    ///
    /// 返回 `(stdout 行, stderr 行, stderr 里 level=error 的条数)`。
    #[must_use]
    pub fn split_text(events: &[Value]) -> (Vec<String>, Vec<String>, u32) {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut error_lines = 0u32;
        for event in events {
            let Some(line) = event.pointer("/payload/line").and_then(Value::as_str) else {
                continue;
            };
            match event.get("kind").and_then(Value::as_str) {
                Some("stdout") => stdout.push(line.to_string()),
                Some("stderr") => {
                    stderr.push(line.to_string());
                    if event.pointer("/payload/level").and_then(Value::as_str) == Some("error") {
                        error_lines += 1;
                    }
                }
                _ => {}
            }
        }
        (stdout, stderr, error_lines)
    }
}

/// 取末尾 `tail` 行并拼成文本（诊断输入用）。
#[must_use]
pub fn tail_join(lines: &[String], tail: usize) -> String {
    let start = lines.len().saturating_sub(tail.max(1));
    let mut text = lines[start..].join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    text
}

/// 索引里有没有这个会话（存在性判断，不读事件流）。
#[must_use]
pub fn index_exists(root: &Path) -> bool {
    session_index::index_path(root).is_file()
}
