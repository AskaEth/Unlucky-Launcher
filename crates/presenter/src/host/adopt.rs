//! 「导入已有版本」的宿主侧（M223 GUIADOPT）：扫描 / 预检 / 确认三条后台线程。
//!
//! 能力全在 core（`unlucky_core::game::adopt`，M221）：本模块只做三件事——**起线程、翻 DTO、
//! 按代次回流**。这里不重新判定任何东西：能不能登记、落到哪个运行目录、有哪些问题，全部由 core
//! 说了算，本模块逐字段照搬（[`scan_view`] / [`plan_view`] / [`done_view`] / [`failure_view`]）。
//!
//! ## 三个命令与写盘面
//!
//! | 命令 | core 调用 | 写盘 |
//! |---|---|---|
//! | [`AdoptCommand::Scan`] | `adopt::scan` | 零 |
//! | [`AdoptCommand::Preview`] | `adopt::plan` | 零（预检就是 core 的 dry-run） |
//! | [`AdoptCommand::Confirm`] | `adopt::adopt` | 实例设置 + `profiles.json`（+ 缺省运行目录） |
//!
//! 预检与确认走**同一个** `AdoptRequestView`：页面在预检拿到落点与后果原文，用户点确认时把
//! **同一份请求**发回来（不是「预检 A、写 B」）。这也是「运行目录只认回程」的实现方式——
//! 页面从头到尾没有机会自己派生一个目录名。
//!
//! ## 代次：为什么要它
//!
//! 三条命令各起一条线程，用户可能连点（扫完再扫、预检后改目标再预检、失败后关面板）。
//! 每条线程带一个代次回来，`adopt_epoch` 只认**最新**那一条：关面板（[`AdoptCommand::Dismiss`]）
//! 也会把代次 +1，于是「点了关闭，两秒后扫描结果又把面板点亮」这种旧病不会发生
//! （它与 M105 安装取消、M113 导入取消是同一套做法）。
//!
//! ## 不做什么
//!
//! - **不自己派生运行目录**：`run_dir` 只从 `plan`/`adopt` 的返回里取（缺省档可能带 `-2` 后缀）；
//! - **不自己拼后果话术**：`notes` 原样来自 core 的 `target_notes()`，与 CLI 打的是同一句；
//! - **不折叠问题**：`problems[]` 逐条搬进 [`AdoptFailureView`]，一条都不省。

use unlucky_core::game::adopt::{
    self, AdoptCandidate, AdoptError, AdoptOutcome, AdoptPlan, AdoptProblem, AdoptRequest,
    AdoptTarget,
};
use unlucky_core::game::install::instance::Isolation;
use unlucky_core::launch::time::{format_rfc3339_millis, now};

use crate::dto::{
    AdoptCandidateView, AdoptCommand, AdoptDoneView, AdoptEvent, AdoptFailureView,
    AdoptIsolationView, AdoptPlanView, AdoptProblemView, AdoptRequestView, AdoptScanView,
    AdoptTargetView, CoreEvent,
};

use super::env::HostEnv;
use super::job::{self, HostTask, JobSender};
use super::CoreHost;

/// 后台那一步是「只看不写」还是「真的写」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    /// `adopt::plan`（预检，零写盘）
    Preview,
    /// `adopt::adopt`（真写）
    Confirm,
}

impl CoreHost {
    /// 作废在飞的 adopt 结果，返回新代次（每条命令都调它，见模块文档「代次」一节）。
    fn next_adopt_epoch(&mut self) -> u64 {
        self.adopt_epoch = self.adopt_epoch.wrapping_add(1);
        self.adopt_epoch
    }

    /// `AppCommand::Adopt` 的落点（M223）。
    ///
    /// 每条命令都先把代次 +1：这一条之后回来的旧结果一律被 [`Self::handle_adopt`] 丢掉。
    pub(super) fn adopt(&mut self, command: AdoptCommand) {
        match command {
            AdoptCommand::Scan => {
                let epoch = self.next_adopt_epoch();
                spawn_scan(self.env.clone(), self.tx.clone(), epoch);
            }
            AdoptCommand::Preview(request) => {
                let epoch = self.next_adopt_epoch();
                spawn_adopt(
                    self.env.clone(),
                    self.tx.clone(),
                    *request,
                    Mode::Preview,
                    epoch,
                );
            }
            AdoptCommand::Confirm(request) => {
                let epoch = self.next_adopt_epoch();
                spawn_adopt(
                    self.env.clone(),
                    self.tx.clone(),
                    *request,
                    Mode::Confirm,
                    epoch,
                );
            }
            AdoptCommand::Dismiss => {
                // 关面板 = 作废在飞的一切（否则迟到的扫描结果会把面板重新点亮）
                self.next_adopt_epoch();
                self.send(CoreEvent::Adopt(Box::new(AdoptEvent::Cleared)));
            }
        }
    }

    /// 后台线程的回流落点：代次不符就丢（见模块文档「代次」一节）。
    ///
    /// 登记成功之后补一次实例快照刷新：新实例必须**这一拍就出现在实例列表里**，
    /// 而不是等用户手点刷新（否则「导入了但看不见」）。
    pub(super) fn handle_adopt(&mut self, event: AdoptEvent, epoch: u64) {
        if epoch != self.adopt_epoch {
            return;
        }
        let registered = matches!(event, AdoptEvent::Done(_));
        self.send(CoreEvent::Adopt(Box::new(event)));
        if registered {
            self.refresh_instances();
        }
    }
}

/// 扫描当前游戏根的 `versions/`（只读）。
pub(super) fn spawn_scan(env: HostEnv, tx: JobSender, epoch: u64) {
    job::spawn("adopt-scan", move || {
        let event = match adopt::scan(&env.game, &env.layout) {
            Ok(scan) => AdoptEvent::Scan(Box::new(AdoptScanView {
                versions_dir: scan.versions_dir.clone(),
                candidates: scan.candidates.iter().map(candidate_view).collect(),
            })),
            // 读目录失败也是**具名**失败（断链的 versions 联接 / 权限），不降级成空列表
            Err(error) => AdoptEvent::Failed(Box::new(failure_view(error))),
        };
        let _ = tx.send(HostTask::Adopt {
            event: Box::new(event),
            epoch,
        });
    });
}

/// 预检（零写盘）或确认登记。
pub(super) fn spawn_adopt(
    env: HostEnv,
    tx: JobSender,
    request: AdoptRequestView,
    mode: Mode,
    epoch: u64,
) {
    job::spawn("adopt", move || {
        let request = core_request(request);
        let event = match mode {
            Mode::Preview => match adopt::plan(&env.game, &env.layout, &request) {
                Ok(plan) => AdoptEvent::Planned(Box::new(plan_view(plan))),
                Err(error) => AdoptEvent::Failed(Box::new(failure_view(error))),
            },
            Mode::Confirm => match adopt::adopt(&env.game, &env.layout, &request) {
                Ok(outcome) => AdoptEvent::Done(Box::new(done_view(outcome))),
                Err(error) => AdoptEvent::Failed(Box::new(failure_view(error))),
            },
        };
        let _ = tx.send(HostTask::Adopt {
            event: Box::new(event),
            epoch,
        });
    });
}

/// 视图请求 → core 请求（`created_at` 由宿主给：core 不碰时钟）。
fn core_request(view: AdoptRequestView) -> AdoptRequest {
    AdoptRequest {
        version_id: view.version_id,
        name: view.name.filter(|name| !name.trim().is_empty()),
        target: target_of(view.target),
        created_at: Some(format_rfc3339_millis(now())),
    }
}

/// 三档视图 → core 的运行目录选择。
///
/// `custom` **不映射**：GUI 不开「用户手输绝对路径」这一档（那要目录选择器），
/// 需要它的调用方走 CLI 的 `ull instances adopt --isolation custom --game-dir DIR`。
fn target_of(view: AdoptTargetView) -> AdoptTarget {
    match view {
        AdoptTargetView::InstanceDir => AdoptTarget::InstanceDir,
        AdoptTargetView::Shared => AdoptTarget::Shared,
        AdoptTargetView::VersionDir => AdoptTarget::VersionDir,
    }
}

/// core 隔离档 → 视图。
fn isolation_view(isolation: Isolation) -> AdoptIsolationView {
    match isolation {
        Isolation::Isolated => AdoptIsolationView::Isolated,
        Isolation::Shared => AdoptIsolationView::Shared,
        Isolation::Custom => AdoptIsolationView::Custom,
    }
}

/// core 候选 → 视图（`registered` 只带实例名：界面要的是「给谁登记过了」）。
fn candidate_view(candidate: &AdoptCandidate) -> AdoptCandidateView {
    AdoptCandidateView {
        version_id: candidate.version_id.clone(),
        jar_present: candidate.jar_present,
        inherits: candidate.inherits.clone(),
        registered: candidate
            .registered
            .iter()
            .map(|instance| instance.name.clone())
            .collect(),
        problems: candidate.problems.iter().map(problem_view).collect(),
    }
}

/// core 问题 → 视图（`code` 稳定 + `Display` 原文：路径、行列号、缺哪个父版本都在里面）。
fn problem_view(problem: &AdoptProblem) -> AdoptProblemView {
    AdoptProblemView {
        code: problem.code().to_string(),
        message: problem.to_string(),
    }
}

/// core 计划 → 视图。`run_dir` / `notes` **原样**搬运，界面不许再算一遍。
fn plan_view(plan: AdoptPlan) -> AdoptPlanView {
    AdoptPlanView {
        version_id: plan.candidate.version_id.clone(),
        name: plan.name,
        target: target_view(&plan.target),
        isolation: isolation_view(plan.isolation),
        run_dir: plan.run_dir,
        notes: plan.notes,
    }
}

/// core 结果 → 视图。
fn done_view(outcome: AdoptOutcome) -> AdoptDoneView {
    AdoptDoneView {
        instance_id: outcome.instance_id,
        name: outcome.name,
        version_id: outcome.version_id,
        isolation: isolation_view(outcome.isolation),
        run_dir: outcome.run_dir,
        dir_created: outcome.dir_created,
        notes: outcome.notes,
    }
}

/// core 运行目录选择 → 视图（`Custom` 只在 core 侧出现，界面按「缺省档」显示）。
fn target_view(target: &AdoptTarget) -> AdoptTargetView {
    match target {
        AdoptTarget::InstanceDir | AdoptTarget::Custom(_) => AdoptTargetView::InstanceDir,
        AdoptTarget::Shared => AdoptTargetView::Shared,
        AdoptTarget::VersionDir => AdoptTargetView::VersionDir,
    }
}

/// core 失败 → 视图：**`problems[]` 一条不少地带出来**（契约 §5.3）。
fn failure_view(error: AdoptError) -> AdoptFailureView {
    AdoptFailureView {
        code: error.code().to_string(),
        message: error.to_string(),
        problems: error.problems().iter().map(problem_view).collect(),
    }
}

#[cfg(test)]
mod tests;
