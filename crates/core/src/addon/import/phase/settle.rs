//! 合并判定 + 决策 → 实际动作，以及冲突记录的组装。
//!
//! 从 [`super`] 拆出来的唯一原因是行数上限（400 行），行为逐字未变：这两个函数是**纯函数**，
//! 不碰盘、不看时钟，所以搬动它们不影响任何判据。

use super::super::merge::{self, MergeAction};
use super::super::report::{ConflictChoice, ConflictFile, ConflictReason, FileAction};

/// 合并判定 + 决策 → 实际动作。
///
/// `picked` 是这一次的处置（逐项决策或整批档，见 [`crate::addon::import::decision::ConflictDecisions`]）：
/// 它**只对冲突生效**——`Install` / `SkipIdentical` 不受它影响，所以"用户点错一条保留"不会漏装。
pub(super) fn settle(
    decision: merge::MergeDecision,
    disk_exists: bool,
    picked: ConflictChoice,
) -> (FileAction, ConflictChoice) {
    let overwrite = picked == ConflictChoice::Overwrite;
    match decision.action {
        MergeAction::Install => (
            if disk_exists {
                FileAction::Replaced
            } else {
                FileAction::Installed
            },
            ConflictChoice::Overwrite,
        ),
        MergeAction::SkipIdentical => (FileAction::SkippedIdentical, ConflictChoice::KeepUser),
        MergeAction::KeepUser if overwrite => (
            if disk_exists {
                FileAction::Replaced
            } else {
                FileAction::Installed
            },
            ConflictChoice::Overwrite,
        ),
        MergeAction::KeepUser => (FileAction::KeptUser, ConflictChoice::KeepUser),
    }
}

/// 组装一条冲突记录。
pub(super) fn conflict(
    path: String,
    reason: ConflictReason,
    choice: ConflictChoice,
    pack_sha1: &str,
    pack_size: u64,
    base_sha1: Option<String>,
    disk_sha1: Option<String>,
) -> ConflictFile {
    ConflictFile {
        path,
        reason,
        choice,
        message_key: reason.message_key(),
        pack_sha1: pack_sha1.to_string(),
        pack_size,
        base_sha1,
        disk_sha1,
    }
}
