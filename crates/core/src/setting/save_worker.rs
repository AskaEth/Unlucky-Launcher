//! 去抖保存的线程封装：app 层持有它，界面改配置只入队，后台线程负责攒批落盘。
//!
//! 出处：`DevDocs/analyse/07-settings-packaging-update.md` §1.6（HMCL `FileSaver` 消费线程 +
//! 关停钩子 `waitForAllSaves()`）。本实现的「攒批/去抖/落盘」逻辑在 [`super::save`]，
//! 这里只做线程与命令通道，方便对时序逻辑保持确定性单测。

use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::error::{SettingError, SettingResult};
use super::save::{AtomicSink, CoalescingWriter, DEFAULT_DEBOUNCE};

/// 线程没起时的兜底路径标记（只为让 [`SettingError::Io`] 有出处）。
const WORKER_PATH: &str = "<save-worker>";

enum Command {
    Queue { path: PathBuf, payload: Vec<u8> },
    Flush(Sender<SettingResult<()>>),
    Shutdown(Sender<SettingResult<()>>),
}

/// 后台保存线程句柄。
#[derive(Debug)]
pub struct SaveWorker {
    tx: Option<Sender<Command>>,
    handle: Option<JoinHandle<()>>,
}

impl SaveWorker {
    /// 用默认 200ms 去抖窗口起线程。
    ///
    /// # Errors
    /// 线程创建失败时返回 [`SettingError::Io`]。
    pub fn spawn_default() -> SettingResult<Self> {
        Self::spawn(DEFAULT_DEBOUNCE)
    }

    /// 用指定去抖窗口起线程。
    ///
    /// # Errors
    /// 线程创建失败时返回 [`SettingError::Io`]。
    pub fn spawn(debounce: Duration) -> SettingResult<Self> {
        let (tx, rx) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("ull-save".to_owned())
            .spawn(move || run_loop(&rx, debounce))
            .map_err(io_error)?;
        Ok(Self {
            tx: Some(tx),
            handle: Some(handle),
        })
    }

    /// 入队一次写盘（不阻塞）。
    ///
    /// # Errors
    /// 保存线程已停止时返回 [`SettingError::Io`]。
    pub fn queue(
        &self,
        path: impl Into<PathBuf>,
        payload: impl Into<Vec<u8>>,
    ) -> SettingResult<()> {
        let command = Command::Queue {
            path: path.into(),
            payload: payload.into(),
        };
        self.send(command)
    }

    /// 同步落盘（返回时队列已空，且错误已带回当前线程）。
    ///
    /// # Errors
    /// 保存线程已停止，或后台落盘失败时返回 [`SettingError`]。
    pub fn flush(&self) -> SettingResult<()> {
        let (ack_tx, ack_rx) = mpsc::channel();
        self.send(Command::Flush(ack_tx))?;
        ack_rx.recv().map_err(|_| {
            io_error(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "保存线程未回传结果",
            ))
        })?
    }

    /// drain 全部任务后停止线程。
    ///
    /// # Errors
    /// 落盘失败时返回 [`SettingError`]。
    pub fn shutdown(&mut self) -> SettingResult<()> {
        let Some(tx) = self.tx.take() else {
            return Ok(());
        };
        let (ack_tx, ack_rx) = mpsc::channel();
        let sent = tx.send(Command::Shutdown(ack_tx));
        let result = match sent {
            Ok(()) => ack_rx.recv().unwrap_or_else(|_| {
                Err(io_error(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "保存线程未回传结果",
                )))
            }),
            Err(_) => Ok(()),
        };
        if let Some(handle) = self.handle.take() {
            handle.join().ok();
        }
        result
    }

    fn send(&self, command: Command) -> SettingResult<()> {
        let Some(tx) = self.tx.as_ref() else {
            return Err(io_error(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "保存线程已停止",
            )));
        };
        tx.send(command)
            .map_err(|_| io_error(io::Error::new(io::ErrorKind::BrokenPipe, "保存线程已停止")))
    }
}

impl Drop for SaveWorker {
    fn drop(&mut self) {
        self.shutdown().ok();
    }
}

fn run_loop(rx: &Receiver<Command>, debounce: Duration) {
    let mut writer = CoalescingWriter::new(AtomicSink, debounce);
    let mut last_error: Option<SettingError> = None;

    loop {
        let wait = writer
            .next_due()
            .map(|due| due.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_secs(3600))
            .max(Duration::from_millis(1));

        match rx.recv_timeout(wait) {
            Ok(Command::Queue { path, payload }) => {
                writer.queue(path, payload, Instant::now());
            }
            Ok(Command::Flush(ack)) => {
                let result = combine(last_error.take(), writer.flush_all());
                ack.send(result).ok();
            }
            Ok(Command::Shutdown(ack)) => {
                let result = combine(last_error.take(), writer.flush_all());
                ack.send(result).ok();
                break;
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Err(err) = writer.flush_due(Instant::now()) {
                    last_error = Some(err);
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                writer.flush_all().ok();
                break;
            }
        }
    }
}

/// 后台静默失败过的错误优先上报（不吞掉）。
fn combine(previous: Option<SettingError>, result: SettingResult<usize>) -> SettingResult<()> {
    match previous {
        Some(err) => Err(err),
        None => result.map(|_| ()),
    }
}

fn io_error(source: io::Error) -> SettingError {
    SettingError::Io {
        path: PathBuf::from(WORKER_PATH),
        source,
    }
}
