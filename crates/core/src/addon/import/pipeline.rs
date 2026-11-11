//! 导入编排：**读取清单 → 解压覆盖内容 → 补全下载 → 落地 → 登记**，每步都可中断、可失败、可重试。
//!
//! # 两阶段落盘（任务硬要求）
//!
//! 先落 `overrides/`（离线、包内自带），再去补远端文件。顺序不是审美问题：
//!
//! - 覆盖内容是最可能被用户改过的那部分——先判定冲突，才能在"用户改过"时**既不覆盖**也不
//!   白下载同名文件（远端条目走同一张三方合并表，冲突就直接跳过下载）；
//! - 远端补全是唯一会失败于网络的一步，放最后意味着"网络不通"最多让结果变成 `Partial`，
//!   不会让已经解好的覆盖内容白干。
//!
//! # 失败分级（判据只在一处：是不是"某个文件搞不到"）
//!
//! | 失败 | 结果 |
//! |------|------|
//! | 开包/探测/读清单/组计划失败（穿越、符号链接、限额、清单坏） | `Failed`，临时目录删掉，目标目录零残留 |
//! | 某条远端文件摸不到（离线 / CF 无客户端 / 404） | `Partial`，逐条进 `missing`，实例照装 |
//! | 下载到的内容与声明摘要不符 | `Failed`（这是**损坏或被换过**，不是"暂时搞不到"） |
//! | 本地写盘失败 | `Failed` |
//! | 实例登记失败 | `Failed`（连内容一起回滚，不留"有目录没档案"的半个实例；回滚不完整时如实列出） |
//!
//! **回滚也要如实**：回滚本身可能失败（文件被占用、权限），这时 `rolled_back` 为 `false`
//! 并在 `rollback.failed` 里逐项列出——不许在"可能没回干净"时宣称一致。
//!
//! # 可重试
//!
//! 失败后目标目录与导入前一致（或明确告诉你哪几项没回去），重跑同一个包是安全的；成功后再跑
//! 一遍会靠基线里的哈希走"跳过 / 已一致"路径，不会重复写盘。
//!
//! # 取消（M195）：七个检查点
//!
//! 编排层负责**阶段边界**的检查点（入口 → 解析后 → 补全后 → 开暂存目录前 → 提交前 → 登记前），
//! 长循环里的**逐条**检查在各自实现里（解压/下载在 [`super::phase`]，合并落盘在
//! [`super::stage`]）。逐段表（含"哪几段没有检查点、为什么"）见 [`super::cancel`]。
//! 每次停下都按"回滚做过什么"收尾：没写盘 ⇒ 删暂存目录；写过 ⇒ 回滚结果如实进报告。
//! **取消不走 [`failed`]**：它走 [`ImportReport::cancel`]，结果类型是 `Cancelled`
//! 而不是 `Failed`。

use super::baseline::PackBaseline;
use super::cancel::Stopped;
use super::error::ImportError;
use super::phase;
use super::plan::ImportPlan;
use super::prepare::{account_plan, probe_and_plan, write_baseline};
use super::report::{keys, tell, ImportIssue, ImportReport, ImportStage, RegisteredInstance};
use super::stage::{CommitStop, RollbackOutcome, Staging};
use super::ImportRequest;

/// 跑一次导入。**永不返回 `Err`**：成功 / 部分成功 / 失败 / 取消全在报告里。
pub async fn run(request: &ImportRequest<'_>) -> ImportReport {
    let mut report = ImportReport::empty(request.target.to_path_buf());
    // 检查点①：入口（用户可能在上一次点开面板之后、线程起来之前就取消了）。
    if request.cancel.is_cancelled() {
        return cancelled(report, RollbackOutcome::default());
    }
    tell(&request.progress, ImportStage::Probe, 0, 0, None);
    let (archive, mut plan) = match probe_and_plan(request) {
        Ok(pair) => pair,
        Err(error) => return abandoned(report, error),
    };
    account_plan(&mut report, &plan);
    // 检查点②：解析后（包读完了，目标目录还一个字节都没写——停下最便宜）。
    if request.cancel.is_cancelled() {
        return cancelled(report, RollbackOutcome::default());
    }
    super::complete::complete_and_note(request, &mut plan, &mut report).await;
    // 检查点③：补全后（补全是逐条网络请求，条目之间已经在 `complete` 里停过一次）。
    if request.cancel.is_cancelled() {
        return cancelled(report, RollbackOutcome::default());
    }

    let baseline = {
        let (baseline, warnings) = load_baseline(request.target, &plan);
        report.warnings.extend(warnings);
        baseline
    };
    // 检查点④：开暂存目录前（此后磁盘上就有临时目录了）。
    if request.cancel.is_cancelled() {
        return cancelled(report, RollbackOutcome::default());
    }
    let staging = match Staging::begin(request.target) {
        Ok(staging) => staging,
        Err(error) => return abandoned(report, error),
    };

    let mut next = baseline.clone();
    let staged = async {
        phase::extract_overrides(
            request,
            &archive,
            &plan,
            &baseline,
            &staging,
            &mut report,
            &mut next,
        )
        .await?;
        phase::fetch_remote(request, &plan, &baseline, &staging, &mut report, &mut next).await?;
        write_baseline(&staging, &plan, &next)?;
        Ok::<(), Stopped>(())
    }
    .await;
    match staged {
        Ok(()) => {}
        // 停在暂存阶段：临时目录整个删掉，目标目录与导入前一致（一个字节都没写过）。
        Err(Stopped::Cancelled) => {
            staging.abort();
            return cancelled(report, RollbackOutcome::default());
        }
        Err(Stopped::Failed(error)) => {
            staging.abort();
            return abandoned(report, error);
        }
    }

    // 检查点⑤：提交前（与"清中间产物"同一条边界）。
    if request.cancel.is_cancelled() {
        staging.abort();
        return cancelled(report, RollbackOutcome::default());
    }

    // 引擎的续传残片（`.ullpart-*`）与 meta 不是整合包内容，提交前剔掉。
    match staging.purge_engine_artifacts() {
        Ok(0) => {}
        Ok(removed) => report.warnings.push(ImportIssue::new(
            keys::NOTE_ENGINE_ARTIFACTS_DROPPED,
            format!("提交前剔除了 {removed} 个下载引擎的中间产物（.ullpart-* / .ullmeta.json）"),
        )),
        Err(error) => {
            staging.abort();
            return abandoned(report, error);
        }
    }

    tell(&request.progress, ImportStage::Commit, 0, 0, None);
    // 检查点⑥：落盘（全新导入只在 rename 之前；合并态在 `apply_merge` 里逐文件）。
    let token = request.cancel.clone();
    let guard = match staging.commit(&token, &request.progress) {
        Ok(guard) => guard,
        Err(CommitStop::Cancelled(outcome)) => return cancelled(report, outcome),
        Err(CommitStop::Failed(failure)) => {
            return record_rollback(failed(report, failure.error), failure.rollback);
        }
    };

    if let Some(layout) = request.register {
        tell(&request.progress, ImportStage::Register, 0, 0, None);
        // 检查点⑦：登记前。内容已经落进目标目录，所以取消要**连内容一起回滚**，否则留下
        // "有目录没档案"的半个实例——这正是"取消后不得登记实例"的判据。
        if request.cancel.is_cancelled() {
            let outcome = guard.rollback();
            return cancelled(report, outcome);
        }
        let name = request
            .instance_name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(plan.manifest.name.as_str());
        match crate::game::install::register(layout, name, &plan.version_id, None) {
            Ok(registration) => {
                report.registered = Some(RegisteredInstance {
                    id: registration.profile.id.as_str().to_string(),
                    name: registration.profile.name.clone(),
                    version_id: registration.profile.version_id.clone(),
                    created: registration.created,
                });
                report.warnings.push(ImportIssue::new(
                    keys::NOTE_REGISTERED,
                    format!(
                        "已登记实例 {:?}（{}）",
                        registration.profile.name, plan.version_id
                    ),
                ));
            }
            Err(error) => {
                // 内容落盘了但档案没建起来 = 用户看不见的半个实例 ⇒ 连内容一起回滚。
                let outcome = guard.rollback();
                return record_rollback(
                    failed(report, ImportError::instance(error.to_string())),
                    outcome,
                );
            }
        }
    }
    guard.finish();

    report.finish_status();
    report
}

/// 取消收尾：把「用户取消」写进报告（**不是失败**），返回收好尾的报告。
///
/// 三档注记与 `rolled_back` 的判据在 [`ImportReport::cancel`]（那里也是失败路径的同形三档）。
fn cancelled(mut report: ImportReport, outcome: RollbackOutcome) -> ImportReport {
    report.cancel(outcome);
    report
}

/// 读三方合并的 base 侧：没有 / 读不出来 / **属于别的包** 三条路都不致命。
///
/// "属于别的包"要单独说一句：同一目录先后导入两个整合包时，若拿别人的基线当 base，用户改过
/// 的文件会被判成"整合包升级"而**被静默覆盖**。所以身份不符时降级成"没有基线"并告警。
///
/// 返回的告警由调用方写进报告（预检也是同一批文案——两侧对"有没有基线"必须有同一个答案）。
pub(super) fn load_baseline(
    target: &std::path::Path,
    plan: &ImportPlan,
) -> (PackBaseline, Vec<ImportIssue>) {
    match PackBaseline::load(target) {
        Ok(baseline) => {
            if baseline.matches_pack(&plan.format, &plan.manifest.name) {
                return (baseline, Vec::new());
            }
            let warning = ImportIssue::new(
                keys::NOTE_BASELINE_PACK_MISMATCH,
                format!(
                    "基线属于另一个整合包（{} / {:?}），本次按「没有基线」处理：磁盘上同名文件一律算用户已有内容",
                    if baseline.format.is_empty() {
                        "?"
                    } else {
                        &baseline.format
                    },
                    baseline.pack_name
                ),
            );
            (PackBaseline::default(), vec![warning])
        }
        Err(reason) => {
            let warning = ImportIssue::new(
                keys::NOTE_BASELINE_UNREADABLE,
                format!(
                    "基线读不出来，本次按「没有基线」处理（磁盘上同名文件一律算用户已有内容）：{reason}"
                ),
            );
            (PackBaseline::default(), vec![warning])
        }
    }
}

/// 把一次回滚的结果写进报告：**文案与机器位同源**，只有完整回滚才宣称"与导入前一致"。
fn record_rollback(mut report: ImportReport, outcome: RollbackOutcome) -> ImportReport {
    if outcome.is_complete() {
        report.rolled_back = true;
        report.warnings.push(ImportIssue::new(
            keys::ROLLBACK_DONE,
            format!(
                "已回滚 {} 项：目标目录恢复为导入前的样子（{}）",
                outcome.attempted,
                report.target.display()
            ),
        ));
    } else {
        report.rolled_back = false;
        report.warnings.push(ImportIssue::new(
            keys::NOTE_ROLLBACK_INCOMPLETE,
            format!(
                "有 {} 项未能回滚（目标目录可能与导入前不同）：{}",
                outcome.failure_count(),
                outcome.failed.join("；")
            ),
        ));
    }
    report.rollback = Some(outcome);
    report
}

/// 失败收尾：**只落错误，不宣称目标目录状态**。
///
/// 口径由调用方补：从未写过目标目录 ⇒ [`abandoned`]；已经写过 ⇒ [`record_rollback`]。
/// 早先这里无条件推"目标目录保持导入前的样子"，而随后的 `record_rollback` 可能再推
/// "有 N 项未能回滚"——同一份 warnings 自相矛盾（复审 r6 抓到）。
fn failed(mut report: ImportReport, error: ImportError) -> ImportReport {
    report.rolled_back = false;
    report.fail(error);
    report
}

/// 失败发生在**碰目标目录之前**（暂存目录已清）：这时才可以宣称"目标目录未被写入"。
fn abandoned(report: ImportReport, error: ImportError) -> ImportReport {
    let mut report = failed(report, error);
    report.rolled_back = true;
    report.warnings.push(ImportIssue::new(
        keys::ROLLBACK_DONE,
        format!(
            "导入失败，已清理临时目录；目标目录未被写入（{}）",
            report.target.display()
        ),
    ));
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addon::import::ImportStatus;

    fn base() -> ImportReport {
        ImportReport::empty(std::path::PathBuf::from("instance"))
    }

    fn messages(report: &ImportReport) -> Vec<&str> {
        report
            .warnings
            .iter()
            .map(|note| note.message.as_str())
            .collect()
    }

    fn incomplete() -> RollbackOutcome {
        RollbackOutcome {
            attempted: 3,
            failed: vec!["删除 config/a.toml 失败：拒绝访问".to_string()],
        }
    }

    #[test]
    fn incomplete_rollback_never_claims_the_target_is_untouched() {
        // 复审 r6 第 1 条：机器位（rolled_back=false）诚实，文案位也必须诚实——同一份 warnings
        // 里不许同时出现"保持导入前的样子"与"有 N 项未能回滚"。
        let report = record_rollback(failed(base(), ImportError::fetch("提交失败")), incomplete());

        assert_eq!(report.status, ImportStatus::Failed);
        assert!(!report.rolled_back, "回滚不完整时不许宣称一致");
        assert_eq!(
            report.rollback.as_ref().map(RollbackOutcome::failure_count),
            Some(1)
        );
        let messages = messages(&report);
        assert!(
            messages
                .iter()
                .any(|line| line.contains("未能回滚") && line.contains("config/a.toml")),
            "失败项要逐条列出：{messages:?}"
        );
        assert!(
            !messages.iter().any(|line| line.contains("保持导入前的样子")
                || line.contains("未被写入")
                || line.contains("恢复为导入前的样子")),
            "回滚不完整却宣称一致（自相矛盾）：{messages:?}"
        );
    }

    #[test]
    fn complete_rollback_claims_consistency_exactly_once() {
        let report = record_rollback(
            failed(base(), ImportError::fetch("提交失败")),
            RollbackOutcome {
                attempted: 2,
                failed: Vec::new(),
            },
        );

        assert!(report.rolled_back);
        assert_eq!(
            report.rollback.as_ref().map(RollbackOutcome::failure_count),
            Some(0)
        );
        let messages = messages(&report);
        assert!(
            messages
                .iter()
                .any(|line| line.contains("恢复为导入前的样子")),
            "{messages:?}"
        );
        assert!(
            !messages.iter().any(|line| line.contains("未能回滚")),
            "{messages:?}"
        );
    }

    #[test]
    fn pre_commit_failure_claims_untouched_only_via_abandoned() {
        // `failed()` 本身不宣称任何目标目录状态；"从未写过目标目录"这条口径只由 `abandoned()` 给。
        let bare = failed(base(), ImportError::fetch("开包失败"));
        assert!(!bare.rolled_back);
        assert!(bare.rollback.is_none());
        assert!(!messages(&bare)
            .iter()
            .any(|line| line.contains("保持导入前的样子") || line.contains("未被写入")));

        let report = abandoned(base(), ImportError::fetch("开包失败"));
        assert!(report.rolled_back);
        assert!(report.rollback.is_none());
        let messages = messages(&report);
        assert!(
            messages.iter().any(|line| line.contains("未被写入")),
            "{messages:?}"
        );
    }
}
