//! 进程管理：spawn + stdout/stderr 泵 + 进程树回收。
//!
//! 对应 `analyse/02` §6 的四条线程模型（`stdout-pump` / `stderr-pump` / `exit-waiter`），
//! 但线程生命周期的收尾由 [`crate::launch::watchdog`] 兜底，不无条件 `join`。
//!
//! 本模块只管“把命令行跑起来并收集输出”，不做组件检查、不做诊断——
//! 那些是 `launch` 上层流水线（阶段B）与 M15 的职责。

use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::launch::args_table::AssembledCommand;
use crate::launch::error::{LaunchError, Result};
use crate::launch::pump::{LineSink, LogLine, PumpGroup, RingBuffer, RingBufferConfig};
use crate::launch::tree_kill;
use crate::launch::watchdog::{self, PumpVerdict, WatchdogConfig};

/// 强制结束后等进程消失的预算。
const FORCE_KILL_BUDGET: Duration = Duration::from_secs(5);
/// 轮询进程状态的间隔。
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// 进程管理配置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessConfig {
    /// 日志泵等待预算。
    pub watchdog: WatchdogConfig,
    /// 停止时温和关闭的宽限期。
    pub graceful_stop: Duration,
    /// 环形缓冲配置。
    pub buffer: RingBufferConfig,
}

impl Default for ProcessConfig {
    fn default() -> Self {
        Self {
            watchdog: WatchdogConfig::default(),
            graceful_stop: Duration::from_secs(3),
            buffer: RingBufferConfig::default(),
        }
    }
}

/// 已启动的游戏进程。
#[derive(Debug)]
pub struct RunningProcess {
    child: Child,
    pump: PumpGroup,
    buffer: Arc<Mutex<RingBuffer>>,
    started_at: Instant,
    pid: u32,
    config: ProcessConfig,
}

impl RunningProcess {
    /// 启动进程并接管两条输出流。
    pub fn spawn(
        command: &AssembledCommand,
        working_dir: Option<&Path>,
        sink: Arc<dyn LineSink>,
        config: ProcessConfig,
    ) -> Result<Self> {
        let mut process = Command::new(&command.program);
        process.args(&command.args);
        for (key, value) in &command.env {
            process.env(key, value);
        }
        if let Some(dir) = working_dir {
            process.current_dir(dir);
        }
        process
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            /// 独立进程组：便于 `taskkill /T` 递归回收。
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
            process.creation_flags(CREATE_NEW_PROCESS_GROUP);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            // 与 Windows 的 CREATE_NEW_PROCESS_GROUP 对称：子进程自成一个进程组，
            // `tree_kill` 的 `kill -- -<pgid>` 才能覆盖整棵子树。`process_group` 是
            // std 1.64+ 的**安全** API，不需要 pre_exec / unsafe（旧注释说“unsafe 被挡”已过时）。
            // 参数 0 = `pgid = 子进程 pid`。
            process.process_group(0);
        }

        let mut child = process.spawn().map_err(|source| LaunchError::Spawn {
            program: command.program.clone(),
            source,
        })?;
        let pid = child.id();
        let buffer = Arc::new(Mutex::new(RingBuffer::new(config.buffer)));
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let pump = PumpGroup::spawn(stdout, stderr, buffer.clone(), sink);
        Ok(Self {
            child,
            pump,
            buffer,
            started_at: Instant::now(),
            pid,
            config,
        })
    }

    /// 进程号。
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// 已运行毫秒数。
    pub fn uptime_ms(&self) -> u64 {
        u64::try_from(self.started_at.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// 启动时刻（窗口检测计时用）。
    pub fn started_at(&self) -> Instant {
        self.started_at
    }

    /// 共享的日志环形缓冲。
    pub fn buffer(&self) -> Arc<Mutex<RingBuffer>> {
        self.buffer.clone()
    }

    /// 当前可读日志快照。
    pub fn lines(&self) -> Vec<LogLine> {
        match self.buffer.lock() {
            Ok(guard) => guard.lines().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 等待进程退出（不等待日志泵）。
    pub fn wait(&mut self) -> Result<ExitStatus> {
        self.child.wait().map_err(|source| LaunchError::Io {
            op: "wait_child",
            path: PathBuf::from(self.pid.to_string()),
            source,
        })
    }

    /// 进程退出后，按预算收尾日志泵。
    pub fn supervise(&mut self) -> PumpVerdict {
        watchdog::wait_for_pumps(&mut self.pump, &self.config.watchdog)
    }

    /// 进程是否仍在运行。
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// 三段式停止整棵进程树（温和 → 宽限 → 强制）。
    ///
    /// 判定标准是**进程是否真的没了**，不是 `taskkill`/`kill` 的退出码：
    /// 外部命令对“已经退出的进程”会返回非 0（Windows 上 `taskkill` 报 128），
    /// 把它当失败会误报——尤其是温和阶段已经把进程关掉的情况。
    pub fn stop(&mut self) -> Result<()> {
        if !self.is_running() {
            return Ok(());
        }
        let _ = tree_kill::terminate(self.pid);
        if self.wait_until_exit(self.config.graceful_stop) {
            return Ok(());
        }
        let _ = tree_kill::force_kill(self.pid);
        if self.wait_until_exit(FORCE_KILL_BUDGET) {
            return Ok(());
        }
        Err(LaunchError::Kill {
            pid: self.pid,
            source: std::io::Error::other("强制结束后进程仍在运行"),
        })
    }

    /// 直接强杀（看门狗/崩溃兜底路径）。
    pub fn force_kill(&mut self) -> Result<()> {
        if !self.is_running() {
            return Ok(());
        }
        let _ = tree_kill::force_kill(self.pid);
        if self.wait_until_exit(FORCE_KILL_BUDGET) {
            Ok(())
        } else {
            Err(LaunchError::Kill {
                pid: self.pid,
                source: std::io::Error::other("强制结束后进程仍在运行"),
            })
        }
    }

    fn wait_until_exit(&mut self, budget: Duration) -> bool {
        let deadline = Instant::now() + budget;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return true,
                Ok(None) => {}
                Err(_) => return false,
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }
}

/// 从退出状态取整数退出码。
///
/// POSIX 上被子进程信号杀死没有 `code()`，按 shell 惯例补 `128 + signal`
/// （`SIGKILL` → 137、`SIGTERM` → 143，与 `analyse/02` 附录 C 一致）。
pub fn exit_code(status: &ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    -1
}
