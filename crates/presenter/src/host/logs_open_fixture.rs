//! 会话相关的 host 用例共用的**盘面夹具**（M458 / M462 两份用例都建在这上面）。
//!
//! 从 `logs_open_tests.rs` 原样搬出（M462 的用例要同一份真索引 + 真事件流；两份用例各抄一份
//! 夹具就是两个会漂的真相源）。零行为变化：函数体逐字未改。

use std::path::PathBuf;

use unlucky_core::launch::session_index::{self, SessionIndexEntry};

use crate::dto::{AppEvent, Backend, CoreEvent, LaunchFinished, LogOutcome};
use crate::state::AppState;

use super::env::HostEnv;
use super::CoreHost;

/// 临时数据根（`HostEnv::at_root` ⇒ 不做多根发现，盘面只有我们写的东西）。
pub(super) struct TempRoot(PathBuf);

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 建一个只有这几场**历史会话**的数据根：`(id, 事件流里那一行文本)`。
///
/// `label` 在**并行**的用例之间必须各不相同（目录按 `label + pid` 拼，同名会互相删）。
pub(super) fn fixture(label: &str, sessions: &[(&str, &str)]) -> (HostEnv, TempRoot) {
    let dir = std::env::temp_dir().join(format!("ull-m458-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let env = HostEnv::at_root(&dir);
    let index = session_index::index_path(&env.sessions_root());
    for (id, text) in sessions {
        // `log_path` 必须是生产写入的形状（相对会话根：`game/<id>.jsonl`），见 M211/D2；
        // 夹具与生产各自自洽会让「读取侧多 join 一层」永远绿。
        let entry = SessionIndexEntry {
            session_id: (*id).to_string(),
            instance_id: "dev".to_string(),
            instance_name: "开发测试".to_string(),
            started_at: "2026-11-02T18:40:00.000Z".to_string(),
            ended_at: "2026-11-02T18:41:00.000Z".to_string(),
            duration_ms: 60_000,
            exit_code: Some(1),
            exit_type: Some("application_error".to_string()),
            log_path: format!("game/{id}.jsonl"),
            ..SessionIndexEntry::default()
        };
        session_index::append(&index, &entry).expect("写索引");
        let file = env.sessions_root().join(format!("game/{id}.jsonl"));
        std::fs::create_dir_all(file.parent().expect("父目录")).expect("建会话目录");
        std::fs::write(
            &file,
            format!("{{\"kind\":\"stdout\",\"payload\":{{\"line\":\"{text}\"}}}}\n"),
        )
        .expect("写事件流");
    }
    (env, TempRoot(dir))
}

/// 这一场刚收尾 ⇒ `selected_session` 停在它上面（线上同一路径：`CoreEvent::LaunchFinished`）。
pub(super) fn finished(state: &mut AppState, session: &str) {
    state.apply_core(CoreEvent::LaunchFinished(LaunchFinished {
        id: "dev".to_string(),
        session: session.to_string(),
        outcome: LogOutcome::Crashed,
        exit_code: Some(1),
        uptime_ms: 60_000,
        failure: None,
    }));
}

/// 把 host 这一轮发的事件全部灌进 state，返回事件本身（按发出顺序）。
pub(super) fn drain_into(host: &mut CoreHost, state: &mut AppState) -> Vec<CoreEvent> {
    let events: Vec<CoreEvent> = host
        .drain_events()
        .into_iter()
        .filter_map(|event| match event {
            AppEvent::Core(core) => Some(core),
            _ => None,
        })
        .collect();
    for core in &events {
        state.apply_core(core.clone());
    }
    events
}
