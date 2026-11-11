//! presenter 的更新快照 → 页面视图模型（M159 的**唯一**展示转换点）。
//!
//! 与 `view/instances.rs` 同一条分工：判断（要不要提示、是不是失败）在 presenter / core 已经做完，
//! 这里只把字段摆成「要画什么」。三条映射必须与 DTO 的判据保持一致：
//!
//! | DTO | 页面视图 |
//! |---|---|
//! | `UpdateOutcome::UpToDate` | [`UpdateStatus::UpToDate`]（**不是**失败） |
//! | `UpdateOutcome::Failed` | [`UpdateStatus::Failed`]（带原因，**不许**画成「已是最新」） |
//! | `UpdateOutcome::MissingArtifact` | [`UpdateStatus::MissingArtifact`]（发布侧漏项，单独一句话） |

use unlucky_presenter::dto::{UpdateOutcome, UpdateView};

use crate::pages::{Update, UpdateStatus};

/// 快照 → 视图模型。
pub(super) fn update_of(view: &UpdateView) -> Update {
    Update {
        current_version: view.current_version.clone(),
        channel: view.channel.clone(),
        auto_check: view.auto_check,
        status: status_of(&view.outcome),
        checked_at: view.checked_at.clone(),
        applying: view.applying,
        ready: view.ready.as_ref().map(|ready| ready.version.clone()),
        skipped: view.skipped.clone(),
        silent: view.silent,
    }
}

/// 结论 → 页面状态（四种结论一一对应，**不折叠**）。
fn status_of(outcome: &UpdateOutcome) -> UpdateStatus {
    match outcome {
        UpdateOutcome::Idle => UpdateStatus::Idle,
        UpdateOutcome::Checking => UpdateStatus::Checking,
        UpdateOutcome::UpToDate { latest } => UpdateStatus::UpToDate {
            latest: latest.clone(),
        },
        UpdateOutcome::Available(candidate) => UpdateStatus::Available {
            version: candidate.version.clone(),
            size: candidate.size,
            notes: candidate.notes.clone(),
            forced: candidate.forced,
        },
        UpdateOutcome::MissingArtifact { version } => UpdateStatus::MissingArtifact {
            version: version.clone(),
        },
        UpdateOutcome::Failed(failure) => UpdateStatus::Failed {
            reason: failure.message.clone(),
            retryable: failure.retryable,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unlucky_presenter::dto::{ReadyUpdate, UpdateFailureView};

    fn view(outcome: UpdateOutcome) -> UpdateView {
        let mut view = UpdateView::new("0.2.2", "stable", true);
        view.outcome = outcome;
        view
    }

    #[test]
    fn failure_is_never_mapped_to_up_to_date() {
        let mapped = update_of(&view(UpdateOutcome::Failed(UpdateFailureView::new(
            "network",
            "取更新清单失败：连接被重置",
            true,
        ))));
        assert!(
            matches!(&mapped.status, UpdateStatus::Failed { reason, retryable } if *retryable && !reason.is_empty()),
            "失败必须原样带原因，实际 {:?}",
            mapped.status
        );
        assert!(mapped.banner().is_some(), "主动查出来的失败要弹提示条");

        // 静默检查的失败：快照里仍是失败，但不弹条（离线是常态）。
        let mut silent = view(UpdateOutcome::Failed(UpdateFailureView::new(
            "network",
            "连不上",
            true,
        )));
        silent.silent = true;
        let mapped = update_of(&silent);
        assert!(matches!(mapped.status, UpdateStatus::Failed { .. }));
        assert!(mapped.banner().is_none());
    }

    #[test]
    fn the_four_outcomes_stay_distinct() {
        let available = update_of(&view(UpdateOutcome::Available(Box::new(
            unlucky_presenter::dto::UpdateCandidateView {
                version: "0.2.3".to_string(),
                size: 21_000_000,
                notes: "修掉启动崩溃".to_string(),
                forced: false,
            },
        ))));
        assert!(available.available().is_some());
        assert!(matches!(
            available.banner(),
            Some(crate::pages::Banner::Available { .. })
        ));

        let up_to_date = update_of(&view(UpdateOutcome::UpToDate {
            latest: Some("0.2.2".to_string()),
        }));
        assert!(matches!(up_to_date.status, UpdateStatus::UpToDate { .. }));
        assert!(up_to_date.banner().is_none());

        let missing = update_of(&view(UpdateOutcome::MissingArtifact {
            version: "0.2.3".to_string(),
        }));
        assert!(matches!(
            missing.status,
            UpdateStatus::MissingArtifact { .. }
        ));
    }

    #[test]
    fn a_ready_update_turns_the_banner_into_the_restart_prompt() {
        let mut view = view(UpdateOutcome::UpToDate {
            latest: Some("0.2.3".to_string()),
        });
        view.ready = Some(ReadyUpdate {
            version: "0.2.3".to_string(),
            path: "C:/ULL/unlucky-launcher.exe".to_string(),
            backup: None,
        });
        let mapped = update_of(&view);
        assert_eq!(mapped.ready.as_deref(), Some("0.2.3"));
        assert!(matches!(
            mapped.banner(),
            Some(crate::pages::Banner::Ready { .. })
        ));
    }

    #[test]
    fn a_skipped_update_keeps_the_snapshot_but_hides_the_banner() {
        let mut view = view(UpdateOutcome::Available(Box::new(
            unlucky_presenter::dto::UpdateCandidateView {
                version: "0.2.3".to_string(),
                size: 1,
                notes: String::new(),
                forced: false,
            },
        )));
        view.skipped = Some("0.2.3".to_string());
        let mapped = update_of(&view);
        assert_eq!(mapped.skipped.as_deref(), Some("0.2.3"));
        assert!(mapped.available().is_some(), "候选要留着给设置页");
        assert!(mapped.banner().is_none());
    }
}
