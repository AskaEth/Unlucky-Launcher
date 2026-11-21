//! 会话日志的**落点**：新家 `logs/`、旧家 `data/agent/sessions/`，以及两者合并的读法（M188）。
//!
//! ## 为什么要搬
//!
//! 旧落点是 `<.ULL>/data/agent/sessions/game/`：目录名里的 `agent` 是历史遗留（它从来只装**游戏**
//! 会话，与 AI 无关），而 `data/` 分区是「可重建但优先保留」的状态数据。用户投诉原话是「我们的
//! 启动日志放在哪？…他找半天没找到崩溃日志」——把日志放在 `data/agent/…` 底下，找不着是必然的。
//!
//! 新落点 `<.ULL>/logs/game/`：与启动器自己的 `launcher.log` 同一个分区、同一个顶层目录，
//! 「打开日志文件夹」一次就能看见两者。
//!
//! ## 旧数据怎么办：**不搬**
//!
//! 搬动用户已有数据是不可逆动作（中途失败留半份会话、崩溃弹窗的会话关联会断），收益只是目录好看。
//! 所以旧目录**原样留着、继续被读**：本模块的 [`entries`] 是两个根的合并视图，同 id 以新根为准。
//! 「我们没动你的数据」这件事写进 [`note_location`] 的记录里，事后可查。

use std::path::PathBuf;

use unlucky_core::launch::session_index::{self, SessionIndexEntry};
use unlucky_core::platform::layout::GAME_LOG_DIR;

use crate::dto::LogSession;

use super::{session_of, HostEnv};

/// 会话日志落点记录的文件名（`<logs>/session-log-location.json`，见 [`note_location`]）。
pub const SESSION_LOCATION_FILE: &str = "session-log-location.json";

/// 会话根（**新会话写在这里**：`logs/`；`launch::LauncherConfig` 在其下建 `game/`）。
pub fn root(env: &HostEnv) -> PathBuf {
    env.sessions_root()
}

/// 会话日志的**旧家**（`data/agent/sessions/`）：M188 起只读兼容。
///
/// 目录不存在时给 `None`——「兼容」不等于「凭空造一个旧目录出来」。
pub fn legacy_root(env: &HostEnv) -> Option<PathBuf> {
    let dir = env.legacy_sessions_root();
    dir.is_dir().then_some(dir)
}

/// 读会话要看的根，**新根在前**（同 id 撞车时新根赢）。
pub(super) fn roots(env: &HostEnv) -> Vec<PathBuf> {
    let mut roots = vec![root(env)];
    roots.extend(legacy_root(env));
    roots
}

/// 全部索引条目（两个根合并；同 id 以**先出现的**为准，即 `logs/` 那条）。
///
/// 为什么两个根都读：用户已有的会话在 `data/agent/sessions/` 下，M188 只改「新会话写到哪」，
/// 不改「旧会话还能不能看」——否则崩溃弹窗里的会话关联会突然断掉。
pub fn entries(env: &HostEnv) -> Vec<SessionIndexEntry> {
    let mut merged: Vec<SessionIndexEntry> = Vec::new();
    for dir in roots(env) {
        let index = session_index::index_path(&dir);
        for entry in session_index::read_all(&index).unwrap_or_default() {
            if merged
                .iter()
                .any(|item| item.session_id == entry.session_id)
            {
                continue;
            }
            merged.push(entry);
        }
    }
    merged.sort_by(|left, right| right.started_at.cmp(&left.started_at));
    merged
}

/// 会话列表（新→旧；`logs/` 与旧家合并）。
pub fn sessions(env: &HostEnv) -> Vec<LogSession> {
    entries(env).iter().map(session_of).collect()
}

/// 会话日志「换了个家」的记录：迁了什么、什么时候、旧路径还在不在。
///
/// ```json
/// {"v":1,"at":"…","sessions_root":"<home>/logs","legacy_root":"<home>/data/agent/sessions",
///  "legacy_present":true,"legacy_files":3,"moved":0,"policy":"read-only-compat"}
/// ```
///
/// best-effort：写不进去就算了（日志目录不可写时不该让启动器起不来）。
pub fn note_location(env: &HostEnv) -> Option<PathBuf> {
    let legacy = env.legacy_sessions_root();
    let legacy_present = legacy.is_dir();
    let legacy_files = std::fs::read_dir(legacy.join(GAME_LOG_DIR))
        .map(|items| items.filter_map(Result::ok).count())
        .unwrap_or(0);
    let body = serde_json::json!({
        "v": 1,
        "at": unlucky_core::launch::time::format_rfc3339_millis(unlucky_core::launch::time::now()),
        "sessions_root": root(env).display().to_string(),
        "legacy_root": legacy.display().to_string(),
        "legacy_present": legacy_present,
        "legacy_files": legacy_files,
        "moved": 0,
        "policy": "read-only-compat",
    });
    let path = env.logs_dir().join(SESSION_LOCATION_FILE);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok()?;
    }
    std::fs::write(&path, serde_json::to_vec_pretty(&body).ok()?).ok()?;
    Some(path)
}
