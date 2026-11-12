//! `ull install --loader liteloader` 的**离线**可验面：词表分流、父版本缺失、help 文案。
//!
//! 这里刻意不联网：LiteLoader 的版本表在 `dl.liteloader.com`，把「拉到表之后」的链路放在
//! `crates/core/tests/liteloader/**`（真实响应夹具 + 合成 + 计划 + 落盘 + 启动参数）。
//! 本文件守的是「进门那一步」：`optifine` 与随机拼法的错误码必须分开、父版本没装必须具名报错、
//! 非 LiteLoader 不许叠在别的加载器层上。

#[path = "support/mod.rs"]
mod support;

use support::{parse_json, run_cli, TempDir};

#[test]
fn optifine_is_recognized_but_not_supported() {
    let dir = TempDir::new("liteloader-optifine");
    let (code, stdout, _) = run_cli(&[
        "install",
        "1.20.1",
        "--loader",
        "optifine",
        "--json",
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 1);
    let value = parse_json(&stdout);
    assert_eq!(
        value["error"]["code"], "loader_unsupported",
        "optifine 是「认得但装不了」，不是未知加载器：{stdout}"
    );
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("optifine"),
        "{stdout}"
    );
}

/// `optifine` 之外，真正不认识的拼法仍是 `loader_unknown`（样本用**明显无效**的名字，
/// 不拿任何真实加载器名当「未知」前提）。
#[test]
fn genuinely_unknown_loader_stays_unknown() {
    let dir = TempDir::new("liteloader-unknown");
    let (code, stdout, _) = run_cli(&[
        "install",
        "1.20.1",
        "--loader",
        "ull-invalid-loader-test",
        "--json",
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 1);
    let value = parse_json(&stdout);
    assert_eq!(value["error"]["code"], "loader_unknown", "{stdout}");
    let hint = value["error"]["hint"].as_str().unwrap_or_default();
    assert!(hint.contains("liteloader"), "提示要列全六家：{hint}");
}

/// 父版本（`1.12.2-forge-…`）没装时：具名报缺哪个父版本，且**不发网络请求**。
#[test]
fn liteloader_on_a_missing_forge_parent_names_the_parent() {
    let dir = TempDir::new("liteloader-missing-parent");
    let (code, stdout, _) = run_cli(&[
        "install",
        "1.12.2-forge-14.23.5.2860",
        "--loader",
        "liteloader",
        "--json",
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 1);
    let value = parse_json(&stdout);
    assert_eq!(value["error"]["code"], "loader_not_found", "{stdout}");
    let message = value["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("1.12.2-forge-14.23.5.2860"),
        "要点名缺哪个父版本：{message}"
    );
}

/// 只有 LiteLoader 接受「叠在别的加载器层上」；别家在进门处就说清（不静默改成叠原版）。
#[test]
fn only_liteloader_accepts_stacking_on_another_loader_layer() {
    let dir = TempDir::new("liteloader-stack-guard");
    let (code, stdout, _) = run_cli(&[
        "install",
        "1.12.2-forge-14.23.5.2860",
        "--loader",
        "fabric",
        "--json",
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 1);
    let value = parse_json(&stdout);
    assert_eq!(value["error"]["code"], "usage", "{stdout}");
    let message = value["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("LiteLoader") && message.contains("Fabric"),
        "{message}"
    );
}

#[test]
fn install_help_names_the_liteloader_route() {
    let (code, stdout, _) = run_cli(&["help", "install"]);
    assert_eq!(code, 0);
    for needle in [
        "liteloader",
        "optifine",
        "launchwrapper",
        "tweakClass",
        "loader_unsupported",
        "loader_unknown",
    ] {
        assert!(
            stdout.contains(needle),
            "`ull help install` 少了 `{needle}`：\n{stdout}"
        );
    }
}

#[test]
fn general_help_lists_liteloader_under_install() {
    let (code, stdout, _) = run_cli(&["--help"]);
    assert_eq!(code, 0);
    let install_block = stdout
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("install <version>"))
        .take(3)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        install_block.contains("liteloader"),
        "总帮助的 install 段要提到 liteloader：\n{install_block}"
    );
}
