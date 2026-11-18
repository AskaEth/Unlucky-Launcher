//! 命令面：help 文案、用法错误、错误码——不碰数据或者只碰「拒绝」那条路。
//!
//! 从 `tests/adopt_test.rs` 拆出来只为守 400 行硬线；搬动是纯搬，断言未变。

use crate::support::{parse_json, run_cli, TempDir};

use crate::fixture::foreign_data_root;

/// 运行目录选项的用法错误：一律退出码 1 + `code=usage`（不是内部错误）。
///
/// 抓得住：`--isolation custom` 少 `--game-dir` 被默默当成缺省、`shared` 同时给 `--game-dir`
/// 被静默忽略、未知词被当缺省。
/// 抓不住：提示文案的措辞（只断言 code 与关键字）。
#[test]
fn isolation_option_misuse_is_a_usage_error() {
    let dir = TempDir::new("adopt-usage");
    foreign_data_root(dir.path());
    let cases: Vec<Vec<&str>> = vec![
        vec!["--isolation", "custom"],
        vec!["--isolation", "shared", "--game-dir", "C:/tmp/x"],
        vec!["--isolation", "nope"],
        vec!["--game-dir", "relative/dir"],
    ];
    for extra in cases {
        let mut args = vec!["instances", "adopt", "1.20.1", "--json", "--data-dir"];
        let dir_arg = dir.arg();
        args.push(&dir_arg);
        args.extend(extra.iter().copied());
        let (code, stdout, stderr) = run_cli(&args);
        assert_eq!(code, 1, "args={args:?}\nstdout={stdout}\nstderr={stderr}");
        let value = parse_json(&stdout);
        assert_eq!(
            value["error"]["code"], "usage",
            "args={args:?} 应是用法错误：{stdout}"
        );
    }
}

/// 不存在的版本 ⇒ `adopt_version_not_found`（不联网、不偷偷装一个）。
///
/// 抓得住：不存在的 id 被登记成功。
/// 抓不住：`versions/` 整个不存在时 `scan` 的空结果（core 侧用例覆盖）。
#[test]
fn unknown_version_reports_not_found() {
    let dir = TempDir::new("adopt-unknown");
    foreign_data_root(dir.path());
    let (code, stdout, _) = run_cli(&[
        "instances",
        "adopt",
        "9.9.9",
        "--json",
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 1, "{stdout}");
    assert_eq!(
        parse_json(&stdout)["error"]["code"],
        "adopt_version_not_found"
    );
}

/// help 里必须有这两条命令（新命令不留「有实现没文档」的状态），且旧命令仍在。
///
/// 抓得住：help 漏列 `instances scan` / `instances adopt`（子串断言），或把旧的
/// `instances launch-args` / `pack import` 弄丢。
/// 抓不住：帮助文案与真实行为是否一致（那要靠上面的行为用例 + 人读）。
#[test]
fn help_lists_the_new_subcommands_next_to_the_old_ones() {
    let general = unlucky_cli::cli::help::general();
    for needle in [
        "instances scan",
        "instances adopt",
        "instances list",
        "install <version>",
    ] {
        assert!(general.contains(needle), "总帮助缺 `{needle}`：\n{general}");
    }
    let topic = unlucky_cli::cli::help::topic("instances").expect("help instances 存在");
    for needle in [
        "instances scan",
        "instances adopt",
        "缺省不沿用全局 isolated",
        "一个字节都不写",
    ] {
        assert!(topic.contains(needle), "help instances 缺 `{needle}`");
    }
    // 旧的 `per_version` 字面量只在 core 的 `Isolation::parse` 里被兼容，CLI 词表不接受它。
    let bogus = std::env::temp_dir().join("ull-adopt-help-nonexistent");
    let bogus = bogus.display().to_string();
    let args = [
        "instances",
        "adopt",
        "1.20.1",
        "--isolation",
        "per_version",
        "--json",
        "--data-dir",
        bogus.as_str(),
    ];
    let (code, stdout, _) = run_cli(&args);
    assert_eq!(code, 1, "`per_version` 不是 CLI 词表：{stdout}");
}

/// **更高祖先断链**（r2 R2.3 的 CLI 面）：`--data-dir` 指到迁移联接下面的 nested 目录，
/// 联接目标被搬走后 `instances scan` 必须非零 + 结构化错误，不能 `ok:true / total:0`。
///
/// 抓得住：只判 `versions`/游戏根两层（更高层断链会漏，这里会看到 exit0 与 total 0）。
///
/// 抓不住：真正的权限不足（EACCES）那一支。
#[cfg(windows)]
#[test]
fn a_dangling_ancestor_is_a_structured_scan_error() {
    let dir = TempDir::new("scan-ancestor-dangling");
    let outer = TempDir::new("scan-ancestor-target");
    let real = outer.path();
    let game = real.join("nested").join(".minecraft");
    std::fs::create_dir_all(game.join("versions").join("1.20.1")).expect("建完整版本树");
    std::fs::write(
        game.join("versions/1.20.1/1.20.1.json"),
        br#"{"id":"1.20.1","type":"release","mainClass":"x","libraries":[]}"#,
    )
    .expect("写清单");
    let link = dir.path().join("game-parent-link");
    let ok = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&link)
        .arg(real)
        .output()
        .is_ok_and(|out| out.status.success());
    if !ok {
        eprintln!("[link-guard] 建不出目录联接，跳过祖先断链 CLI 用例");
        return;
    }
    let data_arg = link.join("nested").display().to_string();

    // 断开之前：能扫到 1 个候选。
    let (code, stdout, stderr) = run_cli(&["instances", "scan", "--json", "--data-dir", &data_arg]);
    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");
    assert_eq!(parse_json(&stdout)["data"]["counts"]["total"], 1);

    // 断开：联接目标改名（入口还在，versions 与游戏根都 NotFound）。
    let moved = dir.path().join("moved-target");
    std::fs::rename(real, &moved).expect("移走联接目标");

    let (code, stdout, stderr) = run_cli(&["instances", "scan", "--json", "--data-dir", &data_arg]);
    assert_eq!(
        code, 1,
        "祖先断链必须非零退出：stdout={stdout} stderr={stderr}"
    );
    let value = parse_json(&stdout);
    assert_eq!(value["ok"], false, "{stdout}");
    assert_eq!(
        value["error"]["code"], "adopt_versions_unreadable",
        "{stdout}"
    );
}
