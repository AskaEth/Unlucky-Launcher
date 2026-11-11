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

/// 一个数据根下的会话索引 + 事件流（可以再挂一个**只读兼容**的旧根，M188）。
#[derive(Debug, Clone)]
pub struct SessionSource {
    root: PathBuf,
    legacy: Option<PathBuf>,
}

impl SessionSource {
    /// `root` = `<数据根>/.ULL/logs`（M188 起；旧名是 `data/agent/sessions`）。
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            legacy: None,
        }
    }

    /// 再挂一个只读兼容的旧根（`data/agent/sessions`）：旧的会话仍然列得出来、打得开。
    #[must_use]
    pub fn with_legacy(mut self, legacy: impl Into<PathBuf>) -> Self {
        self.legacy = Some(legacy.into());
        self
    }

    /// 读会话时要看的根，**新根在前**。
    fn roots(&self) -> Vec<&Path> {
        let mut roots = vec![self.root.as_path()];
        if let Some(legacy) = &self.legacy {
            if legacy.is_dir() {
                roots.push(legacy.as_path());
            }
        }
        roots
    }

    /// 索引文件路径（新根）。
    #[must_use]
    pub fn index(&self) -> PathBuf {
        session_index::index_path(&self.root)
    }

    /// 全部会话（按 `started_at` 倒序，同 id 取最后一行；两个根合并，新根优先）。
    ///
    /// # Errors
    /// 索引读不了（权限/IO）时返回用户可读错误；不存在视为空列表。
    pub fn entries(&self) -> PortResult<Vec<SessionIndexEntry>> {
        let mut merged: Vec<SessionIndexEntry> = Vec::new();
        for root in self.roots() {
            let index = session_index::index_path(root);
            let entries = session_index::read_all(&index)
                .map_err(|error| format!("读会话索引 {} 失败：{error}", index.display()))?;
            for entry in entries {
                if merged
                    .iter()
                    .any(|item| item.session_id == entry.session_id)
                {
                    continue;
                }
                merged.push(entry);
            }
        }
        merged.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(merged)
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

    /// 事件流文件路径（新根没有时回落到旧根）。
    ///
    /// 解析只走 [`session_index::event_file`] 一处（M212/D2）：索引里的 `log_path` 已经是
    /// **相对会话根**的 `game/<id>.jsonl`，这里再拼一层 `game/` 就会拼成
    /// `game/game/<id>.jsonl` —— `ull agent` / MCP 的「读游戏日志」「诊断崩溃」两个工具
    /// 因此对每个真实会话都报「读会话事件流失败」，夹具写裸文件名时这个 bug 测不出来。
    #[must_use]
    pub fn event_path(&self, entry: &SessionIndexEntry) -> PathBuf {
        for root in self.roots() {
            let path = session_index::event_file(root, &entry.log_path);
            if path.is_file() {
                return path;
            }
        }
        // 两处都没有：返回**新根**的路径，调用方据此说出「该去哪找」。
        session_index::event_file(&self.root, &entry.log_path)
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SID: &str = "20261029T124501-9f3c2a";

    fn entry(log_path: &str) -> SessionIndexEntry {
        serde_json::from_value(json!({
            "v": 1,
            "session_id": SID,
            "instance_id": "inst-1",
            "instance_name": "1.20.1 Forge",
            "game_version": "1.20.1",
            "started_at": "2026-10-29T12:45:01.123Z",
            "ended_at": "2026-10-29T12:46:31.123Z",
            "duration_ms": 90000,
            "exit_code": 1,
            "exit_type": "application_error",
            "silent_failure": false,
            "window_detected": true,
            "event_count": 1,
            "log_path": log_path,
            "crash_found": false,
            "diagnosis_top": null
        }))
        .expect("索引条目")
    }

    fn temp_root(tag: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("ull-agent-events-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("game")).expect("建临时根");
        root
    }

    /// 抓得住：新根解析出的路径**正好**是写入侧落盘的那一个（`<根>/game/<id>.jsonl`）。
    /// 抓不住：旧根回落（下一条用例单独守）与事件内容解析（`ports::session` 的用例守）。
    #[test]
    fn event_path_matches_what_the_writer_wrote() {
        let root = temp_root("primary");
        let log_path = session_index::log_path_for(SID);
        let written = session_index::event_file(&root, &log_path);
        std::fs::write(&written, "{\"seq\":0}\n").expect("写事件流");

        let source = SessionSource::new(root.clone());
        let resolved = source.event_path(&entry(&log_path));

        assert_eq!(resolved, root.join("game").join(format!("{SID}.jsonl")));
        assert!(
            resolved.is_file(),
            "解析出的路径应真的是写入侧那份：{resolved:?}"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 文件暂时不在时，返回的「该去哪找」也必须是生产形状，不能多一层 `game/`。
    #[test]
    fn missing_event_file_still_points_at_the_production_shape() {
        let root = temp_root("missing");
        let log_path = session_index::log_path_for(SID);

        let source = SessionSource::new(root.clone());
        let resolved = source.event_path(&entry(&log_path));

        assert_eq!(resolved, session_index::event_file(&root, &log_path));
        assert_eq!(resolved, root.join("game").join(format!("{SID}.jsonl")));

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 旧根（`data/agent/sessions`，M188 前的生产落点）里的存量会话仍要读得回来。
    #[test]
    fn legacy_root_fallback_uses_the_same_resolution() {
        let primary = temp_root("legacy-primary");
        let legacy = temp_root("legacy-old");
        let log_path = session_index::log_path_for(SID);
        let legacy_file = session_index::event_file(&legacy, &log_path);
        std::fs::create_dir_all(legacy_file.parent().expect("父目录")).expect("建 game/");
        std::fs::write(&legacy_file, "{\"seq\":0}\n").expect("写旧根事件流");

        let source = SessionSource::new(primary.clone()).with_legacy(legacy.clone());
        let resolved = source.event_path(&entry(&log_path));

        assert_eq!(resolved, legacy_file);
        assert!(
            resolved.is_file(),
            "旧根里的存量会话应读得回来：{resolved:?}"
        );

        let _ = std::fs::remove_dir_all(&primary);
        let _ = std::fs::remove_dir_all(&legacy);
    }
}
