//! M220：`ull launch` 真正 spawn 的 Java 必须是 D4 校验过的那个。
//!
//! 钉路径的**决策**是纯函数（`commands::java_launch::plan_java`，那边有五种来源的单测）；
//! 本文件补**端到端**：装配之后命令行里那个 java 也得是同一个。
//!
//! 手法：把 `JAVA_HOME` 指到一个**没有 `bin/java` 的空目录**，让「装配期按环境兜底」
//! 与「D4 选中的候选」必然不同——兜底会退回裸名字 `java[.exe]`（相对名，靠 `PATH` 找），
//! 而 D4 选中的是探出来的绝对路径。不动 `JAVA_HOME` 的话兜底恰好也落在 `JAVA_HOME`
//! 那一个文件上，「只钉 Managed」的老实现也能蒙对，正是复审抓到的那个缺陷。
//!
//! 用真子进程（`CARGO_BIN_EXE_ull` + `Command::env`）而不是 `support::run_cli`：
//! `JAVA_HOME` 要按进程注入，改本进程的环境会与同二进制里的其它用例抢。

#[path = "support/mod.rs"]
mod support;

use std::path::Path;
use std::process::Command;

use support::{parse_json, write_version, TempDir};

/// 一个可组装的版本 id：静态表要求 **Java 21+**。
///
/// 选 21+ 是有意的：「托管区里只有 17（距离 4）、环境/系统上有 21/25（距离 0）」，
/// 按 D4 的版本距离优先，选中的必然是托管的**外面**那个——正是复审抓到的
/// 「只有 managed 会钉路径」缺陷所在的那条分支。要求 17 的话托管 17 会赢，
/// 老实现也能蒙对，用例就白跑了。
const VERSION: &str = "1.21";

/// 跑一次 `ull --data-dir <root> <tail...>`，`JAVA_HOME` 注入成 `java_home`。
///
/// `json` 决定加不加全局 `--json`（加了就只有 JSON envelope，人读行被收起来）。
fn cli_with_java_home(
    dir: &TempDir,
    java_home: &Path,
    json: bool,
    tail: &[&str],
) -> (i32, String, String) {
    let mut args = vec!["--data-dir".to_string(), dir.arg()];
    if json {
        args.push("--json".to_string());
    }
    args.extend(tail.iter().map(|arg| arg.to_string()));
    let output = Command::new(env!("CARGO_BIN_EXE_ull"))
        .args(&args)
        .env("JAVA_HOME", java_home)
        .output()
        .expect("应能起 ull");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn the_command_line_runs_the_java_that_d4_validated() {
    let dir = TempDir::new("java-pin");
    write_version(dir.path(), VERSION);
    // 空 JAVA_HOME：`JAVA_HOME/bin/java` 不存在 → 环境兜底这条路走不通。
    let empty_home = dir.path().join("empty-java-home");
    std::fs::create_dir_all(&empty_home).expect("建空 JAVA_HOME");

    let (code, stdout, stderr) =
        cli_with_java_home(&dir, &empty_home, true, &["launch", VERSION, "--dry-run"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let data = parse_json(&stdout)["data"].clone();
    let java = &data["java"];
    let source = java["source"].as_str().unwrap_or("none");
    assert!(
        ["PATH", "system"].contains(&source),
        "前置：`JAVA_HOME` 已置空，且这条版本要 Java 21+（托管区的 17 不满足），\
         来源只该是 PATH 或标准安装位置：{data}"
    );
    let binary = java["binary"]
        .as_str()
        .unwrap_or_else(|| panic!("本机一个 Java 都探不到，这条端到端验不到东西（判红）：{data}"));
    assert!(
        Path::new(binary).is_absolute(),
        "前置：探到的 Java 必须是绝对路径（否则「钉住」与「裸名字」无从区分，判红）：{data}"
    );

    // ① 装配落地的就是校验过的那个（`--json` 里并排给的两个事实必须相等）。
    assert_eq!(
        java["launch_binary"].as_str(),
        Some(binary),
        "装配落地的 java 必须是 D4 校验过的那个：{data}"
    );
    // ② 命令行里真正要跑的也是它 —— 这一条才是「spawn 的是哪个」。
    //    注意 `program` 会过脱敏（主目录 → `<home>`），所以按「绝对路径 + 文件名」判，
    //    不按整串判；退回裸 `java[.exe]` 的老实现在这里必红。
    let program = data["program"].as_str().unwrap_or_default();
    assert!(
        program.starts_with("<home>") || Path::new(program).is_absolute(),
        "命令行里的 java 必须是钉住的绝对路径（裸 `java` = 又退回装配期重选了）：{data}"
    );
    assert_eq!(
        Path::new(program).file_name(),
        Path::new(binary).file_name(),
        "命令行里的 java 与 D4 校验的不是同一个：{data}"
    );
    // ③ 人读输出也要说清「实际用哪个、来源是什么」（终端里对着日志排查时只有一个依据）。
    let (code, stdout, stderr) =
        cli_with_java_home(&dir, &empty_home, false, &["launch", VERSION, "--dry-run"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.contains(&format!("{source} · ")),
        "人读行要写出 Java 来源：{stdout}"
    );
    assert!(
        stdout.contains(binary),
        "人读行要写出实际使用的 java 路径（{binary}）：{stdout}"
    );
}
