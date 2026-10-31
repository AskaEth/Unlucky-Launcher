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

use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures_util::stream::{self, StreamExt};

use super::plan::{FileRole, PlanFile};
use super::InstallPlan;
use crate::download::verify;
use crate::download::{DownloadEngine, DownloadError, DownloadErrorKind, HashAlgo};
use crate::platform::atomic::write_atomic;

/// 单个文件的结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    /// 本次真的下载（或从缓存直取）并落盘。
    Downloaded,
    /// 已存在且校验通过，跳过。
    Skipped,
    /// 失败（原因在 [`FileOutcome::failure`]）。
    Failed,
}

impl FileStatus {
    /// 稳定字符串 id。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Downloaded => "downloaded",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
        }
    }
}

/// 一次失败的分类信息（调用方据此给 `install_network` / `install_integrity` 等稳定码）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFailure {
    /// 下载错误分类。
    pub kind: DownloadErrorKind,
    /// 人读说明。
    pub message: String,
}

/// 一个文件的执行结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileOutcome {
    /// 角色。
    pub role: FileRole,
    /// 人读标识。
    pub name: String,
    /// 落盘路径。
    pub dest: std::path::PathBuf,
    /// 结局。
    pub status: FileStatus,
    /// 实际字节数（跳过时为盘上大小）。
    pub bytes: u64,
    /// 成功的源标识（`bmclapi` / `official` / `cache` / `metadata`）。
    pub source: Option<String>,
    /// 失败详情。
    pub failure: Option<FileFailure>,
}

impl FileOutcome {
    /// 是否失败。
    #[must_use]
    pub fn failed(&self) -> bool {
        self.status == FileStatus::Failed
    }
}

/// 进度回调：每完成一个文件调用一次（可跨线程，由执行器持有 `Arc`）。
///
/// CLI 用它往 stderr 打实时进度；GUI 侧直接消费 `ProgressEvent` 事件流即可，不必用这个。
pub type ProgressSink = Arc<dyn Fn(&FileOutcome) + Send + Sync>;

/// 一次安装的执行结果。
#[derive(Debug, Clone)]
pub struct InstallReport {
    /// 逐文件结果（顺序不保证，按角色/大小汇总即可）。
    pub files: Vec<FileOutcome>,
    /// 版本清单落盘路径；`None` = 有文件失败，故意没写（见 [`execute`] 的说明）。
    pub version_json: Option<PathBuf>,
    /// 计划阶段带来的告警（缺件降级说明）。
    pub warnings: Vec<String>,
}

impl InstallReport {
    /// 真正下载的文件数。
    #[must_use]
    pub fn downloaded(&self) -> usize {
        self.files
            .iter()
            .filter(|file| file.status == FileStatus::Downloaded)
            .count()
    }

    /// 跳过（已存在且校验通过）的文件数。
    #[must_use]
    pub fn skipped(&self) -> usize {
        self.files
            .iter()
            .filter(|file| file.status == FileStatus::Skipped)
            .count()
    }

    /// 失败的文件数。
    #[must_use]
    pub fn failed(&self) -> usize {
        self.files.iter().filter(|file| file.failed()).count()
    }

    /// 本次写入的字节数（只算真正下载的）。
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.files
            .iter()
            .filter(|file| file.status == FileStatus::Downloaded)
            .map(|file| file.bytes)
            .sum()
    }

    /// 是否全部就绪（无失败）。
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.failed() == 0
    }

    /// 失败文件列表（最多取前 `limit` 条，给 CLI 拼提示用）。
    #[must_use]
    pub fn failures(&self, limit: usize) -> Vec<&FileOutcome> {
        self.files
            .iter()
            .filter(|file| file.failed())
            .take(limit)
            .collect()
    }

    /// 失败里最"严重"的大类：有校验不符就先报它，其余按网络/IO 归。
    #[must_use]
    pub fn failure_kind(&self) -> Option<DownloadErrorKind> {
        let mut fallback = None;
        for file in self.files.iter().filter(|file| file.failed()) {
            let Some(failure) = &file.failure else {
                continue;
            };
            if failure.kind == DownloadErrorKind::Integrity {
                return Some(DownloadErrorKind::Integrity);
            }
            fallback.get_or_insert(failure.kind);
        }
        fallback
    }
}

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
                let outcome = install_one(engine, file).await;
                if let Some(sink) = &sink {
                    sink(&outcome);
                }
                outcome
            }
        })
        .buffer_unordered(concurrency.max(1))
        .collect()
        .await;
    outcomes.extend(downloaded);

    // 版本清单只在**全量就绪**时落盘：`is_version_installed()` 以清单存在为判据，
    // 半成品写进去会让 `instances list` 把"缺 jar 的版本"报成已安装。
    let complete = outcomes.iter().all(|outcome| !outcome.failed());
    let version_json = if complete {
        write_version_json(&plan.version_json_path, plan)?;
        Some(plan.version_json_path.clone())
    } else {
        None
    };

    Ok(InstallReport {
        files: outcomes,
        version_json,
        warnings: plan.warnings.clone(),
    })
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

/// 写解析后的版本清单（原子写）。
fn write_version_json(path: &Path, plan: &InstallPlan) -> Result<(), super::InstallError> {
    let text = plan.version_json.to_json_string().map_err(|error| {
        super::InstallError::new(super::InstallErrorKind::Metadata, error.to_string())
    })?;
    write_atomic(path, text.as_bytes())
        .map_err(|error| super::InstallError::new(super::InstallErrorKind::Io, error.to_string()))
}

fn describe(error: &DownloadError) -> FileFailure {
    FileFailure {
        kind: error.kind,
        message: error.message.clone(),
    }
}
