//! 敏感键脱敏（P1-1 回归）：单键 get、无键全量 dump、set 的旧值回显三条路径
//! 都不能把凭据明文交给用户或脚本。

#[path = "support/mod.rs"]
mod support;

use support::{parse_json, run_cli, TempDir};

/// 一眼可 grep 的假凭据。
const SECRET: &str = "sk-live-THIS-MUST-NOT-LEAK";
/// 代理 URL 里的口令（`download.proxy_url` 是 schema 的 High 档，但键名不含敏感词）。
const PROXY: &str = "socks5://user:pw@127.0.0.1:1080";

/// 建一个已经写入敏感键的数据根。
fn data_dir_with_secrets() -> TempDir {
    let dir = TempDir::new("secret");
    let (code, _, stderr) = run_cli(&[
        "config",
        "set",
        "ai.api_key",
        SECRET,
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 0, "{stderr}");
    let (code, _, stderr) = run_cli(&[
        "config",
        "set",
        "download.proxy_url",
        PROXY,
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 0, "{stderr}");
    // 一个普通键：用来验证掩码没有把整份配置打成红条（默认值不落盘，必须先写一次）。
    let (code, _, stderr) = run_cli(&[
        "config",
        "set",
        "ui.language",
        "en_us",
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 0, "{stderr}");
    dir
}

#[test]
fn full_dump_redacts_sensitive_keys_by_default() {
    let dir = data_dir_with_secrets();
    let (code, stdout, stderr) = run_cli(&["config", "get", "--json", "--data-dir", &dir.arg()]);
    assert_eq!(code, 0, "{stderr}");
    assert!(!stdout.contains(SECRET), "全量 dump 泄出 API key：{stdout}");
    assert!(
        !stdout.contains("user:pw@"),
        "全量 dump 泄出代理口令：{stdout}"
    );

    let value = parse_json(&stdout);
    assert_eq!(value["data"]["config"]["ai"]["api_key"], "<redacted>");
    assert_eq!(
        value["data"]["config"]["download"]["proxy_url"],
        "<redacted>"
    );
    assert_eq!(value["data"]["redacted"], true);
    // 非敏感键照常可读（掩码不能把整份配置打成红条）。
    assert_eq!(value["data"]["config"]["ui"]["language"], "en_us");
}

#[test]
fn full_dump_human_output_is_redacted_too() {
    let dir = data_dir_with_secrets();
    let (code, stdout, _) = run_cli(&["config", "get", "--data-dir", &dir.arg()]);
    assert_eq!(code, 0);
    assert!(!stdout.contains(SECRET), "{stdout}");
    assert!(!stdout.contains("user:pw@"), "{stdout}");
    assert!(stdout.contains("<redacted>"), "{stdout}");
    assert!(stdout.contains("--show-secret"), "{stdout}");
}

#[test]
fn single_key_get_masks_by_default() {
    let dir = data_dir_with_secrets();
    let (code, stdout, _) = run_cli(&[
        "config",
        "get",
        "ai.api_key",
        "--json",
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 0);
    assert!(!stdout.contains(SECRET), "{stdout}");
    let value = parse_json(&stdout);
    assert_eq!(value["data"]["value"], "<redacted>");
    assert_eq!(value["data"]["sensitive"], true);
    assert_eq!(value["data"]["redacted"], true);
}

#[test]
fn show_secret_reveals_plaintext() {
    let dir = data_dir_with_secrets();
    // 全量 dump + --show-secret：明文，且 redacted=false。
    let (code, stdout, _) = run_cli(&[
        "config",
        "get",
        "--show-secret",
        "--json",
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 0);
    let value = parse_json(&stdout);
    assert_eq!(value["data"]["config"]["ai"]["api_key"], SECRET);
    assert_eq!(value["data"]["config"]["download"]["proxy_url"], PROXY);
    assert_eq!(value["data"]["redacted"], false);

    // 单键 + --show-secret：人类输出就是明文。
    let (code, stdout, _) = run_cli(&[
        "config",
        "get",
        "ai.api_key",
        "--show-secret",
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 0);
    assert!(stdout.contains(SECRET), "{stdout}");
}

#[test]
fn set_masks_old_value_but_echoes_new() {
    let dir = data_dir_with_secrets();

    let (code, stdout, stderr) = run_cli(&[
        "config",
        "set",
        "ai.api_key",
        "sk-new-value",
        "--json",
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert!(!stdout.contains(SECRET), "set 回显了旧密钥：{stdout}");
    let value = parse_json(&stdout);
    assert_eq!(value["data"]["old"], "<redacted>");
    assert_eq!(value["data"]["new"], "sk-new-value");

    // 人类模式同样不回显旧值明文（新值照常，用户刚敲的）。
    let (code, stdout, _) = run_cli(&[
        "config",
        "set",
        "ai.api_key",
        "sk-third",
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 0);
    assert!(
        !stdout.contains("sk-new-value"),
        "人类输出回显旧值：{stdout}"
    );
    assert!(stdout.contains("<redacted>"), "{stdout}");
    assert!(stdout.contains("sk-third"), "{stdout}");

    // 非敏感键仍然正常回显原值。
    let (code, stdout, _) = run_cli(&[
        "config",
        "set",
        "ui.language",
        "zh_cn",
        "--json",
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 0);
    assert_eq!(parse_json(&stdout)["data"]["old"], "en_us");
}
