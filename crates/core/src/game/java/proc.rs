//! 子进程的**带超时 / 可打断**执行（M175 r1 复审 P2-5）。
//!
//! `Command::output()` 是「等到底」：一份放在挂死网络盘上的 `java` 会让调用它的扫描线程永远不
//! 返回，界面上「取消」也就永远兑现不了。这里只做一件事——**要么拿到输出，要么把它杀掉**。

use std::time::Duration;

/// 单个候选的探测预算（`java -version` 正常几十毫秒；8 秒还没答话就是卡住了）。
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(8);

/// 轮询间隔（`try_wait` 的粒度）：够细，取消能在一帧内生效；够粗，不烧 CPU。
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// 杀掉并**回收**子进程：`kill()` 只是投递终止请求，`wait()` 才是回收。
///
/// 为什么两件必须一起做（M199 / r3 复审 P3-5）：只 `kill()` 不 `wait()`，Unix 上留下**僵尸**
/// （进程表项与 pid 都不释放），Windows 上句柄活到 `Child` 被 drop 为止、终止也只是异步的。
/// 抽成函数的理由：它是这条不变量的唯一落点，三个分支（超时 / 取消 / 出错）都走这里。
fn terminate(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// 跑一个命令并等它结束，**带超时与可打断**：`cancelled()` 为真或超时就把子进程 kill 掉。
///
/// 返回 `None` = 没起来 / 超时 / 被取消（三者对调用方是同一件事：这次探测没有结论）。
///
/// 为什么不用 `Command::output()`：它是「等到底」，一个卡住的子进程会把整个扫描线程钉死，
/// 而取消位只能在两次探测之间被看到——取消就永远兑现不了。
#[must_use]
pub fn run_with_timeout(
    mut command: std::process::Command,
    timeout: Duration,
    cancelled: &dyn Fn() -> bool,
) -> Option<std::process::Output> {
    use std::process::Stdio;

    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().ok()?;
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if cancelled() || started.elapsed() > timeout {
                    terminate(&mut child);
                    return None;
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(_) => {
                terminate(&mut child);
                return None;
            }
        }
    }
    child.wait_with_output().ok()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::run_with_timeout;

    #[test]
    fn a_hung_probe_is_killed_after_the_timeout() {
        // 一个「永远不返回」的子进程：不杀掉它，扫描线程就永远不结束（复审 P2-5）。
        let started = std::time::Instant::now();
        let output = run_with_timeout(
            sleeping_command(30),
            std::time::Duration::from_millis(400),
            &|| false,
        );
        assert!(output.is_none(), "超时必须判成「没有结论」");
        let elapsed = started.elapsed();
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "超时之后必须 kill 掉子进程并返回，实际等了 {elapsed:?}"
        );
    }

    #[test]
    fn a_cancel_interrupts_the_probe_that_is_already_running() {
        // 取消不能只在「两次探测之间」被看到：卡住的那一个也要能打断。
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let tripper = flag.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(150));
            tripper.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        let started = std::time::Instant::now();
        let output = run_with_timeout(
            sleeping_command(30),
            std::time::Duration::from_secs(30),
            &|| flag.load(std::sync::atomic::Ordering::SeqCst),
        );
        assert!(output.is_none(), "被取消的探测没有结论");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(10),
            "取消必须打断正在跑的那个探测，而不是等它自己结束"
        );
    }

    /// 超时之后子进程必须**真的死了**：让它延时写一个标记文件，断言那个文件从未出现。
    ///
    /// 为什么要有这一条（r2 复审 P3-1）：上面两条只断言「返回得快」，把 kill 那几行删掉它们照样绿
    /// ——而没杀掉的子进程会继续跑（写它自己的文件、占句柄）。
    ///
    /// 为什么还要一条「子进程真的起过」（M199 / r3 复审 P3-5）：`command.spawn().ok()?` 在命令
    /// 起不来时也返回 `None`，于是机器上没有 powershell 时本用例**空跑也照样绿**。现在先用同一条
    /// 命令（不延时）真跑一次，再要求超时那一跑也留下启动标记（里面有子进程自己的 pid）——起不来的
    /// 机器从此是红，不是静默通过。
    ///
    /// 时间账（免得慢机器上假红/假绿）：超时 3s、子进程尾部延时 4s、事后等待 4s。若 kill 没生效，
    /// 子进程在「起进程耗时 + 4s」写出存活标记，而这次检查在 3s + 4s = 7s；只要它起得来（上一步已
    /// 证明 3s 内起得来，否则启动标记也写不出来）就一定在检查之前写出来 ⇒ 红。反过来，若 kill 生效，
    /// 它在 3s 就被杀掉，4s 的尾巴根本走不到 ⇒ 绿。
    ///
    /// 变异证明：删掉 `terminate()` 里的 `child.kill()` ⇒ 子进程活到 4 秒、把存活标记写出来 ⇒ 红。
    #[test]
    fn a_timed_out_child_is_really_killed() {
        const TIMEOUT: Duration = Duration::from_secs(3);
        const TAIL: u32 = 4;
        const WAIT: Duration = Duration::from_secs(4);

        let dir = temp_dir("kill");
        let started = dir.join("started.txt");
        let survived = dir.join("survived.txt");

        // ① 子进程真的起得来吗（起不来的话下面全是空跑：`spawn().ok()?` 也给 `None`）。
        let preflight = marker_command(&started, &survived, 0)
            .output()
            .expect("子进程必须真的起得来：本机缺 powershell / sh 时本用例不许静默通过");
        assert!(
            preflight.status.success(),
            "标记命令自己失败了（stderr：{}）",
            String::from_utf8_lossy(&preflight.stderr)
        );
        assert!(
            started.exists() && survived.exists(),
            "标记命令没写出标记：它没真的跑过，后面的结论不成立"
        );
        std::fs::remove_file(&started).ok();
        std::fs::remove_file(&survived).ok();

        // ② 超时那一跑：被杀掉，且被杀之前它确实已经起来了。
        let output = run_with_timeout(marker_command(&started, &survived, TAIL), TIMEOUT, &|| {
            false
        });
        assert!(output.is_none(), "超时没有结论");
        assert!(
            started.exists(),
            "子进程没写出启动标记：这一跑是空跑（{TIMEOUT:?} 内没起来），用例不成立"
        );
        std::thread::sleep(WAIT);
        assert!(
            !survived.exists(),
            "子进程在超时后仍然活着并执行了尾巴（存活标记出现了）：kill 没生效"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 被 kill 之后必须**回收**（`wait()`）：只 `kill()` 不 `wait()` 会留下僵尸。
    ///
    /// 为什么只能在 Linux / Android 上守（M199）：僵尸是 Unix 的概念——进程被杀掉而父进程没
    /// `wait()` 时，它在进程表里留着、`/proc/<pid>` 仍然存在（状态 `Z`），pid 也不释放；`wait()`
    /// 之后进程表项立刻消失。Windows 没有这个可观测状态（终止 + 句柄释放之后没有残留），所以那条
    /// 平台上「少写一行 `wait()`」只能靠代码评审——**这是本用例覆盖不到的一半**，报告里要说清。
    ///
    /// 变异证明：只删 `terminate()` 里的 `child.wait()`（留着 `kill()`）⇒ pid 成僵尸 ⇒ 红；
    /// 把 `terminate()` 整个换成空函数（两条都删）⇒ 子进程还活着 ⇒ 红。
    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[test]
    fn a_killed_child_is_reaped_not_left_a_zombie() {
        let dir = temp_dir("reap");
        let started = dir.join("started.txt");
        let survived = dir.join("survived.txt");
        let output = run_with_timeout(
            marker_command(&started, &survived, 30),
            Duration::from_secs(2),
            &|| false,
        );
        assert!(output.is_none(), "超时没有结论");
        let pid: u32 = std::fs::read_to_string(&started)
            .expect("子进程没写出 pid：这一跑是空跑")
            .trim()
            .parse()
            .expect("启动标记里应当是子进程的 pid");
        // `wait()` 是同步回收：它返回时 `/proc/<pid>` 就该没了，不用等。
        match process_state(pid) {
            None => {}
            Some('Z') => panic!("kill 之后没有 wait 回收：pid {pid} 成了僵尸（进程表项还在）"),
            Some(state) => panic!("pid {pid} 还在（状态 {state}）：它没被杀掉，或没被回收"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `/proc/<pid>` 里的进程状态字符（`None` = 进程表里已经没有这一项）。
    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn process_state(pid: u32) -> Option<char> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        // `pid (comm) S …`：comm 里可能有空格和括号，只认**最后一个** `)` 之后的第一个字符。
        let rest = &stat[stat.rfind(')')? + 1..];
        rest.chars().find(|c| !c.is_whitespace())
    }

    /// 每个用例一个临时目录（pid + 纳秒：并行用例不会撞在一起）。
    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ull-proc-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("临时目录");
        dir
    }

    /// 「先写启动标记（子进程 pid）、延时 `seconds` 秒之后再写存活标记」的命令（两平台语义相同）。
    ///
    /// 启动标记里的 pid 同时证明两件事：**spawn 真的成功、进程真的跑起来了**（M199：`spawn().ok()?`
    /// 起不来时也是 `None`，本用例因此会空跑），以及「这个 pid 后来被回收了吗」。
    ///
    /// 两个标记的路径**走环境变量**：`cmd /C "…"` 会被 Rust 与 cmd 各加一层引号、把重定向目标截断
    /// （实测命令立刻失败 ⇒ 假红）；`sh` 用 `"$VAR"`、PowerShell 用 `$env:` 取，命令行上一个引号
    /// 都不出现，路径里有空格也不会被拆开。
    fn marker_command(
        started: &std::path::Path,
        survived: &std::path::Path,
        seconds: u32,
    ) -> std::process::Command {
        let mut command = if cfg!(windows) {
            let mut command = crate::platform::child::quiet("powershell");
            command.args([
                "-NoProfile",
                "-Command",
                &format!(
                    "Set-Content -Path $env:ULL_PROC_STARTED -Value $PID; \
                     Start-Sleep -Seconds {seconds}; \
                     Set-Content -Path $env:ULL_PROC_SURVIVED -Value alive"
                ),
            ]);
            command
        } else {
            let mut command = crate::platform::child::quiet("sh");
            command.args([
                "-c",
                &format!(
                    "echo $$ > \"$ULL_PROC_STARTED\"; sleep {seconds}; \
                     echo alive > \"$ULL_PROC_SURVIVED\""
                ),
            ]);
            command
        };
        command
            .env("ULL_PROC_STARTED", started)
            .env("ULL_PROC_SURVIVED", survived);
        command
    }

    /// 一个必然超时的命令（两个平台都有，且在「等很久」这件事上行为一致）。
    fn sleeping_command(seconds: u32) -> std::process::Command {
        if cfg!(windows) {
            let mut command = crate::platform::child::quiet("cmd");
            command.args(["/C", &format!("ping -n {seconds} 127.0.0.1 > NUL")]);
            command
        } else {
            let mut command = crate::platform::child::quiet("sleep");
            command.arg(seconds.to_string());
            command
        }
    }
}
