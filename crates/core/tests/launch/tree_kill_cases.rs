//! 进程树回收的安全钉子：组杀只打“目标自己的独立进程组”，绝不命中我们所在的组。
//!
//! 这些用例钉的是 M48 的两条根因：
//! - 子进程必须自成一个进程组（`process_group(0)`），否则 `kill -- -<pgid>` 没有可用的组；
//! - 组杀前必须验组（[`tree_kill::group_kill_target`]），否则 CI 容器里“全体进程 pgid=1”
//!   的那种环境会被一发组杀连测试进程一起带走。
//!
//! `/proc` 只有 Linux 有，进程组相关的用例都用 `#[cfg(target_os = "linux")]` 门住；
//! `pid == 0` 的守卫与平台无关，各平台都跑。

use std::time::Duration;

use unlucky_core::launch::tree_kill;

/// `pid` 0 在 POSIX `kill` 里表示“当前进程组”——绝不能顺手发出去。
#[test]
fn pid_zero_is_rejected() {
    assert!(tree_kill::terminate(0).is_err(), "terminate(0) 必须拒绝");
    assert!(tree_kill::force_kill(0).is_err(), "force_kill(0) 必须拒绝");
    assert!(
        tree_kill::kill_tree(0, Duration::ZERO).is_err(),
        "kill_tree(0) 必须拒绝"
    );
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::process::{Child, Command, Stdio};

    use unlucky_core::launch::tree_kill::group_kill_target;

    /// `/proc/<pid>/stat` 里 `)` 之后的字段（state ppid pgrp ...）。
    fn stat_tail(pid: u32) -> Option<Vec<String>> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let tail = stat.get(stat.rfind(')')? + 1..)?;
        Some(tail.split_whitespace().map(str::to_string).collect())
    }

    fn pgid(pid: u32) -> Option<u32> {
        stat_tail(pid)?.get(2)?.parse().ok()
    }

    /// 还活着（僵尸不算：已经死了只是没人 wait）。
    fn running(pid: u32) -> bool {
        matches!(stat_tail(pid).as_deref().and_then(|fields| fields.first().map(String::as_str)), Some(state) if state != "Z")
    }

    fn wait_gone(pid: u32, budget: Duration) -> bool {
        let deadline = std::time::Instant::now() + budget;
        while std::time::Instant::now() < deadline {
            if !running(pid) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        !running(pid)
    }

    /// 组长的子进程（`process_group(0)`）：自带一个组。
    fn spawn_grouped(script: &str) -> Child {
        use std::os::unix::process::CommandExt;
        Command::new("/bin/sh")
            .arg("-c")
            .arg(script)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .expect("spawn 应成功")
    }

    /// 和测试进程同组的子进程（不调 `process_group`）：组长不是它。
    fn spawn_in_our_group(script: &str) -> Child {
        Command::new("/bin/sh")
            .arg("-c")
            .arg(script)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn 应成功")
    }

    /// 测试失败时也别把 `sleep 30` 留在机器上。
    struct KillOnDrop(u32);

    impl Drop for KillOnDrop {
        fn drop(&mut self) {
            let _ = tree_kill::force_kill(self.0);
        }
    }

    /// 自己的进程组号：组杀安全的基准。
    fn self_pgid() -> u32 {
        pgid(std::process::id()).expect("/proc/self/stat 应可读")
    }

    #[test]
    fn group_kill_target_accepts_only_a_verified_own_group() {
        let own_pgid = self_pgid();
        // 我们自己所在的组永远不是合法目标（哪怕我们正好是组长）。
        assert_eq!(
            group_kill_target(std::process::id()),
            None,
            "自己的进程号绝不能成为组杀目标"
        );
        assert_eq!(
            group_kill_target(own_pgid),
            None,
            "自己的进程组绝不能成为组杀目标"
        );
        assert_eq!(group_kill_target(0), None, "pid 0 不是合法目标");

        // 同组的子进程：它不是组长，`-<pid>` 指向的不是“它的”组。
        let mut child = spawn_in_our_group("sleep 30");
        let pid = child.id();
        let _guard = KillOnDrop(pid);
        assert_eq!(pgid(pid), Some(own_pgid), "同组子进程应继承我们的 pgid");
        assert_eq!(
            group_kill_target(pid),
            None,
            "非组长子进程不能走组杀（`-<pid>` 会打到别人的组）"
        );

        // 独立组的子进程：合法的组杀目标。
        let mut grouped = spawn_grouped("sleep 30");
        let grouped_pid = grouped.id();
        let _grouped_guard = KillOnDrop(grouped_pid);
        assert_eq!(pgid(grouped_pid), Some(grouped_pid), "子进程应是组长");
        assert_eq!(
            group_kill_target(grouped_pid),
            Some(grouped_pid),
            "独立组的组长可以组杀"
        );

        let _ = child.kill();
        let _ = child.wait();
        let _ = grouped.kill();
        let _ = grouped.wait();
    }

    #[test]
    fn group_kill_reaps_grandchildren_without_touching_our_group() {
        let mut child = spawn_grouped("sleep 30 & sleep 30");
        let pid = child.id();
        let _guard = KillOnDrop(pid);
        std::thread::sleep(Duration::from_millis(300));

        let children_of = || -> Vec<u32> {
            std::fs::read_to_string(format!("/proc/{pid}/task/{pid}/children"))
                .unwrap_or_default()
                .split_whitespace()
                .filter_map(|value| value.parse().ok())
                .collect()
        };
        let grandchildren = children_of();
        assert!(
            !grandchildren.is_empty(),
            "shell 应当先 fork 出一个 sleep 子进程：{grandchildren:?}"
        );
        for grandchild in &grandchildren {
            assert_eq!(pgid(*grandchild), Some(pid), "后代必须落在子进程自己的组里");
        }

        tree_kill::terminate(pid).expect("组杀应送达");
        assert!(wait_gone(pid, Duration::from_secs(3)), "组长应被组杀带走");
        for grandchild in &grandchildren {
            assert!(
                wait_gone(*grandchild, Duration::from_secs(3)),
                "组杀应覆盖后代 {grandchild}"
            );
        }

        // 组杀没有溢出：我们自己还在原来的组里，而且没有被信号带走。
        assert_eq!(self_pgid(), pgid(std::process::id()).expect("仍然可读"));
        assert_ne!(pid, self_pgid(), "目标组和我们的组必须不同");

        let _ = child.wait();
    }

    #[test]
    fn terminate_still_kills_a_child_that_shares_our_group() {
        let own_pgid = self_pgid();
        let mut child = spawn_in_our_group("sleep 30");
        let pid = child.id();
        let _guard = KillOnDrop(pid);
        assert_eq!(pgid(pid), Some(own_pgid));

        tree_kill::terminate(pid).expect("单 pid 兜底应送达");
        assert!(
            wait_gone(pid, Duration::from_secs(3)),
            "同组子进程也必须能被单 pid TERM 干掉"
        );
        // 走到这里本身就说明“组杀没有命中自身进程组”——否则测试进程已被 TERM。
        assert_eq!(self_pgid(), pgid(std::process::id()).expect("仍然可读"));

        let _ = child.wait();
    }
}
