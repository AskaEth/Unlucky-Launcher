//! 安装进度的记账器（文件计数 / 字节 / 速度 / ETA）。
//!
//! `execute` 的文件级回调已经把每个文件的结局给了我们，这里只做聚合；
//! 速度用相邻两次采样的字节差算（core 的事件只给绝对值）。

use std::sync::Arc;
use std::time::Instant;

use unlucky_core::game::install::{FileOutcome, FileStatus, ProgressSink};

use crate::dto::{derive_speed, CreateRequest, InstallProgressView, InstallStage};

use super::install::loader_label;
use super::job::{HostTask, JobSender};
/// 一次安装的上下文（进度回调共享）。
pub(super) struct Meter {
    pub(super) progress: InstallProgressView,
    previous: Option<(u64, Instant)>,
}

impl Meter {
    pub(super) fn new(request: &CreateRequest) -> Self {
        Self {
            progress: InstallProgressView::start(
                &request.version,
                &request.instance_name(),
                &loader_label(request.loader, &request.loader_version),
            ),
            previous: None,
        }
    }

    /// 换阶段（重置速度采样）。
    pub(super) fn stage(&mut self, stage: InstallStage, message: &str) {
        self.progress.stage = stage;
        self.progress.message = message.to_string();
        self.previous = None;
    }

    /// 预记账一段待下载的文件（文件数 + 字节）。
    ///
    /// 一次安装可能分几段（原版 / 处理器依赖库 / 加载器库）：每段开工前都先记账，
    /// 整条进度曲线才是单调的——某一段下完不会先到 100%，再因为下一段的文件被计入而回落。
    pub(super) fn expect(&mut self, files: usize, bytes: u64) {
        let files = u64::try_from(files).unwrap_or(u64::MAX);
        self.progress.files_total = self.progress.files_total.saturating_add(files);
        self.progress.bytes_total = self.progress.bytes_total.saturating_add(bytes);
    }

    /// 记账一个文件的结局。
    pub(super) fn note(&mut self, outcome: &FileOutcome) {
        self.progress.files_done += 1;
        if outcome.status != FileStatus::Skipped {
            self.progress.bytes_done = self.progress.bytes_done.saturating_add(outcome.bytes);
        }
        let now = Instant::now();
        if let Some(previous) = self.previous {
            if let Some((speed, eta)) = derive_speed(
                previous,
                (self.progress.bytes_done, now),
                self.progress.bytes_total,
            ) {
                self.progress.speed_bps = speed;
                self.progress.eta_secs = eta;
            }
        }
        self.previous = Some((self.progress.bytes_done, now));
    }
}

/// 进度 sink（`execute` 的文件级回调）。
pub(super) fn sink_for(tx: JobSender, meter: Arc<std::sync::Mutex<Meter>>) -> ProgressSink {
    Arc::new(move |outcome: &FileOutcome| {
        let Ok(mut meter) = meter.lock() else {
            return;
        };
        meter.note(outcome);
        let _ = tx.send(HostTask::Install(Box::new(meter.progress.clone())));
    })
}
