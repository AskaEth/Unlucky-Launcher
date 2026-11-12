//! 会话记录索引：`list_game_sessions` 的数据源。
//!
//! 文件：`<sessions_root>/game/index.jsonl`，每会话一行摘要，会话结束时追加。
//! 读取约定（`DevDocs/SubAgentsTalk/launch-接口约定.md` §2）：同一 `session_id`
//! 可能出现多行（崩溃后重写索引），读取方**按 id 去重取最后一行**，再按 `started_at` 倒序。

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::platform::layout::GAME_LOG_DIR;

/// 索引条目的 schema 版本。
pub const INDEX_VERSION: u8 = 1;

/// 一次会话的摘要。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionIndexEntry {
    /// schema 版本。
    #[serde(default)]
    pub v: u8,
    /// 会话 id。
    pub session_id: String,
    /// 实例 id。
    pub instance_id: String,
    /// 实例名。
    pub instance_name: String,
    /// 游戏版本。
    pub game_version: String,
    /// 开始时间（RFC3339 UTC）。
    pub started_at: String,
    /// 结束时间（RFC3339 UTC）。
    pub ended_at: String,
    /// 时长毫秒。
    pub duration_ms: u64,
    /// 退出码（未跑到进程时为 `None`）。
    pub exit_code: Option<i32>,
    /// 退出判定字符串。
    pub exit_type: Option<String>,
    /// 是否静默失败。
    pub silent_failure: bool,
    /// 是否检测到游戏窗口。
    pub window_detected: bool,
    /// 事件条数。
    pub event_count: u64,
    /// 事件流相对**会话根**的路径（生产形状 `game/<session-id>.jsonl`）。
    ///
    /// 契约里「相对谁」只有这一句，见 [`event_file`]。M211 之前这里写成「相对 `game/`」，
    /// 而写入侧一直按「相对会话根」写 —— 两个说法各自成立，两处读取点于是都多 join 了一层
    /// `game/`，`ull logs` / `ull diag` 对每个真实会话都报「找不到事件流」。
    pub log_path: String,
    /// 是否抓到崩溃报告/hs_err。
    pub crash_found: bool,
    /// 诊断首因（M15 写入的第一个候选 id）。
    pub diagnosis_top: Option<String>,
}

/// 索引文件路径（`<sessions_root>/game/index.jsonl`）。
pub fn index_path(sessions_root: &Path) -> PathBuf {
    sessions_root.join("game").join("index.jsonl")
}

/// 写入侧用的 `log_path`：**相对会话根**（`<sessions_root>`）的形状，`game/<id>.jsonl`。
///
/// 写入侧（[`crate::launch::session::SessionWriter`]）与读取侧（[`event_file`]）都只认这一个
/// 函数产出的形状，所以「存什么」与「怎么找回来」不可能再各说一套（M211/D2）。
pub fn log_path_for(session_id: &str) -> String {
    format!("{GAME_LOG_DIR}/{session_id}.jsonl")
}

/// `log_path` → 事件流文件（相对**会话根**解析，M211/D2）。
///
/// 这是全仓唯一的解析点：CLI 的 `ull logs` / `ull diag` 与 presenter 的日志页都走这里，
/// 谁也不再自己往前拼一层 `game/`。
///
/// 存量数据不需要迁移：索引里的 `log_path` 从第一版起就是 `game/<id>.jsonl`，正是本函数的
/// 输入形状（`<root>/game/<id>.jsonl`），只是之前的读取侧又拼了一次 `game/` 才找不到。
pub fn event_file(sessions_root: &Path, log_path: &str) -> PathBuf {
    sessions_root.join(log_path)
}

/// 追加一行摘要。
pub fn append(index: &Path, entry: &SessionIndexEntry) -> std::io::Result<()> {
    if let Some(parent) = index.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(index)?;
    let line = serde_json::to_string(entry)
        .map_err(|err| std::io::Error::other(format!("序列化索引失败：{err}")))?;
    file.write_all(line.as_bytes())?;
    file.write_all(b"\n")?;
    file.flush()
}

/// 读取全部条目（同一 id 取最后一行，按开始时间倒序）。
pub fn read_all(index: &Path) -> std::io::Result<Vec<SessionIndexEntry>> {
    let file = match File::open(index) {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err),
    };
    let mut entries: Vec<SessionIndexEntry> = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // 容忍坏行（半截 JSON）：索引是“尽力而为”的数据源，不能因为一行坏数据丢掉整份列表。
        let Ok(entry) = serde_json::from_str::<SessionIndexEntry>(trimmed) else {
            continue;
        };
        match entries
            .iter_mut()
            .find(|item| item.session_id == entry.session_id)
        {
            Some(slot) => *slot = entry,
            None => entries.push(entry),
        }
    }
    entries.sort_by(|left, right| right.started_at.cmp(&left.started_at));
    Ok(entries)
}

/// `list_game_sessions` 语义：可选实例过滤 + 条数上限，按开始时间倒序。
pub fn list(
    index: &Path,
    limit: usize,
    instance_id: Option<&str>,
) -> std::io::Result<Vec<SessionIndexEntry>> {
    let entries = read_all(index)?;
    Ok(entries
        .into_iter()
        .filter(|entry| match instance_id {
            Some(id) => entry.instance_id == id,
            None => true,
        })
        .take(limit)
        .collect())
}
