//! 进程树回收：`SIGTERM → 宽限 → SIGKILL` 三段式关闭。
//!
//! HMCL 只 `process.destroy()` 单进程（`analyse/02` §6.8），子进程会残留、日志尾部丢失。
//! 这里做进程**树**回收：
//!
//! - Windows：`taskkill /T /F /PID`（`/T` 递归子树）。P0 不引入 `windows-sys`，因此没有
//!   Job Object——Job Object（`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`）留 P1，替换点就在本文件
//!   （tower 已批准，见 `launch-接口约定.md` §7-1）。
//! - POSIX：`kill -TERM -<pid>`（命不中是常态，因为我们没有 `setsid`——`unsafe` pre_exec 被
//!   workspace `unsafe_code = deny` 挡住）→ 宽限 → `kill -KILL -<pid>`，再退到单 pid。
//!
//! 两个平台都先尝试“温和”，宽限期后再“强制”。
//!
//! ⚠️ 外部命令的退出码**不能**用来判断“进程是否已被结束”：温和阶段成功关掉进程后，
//! 再发 `taskkill /F` 会得到 128（“没有这个运行实例”）。所以 [`crate::launch::process`]
//! 的实际停止流程以“进程是否真的消失”为准（`try_wait` 轮询），本模块只提供发送动作。

use std::io;
use std::process::Command;
use std::thread;
use std::time::Duration;

/// 温和关闭（Windows 无 `/F`；POSIX `SIGTERM`）。
pub fn terminate(pid: u32) -> io::Result<()> {
    imp::signal(pid, false)
}

/// 强制关闭（Windows `/F`；POSIX `SIGKILL`），并尽量覆盖整棵进程树。
pub fn force_kill(pid: u32) -> io::Result<()> {
    imp::signal(pid, true)
}

/// 三段式：温和 → 宽限 → 强制。返回**强制阶段**的结果（温和阶段失败不致命）。
///
/// 注意：返回值不代表“进程已经死了”（见模块注释）。需要准确判定请用
/// [`crate::launch::process::RunningProcess::stop`]。
pub fn kill_tree(pid: u32, grace: Duration) -> io::Result<()> {
    if pid == 0 {
        return Err(io::Error::other("无效的 pid 0"));
    }
    let _ = terminate(pid);
    if !grace.is_zero() {
        thread::sleep(grace);
    }
    force_kill(pid)
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::os::windows::process::CommandExt;

    /// 不弹控制台窗口（否则 kill 子进程会闪一个黑框）。
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub(super) fn signal(pid: u32, force: bool) -> io::Result<()> {
        let mut command = Command::new("taskkill");
        command.arg("/PID").arg(pid.to_string()).arg("/T");
        if force {
            command.arg("/F");
        }
        command.creation_flags(CREATE_NO_WINDOW);
        let status = command
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?;
        if status.success() {
            Ok(())
        } else {
            // taskkill 对已退出的进程返回非 0，这不值得中断关闭流程。
            Err(io::Error::other(format!("taskkill 退出码 {status}")))
        }
    }
}

#[cfg(unix)]
mod imp {
    use super::*;

    pub(super) fn signal(pid: u32, force: bool) -> io::Result<()> {
        let flag = if force { "-KILL" } else { "-TERM" };
        // 先按进程组（`-<pid>`）；没有 setsid 时通常失败，再退到单 pid。
        if run_kill(flag, &format!("-{pid}"))?.is_some() {
            return Ok(());
        }
        match run_kill(flag, &pid.to_string())? {
            Some(status) if status.success() => Ok(()),
            Some(status) => Err(io::Error::other(format!("kill 退出码 {status}"))),
            None => Err(io::Error::other("找不到 kill 命令")),
        }
    }

    fn run_kill(flag: &str, target: &str) -> io::Result<Option<std::process::ExitStatus>> {
        match Command::new("kill")
            .arg(flag)
            .arg(target)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
        {
            Ok(status) if status.success() => Ok(Some(status)),
            Ok(_) => Ok(None),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }
}

#[cfg(not(any(windows, unix)))]
mod imp {
    use super::*;

    pub(super) fn signal(_pid: u32, _force: bool) -> io::Result<()> {
        Err(io::Error::other("当前平台不支持进程树 kill"))
    }
}
