//! processor 依赖库的下载：把 [`ProcessorPlan::libraries`] 交给 M27 的下载引擎。
//!
//! 为什么不直接复用 `game::install::execute`：那个入口在**全量就绪后写版本 JSON**，
//! 而 processors 的产物要先于版本清单落盘（版本 JSON 存在 = 这版真装完了）。
//! 所以这里只借它的**逐文件跳过判定**（`current_bytes`：摘要 → 大小 → 存在即信）
//! 与结果类型，下载本身仍走同一个引擎——镜像、健康度、退避、续传一条不少。

use futures_util::stream::{self, StreamExt};

use crate::download::DownloadEngine;
use crate::game::install::execute::current_bytes;
use crate::game::install::{
    FileFailure, FileOutcome, FileStatus, InstallReport, PlanFile, ProgressSink,
};

/// 并发下载 processor 依赖库（`concurrency` 上限，引擎内部另有信号量）。
///
/// 失败**不**中断：与 M27 一致，逐文件结构化返回，由调用方决定是否继续。
pub async fn download_libraries(
    engine: &DownloadEngine,
    files: &[PlanFile],
    concurrency: usize,
    sink: Option<ProgressSink>,
) -> InstallReport {
    let outcomes: Vec<FileOutcome> = stream::iter(files.iter())
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

    InstallReport {
        files: outcomes,
        version_json: None,
        manifest: None,
        warnings: Vec::new(),
    }
}

/// 单文件：先判「已就绪」，否则走引擎下载。
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
            failure: Some(FileFailure {
                kind: error.kind,
                message: error.message.clone(),
            }),
        },
    }
}
