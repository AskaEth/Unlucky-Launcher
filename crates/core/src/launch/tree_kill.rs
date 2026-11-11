//! 进程树回收：`SIGTERM → 宽限 → SIGKILL` 三段式关闭。
//!
//! HMCL 只 `process.destroy()` 单进程（`analyse/02` §6.8），子进程会残留、日志尾部丢失。
//! 这里做进程**树**回收：
//!
//! - Windows：`taskkill /T /F /PID`（`/T` 递归子树）。P0 不引入 `windows-sys`，因此没有
//!   Job Object——Job Object（`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`）留 P1，替换点就在本文件
//!   （tower 已批准，见 `launch-接口约定.md` §7-1）。
//! - POSIX：子进程由 [`crate::launch::process`] 用 `CommandExt::process_group(0)` 放进**自己的**
//!   进程组（与 Windows 的 `CREATE_NEW_PROCESS_GROUP` 对称，std 1.64+ 的安全 API），因此
//!   `kill -TERM -- -<pgid>` 一次覆盖整棵子树 → 宽限 → `kill -KILL -- -<pgid>`，再补一发单 pid。
//!
//! 两个平台都先尝试“温和”，宽限期后再“强制”。
//!
//! # 两条来自 CI 的硬约束
//!
//! 1. **组杀必须先验组**（[`group_kill_target`]）：目标进程必须是它自己进程组的**组长**，
//!    且该组不能是我们所在的组。否则一次 `kill -- -<pgid>` 会把测试进程（CI 容器里全体进程
//!    常常共用 pgid 1）一起带走——M48 之前 `test (ubuntu-latest)` 就是这么被 SIGTERM 打断的。
//!    验不过就只发单 pid。
//! 2. **`--` 不能省**：procps-ng 的 `kill` 会把裸 `-<pgid>` 当成又一个信号选项吞掉，**退出码 0
//!    却什么都没发**（procps-ng 4.0.2 实测）。旧实现把这种“假成功”当组杀成功直接返回，
//!    于是单 pid 兜底永远不执行，`stop()` 只能干等超时——`stop_kills_process_tree` 与
//!    `pipeline_stop_records_killed_exit` 两条红灯的真正死因。
//!
//! ⚠️ 外部命令的退出码**不能**用来判断“进程是否已被结束”：温和阶段成功关掉进程后，
//! 再发 `taskkill /F` 会得到 128（“没有这个运行实例”）。所以 [`crate::launch::process`]
//! 的实际停止流程以“进程是否真的消失”为准（`try_wait` 轮询），本模块只提供发送动作。
//!
//! # 送达即登记（D10）
//!
//! 每次信号**真的送达**都记进 [`crate::launch::kill_registry`]，退出分析据此把结局判成
//! 「被终止」而不是「应用错误」。登记放在这一层，是因为所有结束路径都经过这里
//! （`RunningProcess::stop` / GUI 的 `host::launch::stop` / 看门狗兜底），调用点不必各记一遍。

use std::io;

use crate::platform::child::quiet;

/// 温和关闭（Windows 无 `/F`；POSIX `SIGTERM`）。
pub fn terminate(pid: u32) -> io::Result<()> {
    check_pid(pid)?;
    signal_and_note(pid, false)
}

/// 强制关闭（Windows `/F`；POSIX `SIGKILL`），并尽量覆盖整棵进程树。
pub fn force_kill(pid: u32) -> io::Result<()> {
    check_pid(pid)?;
    signal_and_note(pid, true)
}

/// 发信号，**送达后**登记「这个 pid 是我们结束的」（D10）。
///
/// 只在送达时登记：`taskkill` 对已退出的进程返回非 0、POSIX 的 `kill` 会报「没有送达」，
/// 那两种情况说明它自己先走了——不能把自然退出洗成「被结束」。
fn signal_and_note(pid: u32, force: bool) -> io::Result<()> {
    let result = imp::signal(pid, force);
    if result.is_ok() {
        crate::launch::kill_registry::note(pid);
    }
    result
}

/// 三段式：温和 → 宽限 → 强制。返回**强制阶段**的结果（温和阶段失败不致命）。
///
/// 注意：返回值不代表“进程已经死了”（见模块注释）。需要准确判定请用
/// [`crate::launch::process::RunningProcess::stop`]。
pub fn kill_tree(pid: u32, grace: std::time::Duration) -> io::Result<()> {
    check_pid(pid)?;
    let _ = terminate(pid);
    if !grace.is_zero() {
        std::thread::sleep(grace);
    }
    force_kill(pid)
}

/// `pid` 0 意味着“当前进程组”（POSIX `kill` 的 `-0`）——绝不能顺手发出去。
fn check_pid(pid: u32) -> io::Result<()> {
    if pid == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "无效的 pid 0：拒绝向当前进程组发信号",
        ));
    }
    Ok(())
}

/// 允许组杀的进程组号；`None` = 只能发单 pid。
///
/// 三重校验，任一不过就退回单 pid：
/// 1. 目标必须是它自己进程组的组长（`pgid == pid`）——否则 `-<pgid>` 指的不是“它的”子树；
/// 2. 必须能读到我们自己所在的进程组；
/// 3. 目标组不能就是我们所在的组——否则信号会打到自己人身上。
#[cfg(unix)]
pub fn group_kill_target(pid: u32) -> Option<u32> {
    if pid == 0 {
        return None;
    }
    let target = pgid_of(pid)?;
    if target != pid {
        return None;
    }
    let own = pgid_of(std::process::id())?;
    (target != own).then_some(target)
}

/// 读 `pid` 所在进程组号。Linux 走 `/proc`，其它 POSIX 用 `ps`；读不到就是 `None`（不发组杀）。
#[cfg(target_os = "linux")]
fn pgid_of(pid: u32) -> Option<u32> {
    // `/proc/<pid>/stat`: `pid (comm) state ppid pgrp ...`，`comm` 可能含空格和括号，
    // 所以从**最后一个** `)` 之后开始数字段。
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let tail = &stat[stat.rfind(')')? + 1..];
    let mut fields = tail.split_whitespace();
    fields.next()?; // state
    fields.next()?; // ppid
    fields.next()?.parse().ok() // pgrp
}

#[cfg(all(unix, not(target_os = "linux")))]
fn pgid_of(pid: u32) -> Option<u32> {
    let output = quiet("ps")
        .args(["-o", "pgid=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

#[cfg(windows)]
mod imp {
    use super::*;

    pub(super) fn signal(pid: u32, force: bool) -> io::Result<()> {
        // `CREATE_NO_WINDOW` 由 `platform::child::quiet` 统一带（否则 kill 子进程会闪一个黑框）。
        let mut command = quiet("taskkill");
        command.arg("/PID").arg(pid.to_string()).arg("/T");
        if force {
            command.arg("/F");
        }
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
        let mut delivered = false;
        // 验过的独立进程组：一次覆盖组长 + 它的全部后代。
        if let Some(pgid) = group_kill_target(pid) {
            let group = format!("-{pgid}");
            delivered |= run_kill(flag, &["--", &group])?;
        }
        // 单 pid 兜底永远执行：组杀不可用/进程已不在组里时靠它。
        let single = pid.to_string();
        delivered |= run_kill(flag, &[&single])?;
        if delivered {
            Ok(())
        } else {
            // 常见于“进程已经退出”，不值得中断关闭流程；但绝不谎报成功。
            Err(io::Error::other(format!(
                "kill {flag} {pid} 没有送达（进程可能已退出）"
            )))
        }
    }

    /// 发一发 `kill <flag> <args...>`，返回是否送达。
    ///
    /// `kill` 不存在时给**明确**错误而不是静默失败：否则调用方只会看到“进程仍在运行”，
    /// 排查方向会被带偏（M48 之前的红灯就是这么来的）。
    fn run_kill(flag: &str, args: &[&str]) -> io::Result<bool> {
        match quiet("kill")
            .arg(flag)
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
        {
            Ok(status) => Ok(status.success()),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Err(io::Error::new(
                io::ErrorKind::NotFound,
                "找不到 kill 命令：POSIX 进程树回收依赖 procps（Linux）或系统自带 kill（macOS/BSD）",
            )),
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
