//! 后台导入（M113）：探测 → 目标目录 → core 的 `import_pack` → 报告。
//!
//! 三条纪律：
//!
//! 1. **探测失败不空跑管线**：先做具名诊断（[`super::diagnose`]），失败就只回 `Probe` 事件；
//! 2. **默认档永远是 `KeepUser`**：只有 `request.overwrite == true` 才换成 `Overwrite`；
//! 3. **取消后不再打扰界面**：进度回调与最终报告都先看取消标记。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use unlucky_core::addon::import::{
    import_pack, ConflictPolicy, DownloadEngineFetcher, ImportProgress, ImportProgressSink,
    ImportRequest,
};
use unlucky_core::download::blocking::block_on;

use crate::dto::{ImportRequestView, PackEvent};
use unlucky_core::addon::import::ImportStatus;

use super::super::super::env::HostEnv;
use super::super::super::job::{spawn, JobSender};
use super::convert;
use super::send;

/// 起一个后台导入线程。
pub(super) fn spawn_import(
    env: HostEnv,
    tx: JobSender,
    request: ImportRequestView,
    epoch: u64,
    cancel: Arc<AtomicBool>,
    inflight: super::InFlight,
) {
    spawn("pack-import", move || {
        // 句柄跟着这次运行走到**出口**：正常收尾、两条早退、panic 展开都会 drop ⇒ 计数减一。
        // 以前是手工 `finished()` 放在 `block_on()` 之后，两条早退路径直接漏掉（复审 r2 P2-1）。
        let _inflight = inflight;
        run(&env, &tx, &request, epoch, cancel);
    });
}

pub(super) fn run(
    env: &HostEnv,
    tx: &JobSender,
    request: &ImportRequestView,
    epoch: u64,
    cancel: Arc<AtomicBool>,
) {
    // 探测先过一遍：失败就只给具名诊断（「看起来像 X，但缺 Y」），不空跑一遍管线
    if let Some(probe) = super::diagnose::of(&request.pack) {
        send(tx, epoch, PackEvent::Probe(Box::new(probe)));
        return;
    }
    let (target, register) = match target_of(env, request) {
        Ok(target) => target,
        Err(reason) => {
            send(
                tx,
                epoch,
                PackEvent::Report(Box::new(convert::failed_report(
                    // 目标目录不可用 ⇒ 如实给空路径（不把包路径冒充成实例目录）
                    PathBuf::new(),
                    &request.pack,
                    "import_host",
                    "Target instance is not usable",
                    &reason,
                ))),
            );
            return;
        }
    };

    let pack_name = request
        .instance_name
        .clone()
        .unwrap_or_else(|| convert::pack_stem(&request.pack));
    let progress_tx = tx.clone();
    let progress_name = pack_name.clone();
    let cancel_flag = Arc::clone(&cancel);
    let sink: ImportProgressSink = Arc::new(move |progress: &ImportProgress| {
        if cancel_flag.load(Ordering::SeqCst) {
            return;
        }
        send(
            &progress_tx,
            epoch,
            PackEvent::Progress(Some(Box::new(convert::progress_view(
                progress,
                &progress_name,
            )))),
        );
    });

    let engine = super::super::super::engine::engine(env);
    let fetcher = DownloadEngineFetcher::new(&engine);
    let mut core_request = ImportRequest::new(&request.pack, &target)
        .with_fetcher(&fetcher)
        .with_progress(sink);
    if request.overwrite {
        core_request = core_request.with_conflict_policy(ConflictPolicy::Overwrite);
    }
    if register {
        core_request = core_request.with_registration(&env.layout, Some(&pack_name));
    }

    let report = block_on(import_pack(&core_request));
    if report.status == ImportStatus::Failed && register {
        // 失败已回滚，但**目录是我们建的**、core 只清文件不清目录 ⇒ 不清掉的话
        // 下一次重试会撞「directory already exists」（复审 r1 P2-9）。
        remove_if_empty(&target);
    }
    if cancel.load(Ordering::SeqCst) {
        // 取消之后不再打扰界面（面板已经收回，代次也已经作废）。
        // 注意：后台这一步**已经跑完**（core 没有取消令牌），实例可能已被登记 ——
        // 所以取消的提示语必须说「还在后台跑」，不能说「实例未创建」。
        return;
    }
    send(
        tx,
        epoch,
        PackEvent::Report(Box::new(convert::report_view(&report, &target))),
    );
}

/// 目录里没有东西就删掉（失败回滚后的收尾；非空一律不动）。
fn remove_if_empty(dir: &std::path::Path) {
    let empty = std::fs::read_dir(dir)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false);
    if empty {
        let _ = std::fs::remove_dir(dir);
    }
}

/// 导入目标：`(目录, 是否登记新实例)`。
///
/// - 选了实例 ⇒ 它的运行目录，**不登记**（已经是一条实例记录）；
/// - 没选实例 ⇒ `<data>/instances/<dir_name>` 新目录 + 登记（实例名 = 用户给的名字或包名）。
///   注意 M111 的既有边界：导入**不装加载器**，所以新实例报告的 `版本 id` 还要另外装。
pub(super) fn target_of(
    env: &HostEnv,
    request: &ImportRequestView,
) -> Result<(PathBuf, bool), String> {
    match &request.instance {
        Some(id) => super::instance_root(env, id).map(|root| (root, false)),
        None => {
            let name = request
                .instance_name
                .clone()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| convert::pack_stem(&request.pack));
            let target = env
                .layout
                .data()
                .join("instances")
                .join(convert::dir_name(&name));
            if target.exists() {
                // 空目录（多半是上一次失败的残留）**可以复用**，否则重试永远报「已存在」
                let mut entries = std::fs::read_dir(&target)
                    .map_err(|error| format!("read instance dir failed: {error}"))?;
                if entries.next().is_some() {
                    return Err(format!(
                        "directory already exists and is not empty: {}",
                        target.display()
                    ));
                }
            } else {
                std::fs::create_dir_all(&target)
                    .map_err(|error| format!("create dir failed: {error}"))?;
            }
            Ok((target, true))
        }
    }
}
