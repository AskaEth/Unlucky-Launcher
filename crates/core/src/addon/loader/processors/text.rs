//! 进程输出的尾部截取与脱敏。
//!
//! processor 的 stdout/stderr 是外部程序（而且部分步骤要联网）写的，内容不可控：
//! 可能带绝对路径、用户名、甚至一大段堆栈。进错误消息/`--json` 前必须做两件事：
//! **只留尾巴**（够定位、不灌满日志）与**抹掉主目录**（`C:\Users\<名>` → `<home>`）。

use std::path::PathBuf;

use crate::launch::redact::Redactor;

/// 尾巴保留的行数（够看异常栈头）。
const TAIL_LINES: usize = 12;
/// 尾巴保留的字符上限。
const TAIL_CHARS: usize = 1200;

/// 宿主脱敏器：把用户主目录换成 `<home>`。
///
/// 复用 [`crate::launch::redact::Redactor`]（与 M15/M31 落地的同一套规则：token、
/// `<home>`、长不透明串、带点 token/JWT）。注册 `USERPROFILE`（Windows）/`HOME`（macOS、Linux）
/// 两个来源，因为 MSYS/CI 环境下两者可能只有一个在场。
#[must_use]
pub fn host_redactor() -> Redactor {
    let mut redactor = Redactor::new();
    for key in ["USERPROFILE", "HOME"] {
        if let Some(value) = std::env::var_os(key) {
            redactor = redactor.with_home(PathBuf::from(value));
        }
    }
    redactor
}

/// 取文本尾部（末 [`TAIL_LINES`] 行，再截到 [`TAIL_CHARS`] 字符）。
#[must_use]
pub fn tail(text: &str) -> String {
    let trimmed = text.trim_end();
    if trimmed.is_empty() {
        return String::new();
    }
    let lines: Vec<&str> = trimmed.lines().collect();
    let start = lines.len().saturating_sub(TAIL_LINES);
    let mut joined = lines[start..].join("\n");
    let length = joined.chars().count();
    if length > TAIL_CHARS {
        joined = joined.chars().skip(length - TAIL_CHARS).collect();
        joined.insert(0, '…');
    }
    joined
}

/// 空串 → `None`（JSON 里不吐空字段）。
pub(super) fn non_empty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::tail;

    #[test]
    fn tail_keeps_the_last_lines_and_truncates_long_ones() {
        let text: String = (1..=40).map(|index| format!("line {index}\n")).collect();
        let tailed = tail(&text);
        assert!(tailed.starts_with("line 29"), "{tailed}");
        assert!(tailed.ends_with("line 40"));
        let long = "x".repeat(5000);
        let tailed = tail(&long);
        assert!(tailed.starts_with('…'));
        assert_eq!(tailed.chars().count(), 1201);
        assert_eq!(tail("   \n"), "");
    }
}
