//! 保存通道的**去抖攒批**部分（纯逻辑，可注入时钟，确定性单测）。
//!
//! 出处：`DevDocs/analyse/07-settings-packaging-update.md` §1.6（HMCL `FileSaver`）：
//! 入队 → 消费线程先 `sleep(200ms)` 等更多改动 → 全量 drain 去重 → 逐条 `saveSync`（tmp + move）。
//! 效果：一次窗口拖拽只产生一次写盘，而不是 60 次。
//!
//! 分层：本文件只管「什么时候写、写什么」；真正落盘由 [`JobSink`] 决定
//! （生产用 [`AtomicSink`]，测试用记录型 sink）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::error::SettingResult;

/// 默认去抖窗口（analyse/07 §1.6 的 200ms）。
pub const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(200);

/// 一条待写任务。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pending {
    payload: Vec<u8>,
    queued_at: Instant,
}

/// 按 key 去重的去抖队列：同一 key 的新内容覆盖旧内容，但**保留首次入队时间**
/// （窗口从第一次改动开始算，避免连续改动把写盘无限推迟）。
#[derive(Debug, Clone)]
pub struct DebounceQueue<K: Ord> {
    debounce: Duration,
    pending: BTreeMap<K, Pending>,
}

impl<K: Ord + Clone> DebounceQueue<K> {
    /// 新建一个去抖队列。
    #[must_use]
    pub fn new(debounce: Duration) -> Self {
        Self {
            debounce,
            pending: BTreeMap::new(),
        }
    }

    /// 去抖窗口。
    #[must_use]
    pub const fn debounce(&self) -> Duration {
        self.debounce
    }

    /// 入队（同 key 覆盖内容）。返回 `true` 表示是新增条目。
    pub fn enqueue(&mut self, key: K, payload: impl Into<Vec<u8>>, now: Instant) -> bool {
        match self.pending.get_mut(&key) {
            Some(existing) => {
                existing.payload = payload.into();
                false
            }
            None => {
                self.pending.insert(
                    key,
                    Pending {
                        payload: payload.into(),
                        queued_at: now,
                    },
                );
                true
            }
        }
    }

    /// 待写条目数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// 是否没有待写任务。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// 某 key 是否在队列里。
    #[must_use]
    pub fn contains(&self, key: &K) -> bool {
        self.pending.contains_key(key)
    }

    /// 最早该写盘的时刻。
    #[must_use]
    pub fn next_due(&self) -> Option<Instant> {
        self.pending
            .values()
            .map(|pending| pending.queued_at + self.debounce)
            .min()
    }

    /// 取出所有已过窗口的任务（按键排序，保证写盘顺序稳定）。
    pub fn drain_due(&mut self, now: Instant) -> Vec<(K, Vec<u8>)> {
        let due: Vec<K> = self
            .pending
            .iter()
            .filter(|(_, pending)| pending.queued_at + self.debounce <= now)
            .map(|(key, _)| key.clone())
            .collect();
        self.take(due)
    }

    /// 取出全部任务（关停兜底）。
    pub fn drain_all(&mut self) -> Vec<(K, Vec<u8>)> {
        let all: Vec<K> = self.pending.keys().cloned().collect();
        self.take(all)
    }

    fn take(&mut self, keys: Vec<K>) -> Vec<(K, Vec<u8>)> {
        keys.into_iter()
            .filter_map(|key| {
                self.pending
                    .remove(&key)
                    .map(|pending| (key, pending.payload))
            })
            .collect()
    }
}

impl<K: Ord + Clone> Default for DebounceQueue<K> {
    fn default() -> Self {
        Self::new(DEFAULT_DEBOUNCE)
    }
}

/// 真正落盘的执行者（测试可注入记录型实现）。
pub trait JobSink {
    /// 写一个文件。
    ///
    /// # Errors
    /// 落盘失败时返回 [`super::error::SettingError`]。
    fn write(&self, path: &Path, payload: &[u8]) -> SettingResult<()>;
}

/// 生产用 sink：tmp → fsync → rename。
#[derive(Debug, Clone, Copy, Default)]
pub struct AtomicSink;

impl JobSink for AtomicSink {
    fn write(&self, path: &Path, payload: &[u8]) -> SettingResult<()> {
        crate::platform::atomic::write_atomic(path, payload)?;
        Ok(())
    }
}

/// 去抖队列 + sink 的组合体（app 层持有它，或交给 [`super::save_worker::SaveWorker`] 托管）。
#[derive(Debug, Clone)]
pub struct CoalescingWriter<S: JobSink> {
    queue: DebounceQueue<PathBuf>,
    sink: S,
}

impl<S: JobSink> CoalescingWriter<S> {
    /// 组合去抖队列与 sink。
    #[must_use]
    pub fn new(sink: S, debounce: Duration) -> Self {
        Self {
            queue: DebounceQueue::new(debounce),
            sink,
        }
    }

    /// 入队一次写盘。
    pub fn queue(&mut self, path: impl Into<PathBuf>, payload: impl Into<Vec<u8>>, now: Instant) {
        self.queue.enqueue(path.into(), payload, now);
    }

    /// 待写条目数。
    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.queue.len()
    }

    /// 是否没有待写任务。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// 最早该写盘的时刻。
    #[must_use]
    pub fn next_due(&self) -> Option<Instant> {
        self.queue.next_due()
    }

    /// 写掉已过窗口的任务，返回写了几个文件。
    ///
    /// # Errors
    /// sink 落盘失败时返回 [`super::error::SettingError`]。
    pub fn flush_due(&mut self, now: Instant) -> SettingResult<usize> {
        let jobs = self.queue.drain_due(now);
        self.run(jobs)
    }

    /// 写掉全部任务（关停兜底），返回写了几个文件。
    ///
    /// # Errors
    /// sink 落盘失败时返回 [`super::error::SettingError`]。
    pub fn flush_all(&mut self) -> SettingResult<usize> {
        let jobs = self.queue.drain_all();
        self.run(jobs)
    }

    fn run(&mut self, jobs: Vec<(PathBuf, Vec<u8>)>) -> SettingResult<usize> {
        let mut written = 0usize;
        for (path, payload) in jobs {
            self.sink.write(&path, &payload)?;
            written += 1;
        }
        Ok(written)
    }
}
