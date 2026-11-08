//! 反馈家族自己的 gallery 段落开关（`ULL_FEEDBACK_PART`）。
//!
//! 契约 §9.1/§9.2：**`gallery/` 是 harness，不属于任何家族**——家族线不得为了自己出图去改
//! `gallery/mod.rs`；需要新能力时用「环境变量 + 默认向后兼容」，并报给 tower。
//! 本文件走的就是这条：开关本身住在**家族自己的目录**里，`gallery/feedback.rs` 只消费它。
//!
//! 为什么要再切一层：`ULL_GALLERY_SECTION=feedback` 只能把整个家族拎出来，而四件的高度
//! 超过 1180×760 首屏；`probe` 的几何断言需要「这一屏只画这一件」的干净取景。
//!
//! | 值 | 渲染 |
//! |---|---|
//! | 未设置 / 空 / `all` | 四块全渲染（prog → snack → empty → dialog），与不加开关一致 |
//! | `progress` / `snackbar` / `empty` / `dialog` | 只渲染那一块 |
//! | 其它 | 回落 `all` **并打一行 stderr**（静默回落会让人拿错图还以为拍的就是那一段） |

use std::sync::atomic::{AtomicBool, Ordering};

/// 家族内切分的环境变量名。
pub const PART_ENV: &str = "ULL_FEEDBACK_PART";

/// 四块的位（1 progress / 2 snackbar / 4 empty / 8 dialog）。
pub const PROGRESS: u8 = 1;
pub const SNACKBAR: u8 = 2;
pub const EMPTY: u8 = 4;
pub const DIALOG: u8 = 8;

const BLOCKS: [(u8, &str); 4] = [
    (PROGRESS, "progress"),
    (SNACKBAR, "snackbar"),
    (EMPTY, "empty"),
    (DIALOG, "dialog"),
];

/// 本次渲染哪几块。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Part(u8);

impl Part {
    /// 四块全渲染（默认）。
    pub const ALL: Self = Self(PROGRESS | SNACKBAR | EMPTY | DIALOG);

    /// 只有一块（名字不认识 → `None`，由 [`Part::from_env`] 回落并打日志）。
    pub fn only(name: &str) -> Option<Self> {
        BLOCKS
            .iter()
            .find(|(_, block)| *block == name)
            .map(|(bit, _)| Self(*bit))
    }

    /// 读 `ULL_FEEDBACK_PART`：空 / `all` → 全渲染；未识别的值 → 回落 + stderr 一行。
    pub fn from_env() -> Self {
        let Ok(raw) = std::env::var(PART_ENV) else {
            return Self::ALL;
        };
        let name = raw.trim().to_ascii_lowercase();
        if name.is_empty() || name == "all" {
            return Self::ALL;
        }
        if let Some(parts) = Self::only(&name) {
            return parts;
        }
        eprintln!(
            "[feedback] {PART_ENV}={raw:?} 不是已知部分（all|progress|snackbar|dialog|empty），已回落 all"
        );
        Self::ALL
    }

    /// 这一块要不要画。
    pub fn has(self, bit: u8) -> bool {
        self.0 & bit != 0
    }

    /// 本次渲染的块名（stderr 日志用）。
    pub fn names(self) -> Vec<&'static str> {
        BLOCKS
            .iter()
            .filter(|(bit, _)| self.has(*bit))
            .map(|(_, name)| *name)
            .collect()
    }

    /// 打印本次渲染了哪几块（首帧一次）。
    pub fn log_once(self) {
        if PART_LOGGED.swap(true, Ordering::Relaxed) {
            return;
        }
        let raw = std::env::var(PART_ENV).unwrap_or_else(|_| "all".to_owned());
        eprintln!(
            "[feedback] part={raw} → 渲染 {} 块：{}",
            self.names().len(),
            self.names().join(", ")
        );
    }
}

/// `[feedback] ...` 每进程只打一行（`view` 每帧都跑）。
static PART_LOGGED: AtomicBool = AtomicBool::new(false);
