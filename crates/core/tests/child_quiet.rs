//! M213：`platform::child::quiet` 的**行为**测试（它仍是一个普通 `Command`）。
//!
//! 抑制窗口最容易顺手搞坏的是「`.output()` 的管道、退出码、env、错误语义」，
//! 所以这一组用真程序把这四件事各钉一条。
//!
//! 源码扫描型守卫（所有 spawn 是否都经 `quiet`）在 `child_quiet_guard.rs`，
//! 与这里分开是为了让两边都留在 400 行以内，不靠压缩论证换行数。

use std::process::Command;

#[test]
fn quiet_is_a_plain_command_that_still_captures_output() {
    // 抑制窗口不能顺手把输出吞了：`.output()` 的管道与退出码必须照旧可用。
    let output = shell(&["echo hi"]).output().expect("spawn 失败");
    assert!(output.status.success(), "退出码：{}", output.status);
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "hi");
}

#[test]
fn quiet_keeps_stderr_and_nonzero_exit_code() {
    let output = shell(&["echo oops 1>&2", "exit 3"])
        .output()
        .expect("spawn 失败");
    assert_eq!(output.status.code(), Some(3), "退出码没透传");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("oops"),
        "stderr 没被捕获：{:?}",
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn quiet_keeps_env_and_args() {
    // 走 argv 与 env 两条通道，证明 `quiet` 没把 Command 包装成别的东西。
    let script = if cfg!(windows) {
        "echo %ULL_QUIET_PROBE%"
    } else {
        "echo $ULL_QUIET_PROBE"
    };
    let output = shell(&[script])
        .env("ULL_QUIET_PROBE", "m213")
        .output()
        .expect("spawn 失败");
    assert!(output.status.success(), "退出码：{}", output.status);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "m213",
        "参数/环境变量在构造器里丢了",
    );
}

#[test]
fn quiet_does_not_swallow_spawn_errors() {
    // 找不到程序时仍然是 Err：构造器不加任何「失败即静默」的逻辑。
    let error = unlucky_core::platform::child::quiet("ull-no-such-program-m213")
        .output()
        .expect_err("不存在的程序居然 spawn 成功了？");
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
}

/// 构造器的返回值必须能直接当 `std::process::Command` 用（下游 `run_with_timeout` 就是这么接的）。
#[test]
fn quiet_returns_a_std_command() {
    fn takes_command(_: Command) {}
    takes_command(unlucky_core::platform::child::quiet("ull-type-check-only"));
}

/// 走构造器起一个 shell，拼上本平台的「跑一段脚本」参数。
fn shell(scripts: &[&str]) -> Command {
    let (program, flag) = if cfg!(windows) {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    };
    let mut command = unlucky_core::platform::child::quiet(program);
    command.arg(flag);
    // Windows 的 `cmd /C` 只吃一条命令行；POSIX 的 `sh -c` 同理，所以用 `&` / `;` 串起来。
    let joined = if cfg!(windows) {
        scripts.join(" & ")
    } else {
        scripts.join("; ")
    };
    command.arg(joined);
    command
}
