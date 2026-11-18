//! 安装过程的可回溯日志（`.ULL/logs/install.log`，一轮一行）。
//!
//! 起因（M45）：Forge/NeoForge 装失败时，GUI 只有一行 Toast，processor 的 stderr 尾部
//! 与「哪一步挂了」在面板一关就没了。这里把安装过程按行追加到 `.ULL/logs/`
//! （与 `auth.jsonl` 同一分区），失败文案里带上日志路径，用户能自己回看。
//!
//! 三条纪律（与 auth 日志同源）：
//! 1. **只追加**，不轮转、不截断（体积由「一行一条、只记诊断字段」控制）；
//! 2. **只记诊断字段**：阶段、processor 坐标、退出码、已脱敏的 stderr 尾部（core 侧脱敏）；
//! 3. **写盘尽力而为**：开不了文件就静默跳过，绝不让日志失败连累安装。

use std::io::Write;
use std::path::{Path, PathBuf};

use unlucky_core::launch::time;

use super::env::HostEnv;

/// 日志文件名（在 `.ULL/logs/` 下）。
pub const INSTALL_LOG_FILE: &str = "install.log";

/// 一轮安装的日志句柄（`Clone` 很便宜：只有一个路径）。
#[derive(Debug, Clone)]
pub struct Journal {
    path: PathBuf,
}

impl Journal {
    /// 按数据根打开（`logs/` 不存在就补建；建不出来只是没有日志）。
    pub fn open(env: &HostEnv) -> Self {
        let dir = env.layout.logs();
        let _ = std::fs::create_dir_all(&dir);
        Self {
            path: dir.join(INSTALL_LOG_FILE),
        }
    }

    /// 日志路径（失败文案里给用户看）。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 追加一行（自动补 RFC3339 时间戳；写失败静默）。
    pub fn line(&self, text: &str) {
        let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        else {
            return;
        };
        let _ = writeln!(
            file,
            "[{}] {text}",
            time::format_rfc3339_millis(time::now())
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_appended_with_a_timestamp() {
        let root = std::env::temp_dir().join(format!("ull-install-journal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let env = HostEnv::at_root(&root);
        let journal = Journal::open(&env);
        journal.line("第一行");
        journal.line("第二行");
        let text = std::fs::read_to_string(journal.path()).expect("日志存在");
        assert!(text.contains("第一行") && text.contains("第二行"), "{text}");
        assert_eq!(text.lines().count(), 2);
        assert!(text.starts_with("[2"), "带时间戳：{text}");
        assert_eq!(journal.path(), env.layout.logs().join(INSTALL_LOG_FILE));
        std::fs::remove_dir_all(&root).ok();
    }
}
