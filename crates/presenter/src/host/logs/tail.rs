//! 实时尾随器（从 `host/logs.rs` 搬出：那边加了 M462 的整份重读之后贴到 400 行）。
//!
//! 一次只跟一场（`host::handle` 的 `LaunchStarted` 只在没有尾随目标时 `attach`），只读文件
//! **新增的字节**，解析出完整的 JSON 行——半行留到下一拍，免得把一次写盘切成两截的 JSON
//! 当成脏行丢掉。磁盘上的事件流是唯一真相：这里的偏移只是「已经送进正文到哪」，不是缓存。
//!
//! 两种读法都从这同一份字节里出：
//!
//! | 方法 | 读哪一段 | 谁用 |
//! |---|---|---|
//! | [`Tail::poll`] | `offset` 之后的新增字节 | `host::handle::poll_tail`（每拍一次） |
//! | [`Tail::reread`] | **整份**（读后把偏移推到同一处） | `host::mod::open_session`（M462「回到实时」） |

use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

use serde_json::Value;

use crate::dto::{LogLine, SessionId};

use super::line_of;

/// 实时尾随器：只读文件新增的字节，解析出完整的 JSON 行。
#[derive(Debug, Default)]
pub struct Tail {
    path: Option<PathBuf>,
    session: Option<SessionId>,
    offset: u64,
}

impl Tail {
    /// 开始尾随一个会话。
    pub fn attach(&mut self, path: PathBuf, session: &str) {
        self.path = Some(path);
        self.session = Some(session.to_string());
        self.offset = 0;
    }

    /// 停止尾随。
    pub fn detach(&mut self) {
        self.path = None;
        self.session = None;
        self.offset = 0;
    }

    /// 当前尾随的会话。
    pub fn session(&self) -> Option<&str> {
        self.session.as_deref()
    }

    /// 读新行（返回 `(会话 id, 新行)`；没有新内容返回 `None`）。
    pub fn poll(&mut self) -> Option<(String, Vec<LogLine>)> {
        let path = self.path.clone()?;
        let session = self.session.clone()?;
        let file = std::fs::File::open(&path).ok()?;
        let len = file.metadata().ok()?.len();
        if len <= self.offset {
            return None;
        }
        let mut file = file;
        file.seek(SeekFrom::Start(self.offset)).ok()?;
        let mut buffer = String::new();
        file.read_to_string(&mut buffer).ok()?;
        // 只吃到最后一个换行：半行留到下一拍（避免把 JSON 截断解析成脏行）
        let last_newline = buffer.rfind('\n')?;
        let complete = &buffer[..=last_newline];
        self.offset += u64::try_from(complete.len()).unwrap_or(u64::MAX);
        Some((session, lines_of(complete)))
    }

    /// 从文件头整份重读（M462：「回到实时那一场」时正文要换成这一场**已有的行**）。
    ///
    /// 与 [`Self::poll`] 的两点差别都是要的：
    ///
    /// 1. 从 0 读到底（`offset` 不参与），随后把偏移推到「最后一个完整行」的边界上——下一拍
    ///    `poll` 只拿新增字节，不会把刚读过的行再追加一遍（否则正文里每行出现两次）。
    /// 2. **不靠偏移**，所以重复调用返回同一份内容：连点两下「回到实时」时第二下的整份重读
    ///    不会把正文清空（M462 判据③「重复点击不堆叠状态」）。
    ///
    /// 文件还不存在 / 一行都没写完 ⇒ `None`（调用方据此送一份空正文，下一拍尾随补上来）。
    pub fn reread(&mut self) -> Option<Vec<LogLine>> {
        let path = self.path.clone()?;
        let text = std::fs::read_to_string(path).ok()?;
        let complete = text.rfind('\n').map_or("", |last| &text[..=last]);
        self.offset = u64::try_from(complete.len()).unwrap_or(u64::MAX);
        Some(lines_of(complete))
    }
}

/// 一段 JSONL 文本 → 日志行（空行、截断的半行、解析不了的行一律跳过）。
///
/// 实时尾随的两条读法（[`Tail::poll`] 的增量与 [`Tail::reread`] 的整份）共用这一处，免得
/// 「增量读」与「整份读」对同一份字节给出两种结果。
fn lines_of(text: &str) -> Vec<LogLine> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|event| line_of(&event))
        .collect()
}
