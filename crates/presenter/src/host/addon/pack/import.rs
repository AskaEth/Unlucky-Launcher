//! 后台导入（M113/M160）：探测 → **预检（冲突清单）** → 目标目录 → core 的 `import_pack` → 报告。
//!
//! 四条纪律：
//!
//! 1. **探测失败不空跑管线**：先做具名诊断（[`super::diagnose`]），失败就只回 `Probe` 事件；
//! 2. **先预检再导入**（M160）：预检只读，把"会盖掉用户哪几个文件"算清楚。没有冲突就直接
//!    往下导（不打扰用户）；有冲突就回 [`PackEvent::Scan`] 让页面画清单，等用户点完再导；
//! 3. **缺省档永远 `KeepAll`**：决策从 DTO 逐字段搬（[`convert::decisions_of`]），页面没点过的
//!    路径在 core 侧落到"保留"；
//! 4. **取消后不再打扰界面**：进度回调与最终报告都先看取消标记。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use unlucky_core::addon::import::{
    import_pack, scan_pack_online, DownloadEngineFetcher, ImportProgress, ImportProgressSink,
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

/// 起一个后台**预检**线程（M160：只读，不写盘）。
pub(super) fn spawn_scan(
    env: HostEnv,
    tx: JobSender,
    request: ImportRequestView,
    epoch: u64,
    cancel: Arc<AtomicBool>,
) {
    spawn("pack-scan", move || {
        // 探测先过一遍：失败就只给具名诊断，不空跑一遍预检。
        if let Some(probe) = super::diagnose::of(&request.pack) {
            send(&tx, epoch, PackEvent::Probe(Box::new(probe)));
            return;
        }
        let target = match target_of(&env, &request) {
            Ok((target, _)) => target,
            Err(reason) => {
                // 目标实例不可用：与导入路径同一形状的失败报告（界面不用学第二套）。
                send(
                    &tx,
                    epoch,
                    PackEvent::Report(Box::new(convert::failed_report(
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
        // 预检本身只读（不写盘），但 M162 起会**先补全再判**：缺 `hashes.sha1` 的 mrpack 条目
        // 按 sha512 反查 Modrinth。补不出仍按 M160 的口径计数，所以冷启动没网也不影响判定。
        let engine = super::super::super::engine::engine(&env);
        let fetcher = DownloadEngineFetcher::new(&engine);
        let core_request = ImportRequest::new(&request.pack, &target).with_fetcher(&fetcher);
        let scan = block_on(scan_pack_online(&core_request));
        if cancel.load(Ordering::SeqCst) {
            return;
        }
        // 先发预检结果（`error` 也照原样带出去：调用方按"有没有冲突"分流要用它）。
        send(
            &tx,
            epoch,
            PackEvent::Scan(Box::new(convert::scan_view(&scan))),
        );
        // **预检自己没算出来 ⇒ 必须再发一条结果事件**（复审 r1 P2-1）。
        //
        // 只发 `Scan(error)` 的话，页面那边 `pick` 为空（它只由 `conflicts` 生成）、进度与报告
        // 又被 `Scan` 清空 ⇒ 落回入口态，用户点完「导入」屏幕上一个字都不变（静默空转）。
        // 走 `Report` 就回到 M160 之前那条既有形状：结果块的「致命 —— 已回滚」把原因画出来，
        // UI 一行不用改；`code`/`label`/`message`/`hint` 全部从 `scan.error` 搬。
        if let Some(error) = &scan.error {
            send(
                &tx,
                epoch,
                PackEvent::Report(Box::new(convert::failed_report_of(
                    target,
                    &request.pack,
                    error,
                ))),
            );
        }
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
    if register {
        if let Err(reason) = prepare_target(&target) {
            send(
                tx,
                epoch,
                PackEvent::Report(Box::new(convert::failed_report(
                    target,
                    &request.pack,
                    "import_host",
                    "Target instance is not usable",
                    &reason,
                ))),
            );
            return;
        }
    }

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
        .with_progress(sink)
        // 决策逐字段搬过来：`KeepAll`（缺省）/ `OverwriteAll` / 逐项，语义在 core 那一侧落地。
        .with_decisions(convert::decisions_of(&request.decisions));
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

/// 导入目标：`(目录, 是否登记新实例)` —— **只算路径，不建目录**（预检要能只读地用它）。
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
            }
            Ok((target, true))
        }
    }
}

/// 真要写盘之前建目录（**预检不调它**：预检是只读的，不建任何东西）。
pub(super) fn prepare_target(target: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(target).map_err(|error| format!("create dir failed: {error}"))
}
