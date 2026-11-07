//! 进度事件与发射器。
//!
//! 事件是「字节级 + 文件级」双层的载体（设计文档 §5）：文件级由 `Started/Completed/Failed`
//! 表达，字节级由节流后的 `Progress` 表达。`total_bytes` 未知时给 `None`——不做假进度。

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use super::error::DownloadErrorKind;
use super::types::{DownloadId, HashAlgo, ResourceKind};

/// 一条下载进度事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ProgressEvent {
    /// 开始一次下载（含候选数量与总字节预算）。
    Started {
        /// 关联 id。
        id: DownloadId,
        /// 资源类型。
        kind: ResourceKind,
        /// 逻辑 URL。
        url: String,
        /// 落盘路径。
        dest: String,
        /// 候选源数量。
        candidates: usize,
        /// 总字节数（未知为 `None`）。
        total_bytes: Option<u64>,
    },
    /// 字节级进度（节流后）。
    Progress {
        /// 关联 id。
        id: DownloadId,
        /// 已下载字节。
        downloaded: u64,
        /// 总字节数（未知为 `None`）。
        total_bytes: Option<u64>,
    },
    /// 某个源失败（单次尝试）。
    SourceFailed {
        /// 关联 id。
        id: DownloadId,
        /// 源标识。
        source: String,
        /// 失败原因（短描述）。
        reason: String,
        /// 第几次尝试（从 0 起）。
        attempt: u32,
        /// 是否还会在同源重试。
        will_retry: bool,
        /// 下次重试的等待毫秒数。
        retry_in_ms: u64,
    },
    /// 切换到下一个候选源。
    SourceSwitched {
        /// 关联 id。
        id: DownloadId,
        /// 原源。
        from: String,
        /// 新源。
        to: String,
    },
    /// 进入校验阶段。
    Verifying {
        /// 关联 id。
        id: DownloadId,
        /// 算法。
        algo: HashAlgo,
        /// 期望摘要前缀（8 位，够定位又不太啰嗦）。
        expected_prefix: String,
    },
    /// 下载成功。
    Completed {
        /// 关联 id。
        id: DownloadId,
        /// 字节数。
        bytes: u64,
        /// 是否命中内容寻址缓存。
        from_cache: bool,
        /// 成功的源。
        source: String,
        /// 耗时毫秒。
        elapsed_ms: u64,
    },
    /// 下载失败（所有候选源都失败或不可重试）。
    Failed {
        /// 关联 id。
        id: DownloadId,
        /// 错误分类。
        kind: DownloadErrorKind,
        /// 错误描述。
        message: String,
    },
}

impl ProgressEvent {
    /// 事件归属的下载 id。
    #[must_use]
    pub const fn id(&self) -> DownloadId {
        match self {
            Self::Started { id, .. }
            | Self::Progress { id, .. }
            | Self::SourceFailed { id, .. }
            | Self::SourceSwitched { id, .. }
            | Self::Verifying { id, .. }
            | Self::Completed { id, .. }
            | Self::Failed { id, .. } => *id,
        }
    }
}

/// 进度发射器：没有接收端时退化为空操作（core 不关心谁在听）。
#[derive(Debug, Clone, Default)]
pub struct ProgressEmitter {
    tx: Option<UnboundedSender<ProgressEvent>>,
}

impl ProgressEmitter {
    /// 静默发射器（测试、后台静默下载用）。
    #[must_use]
    pub const fn silent() -> Self {
        Self { tx: None }
    }

    /// 建一对（发射器, 接收端）；接收端由 app 层转成 IPC DTO。
    #[must_use]
    pub fn channel() -> (Self, UnboundedReceiver<ProgressEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx: Some(tx) }, rx)
    }

    /// 发一条事件；接收端已关闭则静默丢弃。
    pub fn emit(&self, event: ProgressEvent) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(event);
        }
    }

    /// 是否没有接收端。
    #[must_use]
    pub const fn is_silent(&self) -> bool {
        self.tx.is_none()
    }
}

/// 进度节流器：距上次发射不足 `interval` 且未完成时压掉本次事件。
#[derive(Debug, Clone)]
pub struct ProgressThrottle {
    interval: Duration,
    last_emit: Option<Instant>,
}

impl ProgressThrottle {
    /// 构造节流器。
    #[must_use]
    pub const fn new(interval: Duration) -> Self {
        Self {
            interval,
            last_emit: None,
        }
    }

    /// 节流间隔。
    #[must_use]
    pub const fn interval(&self) -> Duration {
        self.interval
    }

    /// 是否应该发射本次进度（`finished = true` 时无条件放行）。
    pub fn should_emit(&mut self, now: Instant, finished: bool) -> bool {
        if finished {
            self.last_emit = Some(now);
            return true;
        }
        match self.last_emit {
            Some(last) if now.duration_since(last) < self.interval => false,
            _ => {
                self.last_emit = Some(now);
                true
            }
        }
    }

    /// 复位（换源/重试时重新计时）。
    pub fn reset(&mut self) {
        self.last_emit = None;
    }
}

impl Default for ProgressThrottle {
    fn default() -> Self {
        Self::new(Duration::from_millis(250))
    }
}
