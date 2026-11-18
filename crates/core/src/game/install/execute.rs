//! 落盘执行：跳过判定 → 并发下载（引擎负责镜像/退避/续传/校验）→ 原子写版本 JSON。
//!
//! 依据：`DevDocs/analyse/03-version-download.md` §6.9（完整性修复路径的"存在即信"分级）、
//! `DevDocs/PlanDoc/05-download-and-mirrors.md` §2（按资源类型分层）、§5（静态展开并行度）。
//!
//! # 跳过判定（幂等的地基）
//!
//! 判据分三级，与 analyse/03 §6.9 的思路一致：
//! - 有 sha1 → **重算摘要比对**（唯一可信判据；代价是读一遍盘）；
//! - 无 sha1 有大小 → 只比大小（清单没给摘要时退一步）；
//! - 两者都没有 → 存在即信（HMCL 同类做法，留给后续"一键修复"补校验）。
//!
//! 跳过不写盘、不进网络，因此"重复 install 同版本"第二次几乎零成本。
//!
//! # 失败策略
//!
//! **不整体回滚**：已下好的共享层文件（libraries/assets）留在原地复用，失败信息逐文件结构化
//! 返回，由调用方决定是重试、还是只提示"缺哪几个文件"。这与 analyse/03 §6.3 的"宁可给出
//! 可修复的骨架"一致，但**版本清单与实例登记只在全量成功后才写**，不会出现半实例。
//!
//! # 取消（M105：GUI 的「取消安装」要**真停**）
//!
//! [`execute`] 与 CLI 的契约**一字不变**（不传取消标志 = 今天的行为）；
//! 取消走 [`execute_with_cancel`]，判据三条：
//!
//! 1. **不再起新文件**：流里**每个 item 的第一句**就看标志，置位直接返回 `None` ⇒
//!    那些 item **连 `install_one` 都不进**（不发请求、不算整文件 sha1、不回调 `sink`）。
//!    这一句不能挪到 `select` 的右支：`select` 先轮询左支，左支第一次 poll 就把
//!    `engine.download(…)` 派出去了（复审 P2-1 就是从这条抓出来的）。
//! 2. **在飞的文件就地掐断**：`install_one` 与「取消 future」`select` 赛跑，
//!    取消赢就 drop 掉下载 future —— reqwest 的响应流随之关闭（socket 断开），
//!    `.ullpart` 由 [`PartFile`](crate::download::PartFile) 的 `Drop` 清掉（不留半截垃圾）。
//! 3. **半成品不许变成品**：取消后不再写版本 JSON，返回
//!    [`InstallErrorKind::Cancelled`](super::InstallErrorKind)，调用方（presenter）据此把阶段
//!    记成「已取消」而不是「失败」。
//!
//! 于是取消延迟 = **一个数据块的唤醒**（不是「等一个文件下完」，也不是「等这一段跑完」）。
//!
//! ## 固有边界（复审要求留档，别把它当 bug 修）
//!
//! `buffer_unordered(concurrency)` 在取消那一刻**已经有最多 `concurrency`（GUI 传 16）个 future
//! 被拉起来了**，它们已经过了第 1 条那道门。所以最坏情况下这 16 个文件里：
//!
//! - **盘上已就绪的**（第一 poll 就 `Ready`）会照常「跑完」并被回调一次 `sink` ——
//!   presenter 侧靠**安装代次**（`CoreHost::install_epoch`）把这些回调带来的进度事件丢掉，
//!   所以面板不会因此复燃；
//! - **真在下载的**会在同一次 poll 的右支被取消掉（future drop、连接关掉、`.ullpart` 清掉）。
//!
//! 也就是说：**「取消之后一个字节都不会再写」这句话只对「还没被拉起的 item」成立**；
//! 在飞的 16 个里已就绪的那些仍会走完自己的回调。要连这一步都收掉，得让
//! `buffer_unordered` 支持「取消即清空队列」，本轮不做（不在 M105 的验收口径里）。

use std::future::poll_fn;
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::Poll;

use futures_util::future::{select, Either};
use futures_util::stream::{self, StreamExt};

use super::plan::PlanFile;
use super::report::ManifestWrite;
use super::{InstallError, InstallErrorKind, InstallPlan};
use crate::download::verify;
use crate::download::{DownloadEngine, DownloadError, DownloadErrorKind, HashAlgo};
use crate::platform::atomic::write_atomic;

pub use super::report::{FileFailure, FileOutcome, FileStatus, InstallReport, ProgressSink};

/// 执行安装计划。
///
/// 具体步骤：
/// 1. 规划期已取到的元数据（资源索引，见 [`InstallPlan::prefetch`]）先原子落盘，零网络；
/// 2. 其余文件并发下载（`concurrency` 上限，引擎内部另有信号量），每个文件带进度回调；
/// 3. **全量就绪**才写 `versions/<id>/<id>.json`：这份清单是"已安装"的判据
///    （[`GameLayout::is_version_installed`]），半成品写进去会把缺件的版本报成已安装。
///    共享层里已下好的文件留着复用，重跑 `ull install` 会补齐剩下的。
///
/// # Errors
///
/// 版本 JSON 落盘失败时返回 IO 错误；文件下载失败**不**返回错误，而是进
/// [`InstallReport`]（调用方按 [`InstallReport::is_complete`] 决策）。
pub async fn execute(
    engine: &DownloadEngine,
    plan: &InstallPlan,
    concurrency: usize,
    sink: Option<ProgressSink>,
) -> Result<InstallReport, super::InstallError> {
    execute_with_cancel(engine, plan, concurrency, sink, None).await
}

/// 同 [`execute`]，但可以带一个**取消标志**（M105）。
///
/// `cancel = None` 与 [`execute`] 逐字等价（CLI 走这条）；`Some(flag)` 时：
/// 置位就不再起新文件、并就地掐断在飞的文件，最后返回
/// [`InstallErrorKind::Cancelled`]（**不写版本 JSON**）。
///
/// # Errors
///
/// 同 [`execute`]；另外取消时返回 `Cancelled` 大类。
pub async fn execute_with_cancel(
    engine: &DownloadEngine,
    plan: &InstallPlan,
    concurrency: usize,
    sink: Option<ProgressSink>,
    cancel: Option<&AtomicBool>,
) -> Result<InstallReport, super::InstallError> {
    if is_cancelled(cancel) {
        return Err(cancelled_error());
    }
    let mut outcomes: Vec<FileOutcome> = Vec::with_capacity(plan.files.len());
    let mut prefetched: Vec<&PlanFile> = Vec::new();

    for payload in &plan.prefetched {
        let Some(file) = plan.files.iter().find(|file| file.dest == payload.dest) else {
            continue;
        };
        // 盘上已是同一份内容就不再重写（规划期为了枚举对象必须联网取索引，
        // 但那不等于"需要重新落盘"——重装的"下载 0 个"要名副其实）。
        if let Some(bytes) = current_bytes(file) {
            let outcome = FileOutcome {
                role: file.role,
                name: file.name.clone(),
                dest: file.dest.clone(),
                status: FileStatus::Skipped,
                bytes,
                source: None,
                failure: None,
            };
            if let Some(sink) = &sink {
                sink(&outcome);
            }
            outcomes.push(outcome);
            prefetched.push(file);
            continue;
        }
        let written = write_atomic(&payload.dest, &payload.bytes).map_err(|error| {
            super::InstallError::new(super::InstallErrorKind::Io, error.to_string())
        });
        let outcome = match written {
            Ok(()) => FileOutcome {
                role: file.role,
                name: file.name.clone(),
                dest: file.dest.clone(),
                status: FileStatus::Downloaded,
                bytes: payload.bytes.len() as u64,
                source: Some("metadata".to_string()),
                failure: None,
            },
            Err(error) => FileOutcome {
                role: file.role,
                name: file.name.clone(),
                dest: file.dest.clone(),
                status: FileStatus::Failed,
                bytes: 0,
                source: None,
                failure: Some(FileFailure {
                    kind: DownloadErrorKind::Io,
                    message: error.to_string(),
                }),
            },
        };
        if let Some(sink) = &sink {
            sink(&outcome);
        }
        outcomes.push(outcome);
        prefetched.push(file);
    }

    let pending: Vec<&PlanFile> = plan
        .files
        .iter()
        .filter(|file| !prefetched.iter().any(|done| done.dest == file.dest))
        .collect();

    let downloaded: Vec<FileOutcome> = stream::iter(pending)
        .map(|file| {
            let sink = sink.clone();
            async move {
                // **每个 item 的第一句就查标志**（复审 P2-1，p1）：`select` 先轮询左支，
                // 左支第一次 poll 就会把 `engine.download(...)` 派发出去 —— 只把标志检查写在
                // `select` 的右支，等于「取消之后每个剩余文件仍各发一次整文件 sha1 + 一次请求」，
                // 1.20.1 那种四位数文件的 plan 会一直走到列表末尾，而且「盘上已就绪」的文件
                // 第一 poll 就 `Ready` ⇒ 还会各回调一次 `sink`。
                // 放在这里，取消后这些 item **连 `install_one` 都不进**。
                if is_cancelled(cancel) {
                    return None;
                }
                // 已经开始的那些（`buffer_unordered` 在飞，最多 `concurrency` 个）与取消 future 赛跑：
                // 取消赢就 drop 掉下载 future（响应流关闭 + `.ullpart` 由 `PartFile::Drop` 清掉）。
                let outcome = match cancel {
                    Some(flag) => {
                        let download = install_one(engine, file);
                        let abort = cancelled(flag);
                        let mut download = pin!(download);
                        let mut abort = pin!(abort);
                        match select(download.as_mut(), abort.as_mut()).await {
                            Either::Left((outcome, _)) => Some(outcome),
                            Either::Right(((), _)) => None,
                        }
                    }
                    None => Some(install_one(engine, file).await),
                };
                if let (Some(sink), Some(outcome)) = (&sink, &outcome) {
                    sink(outcome);
                }
                outcome
            }
        })
        .buffer_unordered(concurrency.max(1))
        .filter_map(|outcome| async move { outcome })
        .collect()
        .await;
    outcomes.extend(downloaded);

    // 取消了就到此为止：**不写版本清单**（否则半成品会被 `is_version_installed()` 当成已安装）。
    if is_cancelled(cancel) {
        return Err(cancelled_error());
    }

    // 版本清单只在**全量就绪**时落盘：`is_version_installed()` 以清单存在为判据，
    // 半成品写进去会让 `instances list` 把"缺 jar 的版本"报成已安装。
    let complete = outcomes.iter().all(|outcome| !outcome.failed());
    let mut warnings = plan.warnings.clone();
    let manifest = if complete {
        let written = write_version_json(&plan.version_json_path, plan)?;
        // **M216 ③**：替换了别人的清单要大声说一句（`report.warnings` 会进 CLI 的 `--json`，
        // GUI 侧另有结构化的 `manifest` 字段可读）。内容相同（重装）不在此列。
        if let Some(backup) = written.backup() {
            warnings.push(format!(
                "已替换版本清单 {}（旧的那份备份在 {}）：这个 .minecraft 是共享的，别的启动器可能也用它",
                plan.version_json_path.display(),
                backup.display()
            ));
        }
        Some(written)
    } else {
        None
    };

    Ok(InstallReport {
        files: outcomes,
        version_json: complete.then(|| plan.version_json_path.clone()),
        manifest,
        warnings,
    })
}

/// 标志是否已置位（`None` = 没有取消通道，永远 false）。
///
/// 用 `SeqCst` 与 presenter 侧 `store` 的强度一致：这里不是性能热点（每个文件一次），
/// 换更强的顺序只买一个「不给自己留疑问」。
fn is_cancelled(cancel: Option<&AtomicBool>) -> bool {
    cancel.is_some_and(|flag| flag.load(Ordering::SeqCst))
}

/// 取消 future：标志一置位就 `Ready`，否则永远 `Pending`（不注册 waker）。
///
/// 为什么敢用「不注册 waker 的 Pending」：它只跟下载 future 一起被 `select` 轮询，
/// 下载每读到一个数据块就会唤醒 executor、连带重新 poll 这个 future，所以取消延迟
/// 天然是「一个数据块」；真正卡死的连接另有读超时兜底（`chunked` 的 30 s）。
async fn cancelled(flag: &AtomicBool) {
    poll_fn(|_| {
        if flag.load(Ordering::SeqCst) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await;
}

/// 取消的稳定错误（大类 `Cancelled`，不是失败）。
fn cancelled_error() -> InstallError {
    InstallError::new(
        InstallErrorKind::Cancelled,
        "已取消：剩下的文件没有开始下载，已下好的文件保留（重装续用）",
    )
}

/// 单文件：先判"已就绪"，否则走引擎下载。
async fn install_one(engine: &DownloadEngine, file: &PlanFile) -> FileOutcome {
    if let Some(bytes) = current_bytes(file) {
        return FileOutcome {
            role: file.role,
            name: file.name.clone(),
            dest: file.dest.clone(),
            status: FileStatus::Skipped,
            bytes,
            source: None,
            failure: None,
        };
    }
    match engine.download(file.request()).await {
        Ok(outcome) => FileOutcome {
            role: file.role,
            name: file.name.clone(),
            dest: file.dest.clone(),
            status: FileStatus::Downloaded,
            bytes: outcome.bytes,
            source: Some(outcome.source),
            failure: None,
        },
        Err(error) => FileOutcome {
            role: file.role,
            name: file.name.clone(),
            dest: file.dest.clone(),
            status: FileStatus::Failed,
            bytes: 0,
            source: None,
            failure: Some(describe(&error)),
        },
    }
}

/// 文件已就绪则返回盘上大小（跳过判定，见模块文档的三级分级）。
#[must_use]
pub fn current_bytes(file: &PlanFile) -> Option<u64> {
    if !file.dest.is_file() {
        return None;
    }
    let size = std::fs::metadata(&file.dest).ok()?.len();
    match file.sha1.as_deref().filter(|hex| !hex.is_empty()) {
        Some(expected) => {
            let actual = verify::digest_file(&file.dest, HashAlgo::Sha1).ok()?;
            verify::matches(expected, &actual).then_some(size)
        }
        None => match file.size {
            Some(expected) => (size == expected).then_some(size),
            // 没有摘要也没有大小：存在即信（后续"一键修复"再补校验）。
            None => Some(size),
        },
    }
}

/// 写解析后的版本清单（原子写），并如实交代「原来那份怎么办」（**M216 ③**）。
///
/// 三条判据：
/// - 目标不存在 → [`ManifestWrite::Fresh`]，正常写；
/// - 存在且内容**逐字节相同** → [`ManifestWrite::Identical`]，**一个字节都不写**（重装同版本
///   是正常操作：既不是冲突，也不该改别人的 mtime）；
/// - 存在且内容不同（含「存在但读不出来」——读不了 ≠ 相同）→ 旧的那份先 **copy** 到
///   `<dest>.ullbak-<pid>`，再原子写 ⇒ [`ManifestWrite::Replaced`]。
///
/// 备份用 `copy` 而不是 `rename`：整合包导入的合并态用 rename 是因为它紧接着覆盖同一路径、
/// 失败还会回滚；这里若 rename 之后原子写失败（目标正被别的启动器占着），盘上会**一份清单
/// 都不剩**。copy 之下最坏是「原件完好 + 多一份备份」。
fn write_version_json(
    path: &Path,
    plan: &InstallPlan,
) -> Result<ManifestWrite, super::InstallError> {
    let text = plan.version_json.to_json_string().map_err(|error| {
        super::InstallError::new(super::InstallErrorKind::Metadata, error.to_string())
    })?;
    let bytes = text.as_bytes();
    if std::fs::read(path).ok().as_deref() == Some(bytes) {
        return Ok(ManifestWrite::Identical);
    }
    let Some(backup) = path.is_file().then(|| backup_manifest(path)) else {
        write_atomic(path, bytes).map_err(io_error)?;
        return Ok(ManifestWrite::Fresh);
    };
    std::fs::copy(path, &backup).map_err(io_error)?;
    write_atomic(path, bytes).map_err(io_error)?;
    Ok(ManifestWrite::Replaced { backup })
}

/// IO 失败 → 安装错误（`std::io::Error` 与 `PlatformError` 都吃，取 `Display` 原文）。
fn io_error(error: impl std::fmt::Display) -> super::InstallError {
    super::InstallError::new(super::InstallErrorKind::Io, error.to_string())
}

/// `<dest>.ullbak-<pid>`（与整合包导入同一命名习惯，见 `addon/import/stage/commit.rs`）。
///
/// 同一进程里再替换一次时补 `-<n>`：否则第二份备份会盖掉第一份，而第一份很可能才是别人
/// 启动器铺的那份原文。
fn backup_manifest(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map_or_else(|| "version.json".into(), std::ffi::OsStr::to_os_string);
    name.push(format!(".ullbak-{}", std::process::id()));
    let mut candidate = path.with_file_name(&name);
    let mut seq = 1_u32;
    while candidate.exists() {
        let mut next = name.clone();
        next.push(format!("-{seq}"));
        candidate = path.with_file_name(next);
        seq += 1;
    }
    candidate
}

fn describe(error: &DownloadError) -> FileFailure {
    FileFailure {
        kind: error.kind,
        message: error.message.clone(),
    }
}
