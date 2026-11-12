//! 健康度的落盘格式与读盘/写盘（`cache/health.json`）。
//!
//! 与 `health.rs` 分开的原因很实际：序列化格式与文件 IO 的细节会随版本增长（字段增删、
//! 迁移、按源分文件…），把这些从「打分/冷却」的纯逻辑里挪出来，两边都好读好改。

use std::collections::HashMap;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::atomic::write_atomic;
use super::health::{HealthKey, SourceHealth};
use super::types::ResourceKind;

/// 健康度文件名（放在 `cache/` 下，与 `index.json` 同级）。
pub const FILE_NAME: &str = "health.json";
/// 记录保留期：超过这个时间没更新过的源直接丢掉（缓存目录可能被放很久）。
pub const MAX_AGE_MS: u64 = 7 * 24 * 60 * 60 * 1000;

/// 持久化的条目形态（`HealthKey` 拍平，便于 JSON 阅读）。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedEntry {
    source: String,
    kind: ResourceKind,
    health: SourceHealth,
}

/// 持久化文件的容器。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedFile {
    version: u32,
    #[serde(default)]
    entries: Vec<PersistedEntry>,
}

/// 读盘：不存在/损坏/超龄都只丢掉对应记录，绝不阻断启动。
pub(crate) fn load(path: &Path, max_age_ms: u64, now_ms: u64) -> HashMap<HealthKey, SourceHealth> {
    let mut entries = HashMap::new();
    let Ok(bytes) = std::fs::read(path) else {
        return entries;
    };
    match serde_json::from_slice::<PersistedFile>(&bytes) {
        Ok(file) => {
            for entry in file.entries {
                if entry.health.updated_at_ms.saturating_add(max_age_ms) >= now_ms {
                    entries.insert(HealthKey::new(entry.source, entry.kind), entry.health);
                }
            }
        }
        Err(err) => warn(&format!("健康度文件解析失败，按空表处理：{err}")),
    }
    entries
}

/// 写盘：原子写（`tmp → rename`），失败由调用方决定是记日志还是忽略。
pub(crate) fn store(path: &Path, entries: &HashMap<HealthKey, SourceHealth>) -> io::Result<()> {
    let file = PersistedFile {
        version: 1,
        entries: entries
            .iter()
            .map(|(key, health)| PersistedEntry {
                source: key.source.clone(),
                kind: key.kind,
                health: health.clone(),
            })
            .collect(),
    };
    let json = serde_json::to_vec_pretty(&file).map_err(io::Error::other)?;
    write_atomic(path, &json)
}

/// core 不引 tracing（P0 阶段日志由 app 层接管），这里只做一次静默的 stderr 兜底。
fn warn(message: &str) {
    if cfg!(debug_assertions) {
        eprintln!("[ull::download] {message}");
    } else {
        let _ = message;
    }
}
