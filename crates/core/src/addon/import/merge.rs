//! overrides 三方合并的**判据**（纯函数，无 IO）：包内原值 / 磁盘现状 / 基线哈希。
//!
//! # 一张表看完全部情形
//!
//! 记 `T` = 包内声明（theirs）、`B` = 基线（base，上次导入的包声明）、`D` = 磁盘现状（ours）。
//! `D` 为 `None` 表示文件不存在。
//!
//! | `D` | `B` | 关系 | 结论 | 理由 |
//! |-----|-----|------|------|------|
//! | 无 | 无 | — | [`MergeAction::Install`] | 全新文件，没有可保护的东西 |
//! | 无 | 有 | `B` 任意 | [`MergeAction::KeepUser`] + `UserDeleted` | 用户删过：**不许**偷偷装回来 |
//! | 有 | 无 | `D == T` | [`MergeAction::SkipIdentical`] | 内容一样，写盘是浪费 |
//! | 有 | 无 | `D != T` | [`MergeAction::KeepUser`] + `PreExisting` | 没有基线可判 ⇒ 当用户已有内容 |
//! | 有 | 有 | `D == T` | [`MergeAction::SkipIdentical`] | 已经是最新 |
//! | 有 | 有 | `D == B` | [`MergeAction::Install`] | 用户没动过 ⇒ 整合包升级，覆盖是对的 |
//! | 有 | 有 | `D != B` | [`MergeAction::KeepUser`] + `UserModified` | 用户改过：**不许**静默覆盖 |
//!
//! 两条"不许"就是任务里的硬要求（"不得直接盖掉用户改过的文件"）。判据只此一处：决策
//! （逐项 [`super::decision::ConflictDecisions`]，或整批 [`super::ConflictPolicy`]）为
//! `Overwrite` 时，`KeepUser` 才会被判据的调用方翻成 [`MergeAction::Install`]，**判据本身
//! 不读决策**——这样"哪些情形算冲突"与"冲突怎么处置"是两个可独立验证的事实。
//!
//! # `.disabled` / `.old` 不参与判据
//!
//! 后缀是路径的一部分（`mods/x.jar.disabled` 与 `mods/x.jar` 是**两个文件**），所以它们
//! 天然按不同键各自走上面的表。不给 `.old` 附加"清理/回滚"语义（tower 定案：只识别与保留）。

use super::report::ConflictReason;

/// 判据给出的动作（策略开关在调用方）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeAction {
    /// 写入包内内容（新建或覆盖）。
    Install,
    /// 磁盘内容已经与包内一致，什么都不做。
    SkipIdentical,
    /// 保留用户那一侧（不写盘）。
    KeepUser,
}

/// 一次判定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MergeDecision {
    /// 动作。
    pub action: MergeAction,
    /// 算冲突时的成因（`None` = 不是冲突）。
    pub reason: Option<ConflictReason>,
}

impl MergeDecision {
    const fn plain(action: MergeAction) -> Self {
        Self {
            action,
            reason: None,
        }
    }

    const fn conflict(reason: ConflictReason) -> Self {
        Self {
            action: MergeAction::KeepUser,
            reason: Some(reason),
        }
    }

    /// 是否冲突（与最终处置无关）。
    #[must_use]
    pub const fn is_conflict(self) -> bool {
        self.reason.is_some()
    }
}

/// 三方合并判据。
#[must_use]
pub fn decide(pack_sha1: &str, base_sha1: Option<&str>, disk_sha1: Option<&str>) -> MergeDecision {
    match (disk_sha1, base_sha1) {
        // 磁盘上没有这个文件。
        (None, None) => MergeDecision::plain(MergeAction::Install),
        (None, Some(_)) => MergeDecision::conflict(ConflictReason::UserDeleted),
        // 磁盘上有、内容与包内**逐字节相同**（同一份 SHA-1）。
        //
        // 这是"内容相同"的**唯一判据**：预检（`super::scan::judge`）刻意不再自己写一遍
        // `disk == pack` 的早退（复审 r1 P3-2），而是走这里的 `reason == None`。改这一行
        // 会同时影响"导入怎么处置"与"预检列不列这条"，所以两边的测试都会红。
        (Some(disk), _) if disk == pack_sha1 => MergeDecision::plain(MergeAction::SkipIdentical),
        (Some(_), None) => MergeDecision::conflict(ConflictReason::PreExisting),
        (Some(disk), Some(base)) if disk == base => MergeDecision::plain(MergeAction::Install),
        (Some(_), Some(_)) => MergeDecision::conflict(ConflictReason::UserModified),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: &str = "aaaa";
    const B: &str = "bbbb";

    #[test]
    fn fresh_file_is_installed() {
        let decision = decide(T, None, None);
        assert_eq!(decision.action, MergeAction::Install);
        assert!(!decision.is_conflict());
    }

    #[test]
    fn untouched_file_is_replaced_on_upgrade() {
        // 磁盘 == 基线（用户没动过）⇒ 整合包升级，覆盖。
        let decision = decide(T, Some(B), Some(B));
        assert_eq!(decision.action, MergeAction::Install);
        assert!(!decision.is_conflict());
    }

    #[test]
    fn user_modification_is_a_conflict() {
        let decision = decide(T, Some(B), Some("cccc"));
        assert_eq!(decision.action, MergeAction::KeepUser);
        assert_eq!(decision.reason, Some(ConflictReason::UserModified));
    }

    #[test]
    fn user_deletion_is_a_conflict() {
        let decision = decide(T, Some(B), None);
        assert_eq!(decision.action, MergeAction::KeepUser);
        assert_eq!(decision.reason, Some(ConflictReason::UserDeleted));
    }

    #[test]
    fn pre_existing_without_baseline_is_a_conflict() {
        let decision = decide(T, None, Some("dddd"));
        assert_eq!(decision.action, MergeAction::KeepUser);
        assert_eq!(decision.reason, Some(ConflictReason::PreExisting));
    }

    #[test]
    fn identical_content_is_skipped() {
        // 无论有没有基线，磁盘与包内一致就不写。
        assert_eq!(decide(T, None, Some(T)).action, MergeAction::SkipIdentical);
        assert_eq!(
            decide(T, Some(B), Some(T)).action,
            MergeAction::SkipIdentical
        );
    }
}
