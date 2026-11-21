//! 保存通道：200ms 去抖、同路径覆盖、攒批落盘、后台线程 flush/关停。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use unlucky_core::setting::error::SettingResult;
use unlucky_core::setting::save::{CoalescingWriter, DebounceQueue, JobSink, DEFAULT_DEBOUNCE};
use unlucky_core::setting::save_worker::SaveWorker;

use crate::common::TempDir;

/// 记录下来的 (路径, 内容) 列表。
type Written = Vec<(PathBuf, Vec<u8>)>;

#[derive(Clone, Default)]
struct RecordingSink {
    written: Arc<Mutex<Written>>,
}

impl RecordingSink {
    fn entries(&self) -> Vec<(PathBuf, Vec<u8>)> {
        self.written.lock().expect("锁").clone()
    }
}

impl JobSink for RecordingSink {
    fn write(&self, path: &Path, payload: &[u8]) -> SettingResult<()> {
        self.written
            .lock()
            .expect("锁")
            .push((path.to_path_buf(), payload.to_vec()));
        Ok(())
    }
}

#[test]
fn default_debounce_is_200ms() {
    assert_eq!(DEFAULT_DEBOUNCE, Duration::from_millis(200));
}

#[test]
fn waits_for_the_window_before_writing() {
    let start = Instant::now();
    let mut queue = DebounceQueue::<String>::new(DEFAULT_DEBOUNCE);
    assert!(queue.is_empty());

    queue.enqueue("a".to_owned(), b"1".to_vec(), start);
    queue.enqueue("b".to_owned(), b"2".to_vec(), start);
    assert_eq!(queue.len(), 2);
    assert!(queue.contains(&"a".to_owned()));
    assert_eq!(queue.next_due(), Some(start + DEFAULT_DEBOUNCE));

    assert!(queue
        .drain_due(start + Duration::from_millis(100))
        .is_empty());
    let due = queue.drain_due(start + DEFAULT_DEBOUNCE);
    assert_eq!(due.len(), 2);
    assert_eq!(due[0].0, "a", "同一批按键排序，写盘顺序稳定");
    assert!(queue.is_empty());
}

#[test]
fn same_key_collapses_and_keeps_first_deadline() {
    let start = Instant::now();
    let mut queue = DebounceQueue::<String>::new(DEFAULT_DEBOUNCE);
    queue.enqueue("a".to_owned(), b"1".to_vec(), start);
    queue.enqueue(
        "a".to_owned(),
        b"2".to_vec(),
        start + Duration::from_millis(150),
    );

    assert_eq!(queue.len(), 1);
    assert_eq!(
        queue.next_due(),
        Some(start + DEFAULT_DEBOUNCE),
        "窗口从首次改动开始算，避免连续改动无限推迟写盘"
    );
    assert_eq!(
        queue.drain_due(start + DEFAULT_DEBOUNCE),
        vec![("a".to_owned(), b"2".to_vec())]
    );
}

#[test]
fn coalescing_writer_batches_into_one_flush() {
    let sink = RecordingSink::default();
    let mut writer = CoalescingWriter::new(sink.clone(), DEFAULT_DEBOUNCE);
    let start = Instant::now();

    writer.queue("/tmp/a.json", b"1".to_vec(), start);
    writer.queue(
        "/tmp/a.json",
        b"2".to_vec(),
        start + Duration::from_millis(50),
    );
    writer.queue("/tmp/b.json", b"3".to_vec(), start);
    assert_eq!(writer.pending_len(), 2);
    assert!(!writer.is_empty());

    assert_eq!(writer.flush_due(start).expect("未到窗口"), 0);
    assert_eq!(
        writer.flush_due(start + DEFAULT_DEBOUNCE).expect("到窗口"),
        2
    );
    assert!(writer.is_empty());
    assert_eq!(writer.flush_all().expect("空队列兜底"), 0);

    let written = sink.entries();
    assert_eq!(written.len(), 2);
    assert_eq!(written[0].0, PathBuf::from("/tmp/a.json"));
    assert_eq!(written[0].1, b"2".to_vec(), "只写最新内容");
}

#[test]
fn save_worker_flushes_and_shuts_down() {
    let temp = TempDir::new("save-worker");
    let target = temp.child("config/launcher.json");
    let mut worker = SaveWorker::spawn(Duration::from_millis(30)).expect("起线程");

    worker.queue(&target, b"{\"a\":1}").expect("入队");
    worker.queue(&target, b"{\"a\":2}").expect("入队");
    worker.flush().expect("同步落盘");

    assert_eq!(temp.read("config/launcher.json"), "{\"a\":2}");
    worker.shutdown().expect("关停");
}
