//! `ull accounts` 与 `ull launch --account` 的端到端断言。
//!
//! 职责：证明「账户能建/能删/能选」，并且**凭据红线守住了**——
//! stdout（含 `--json`）、`launch --dry-run` 的 argv 与身份块里都不出现 token 明文。

#[path = "support/mod.rs"]
mod support;

use std::path::{Path, PathBuf};

use support::{parse_json, run_cli, TempDir};
use unlucky_core::auth::offline::offline_uuid;

const VERSION: &str = "1.20.1";

/// 建一个带「版本本体 + 一个实例」的数据根。
fn data_root(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    support::write_version(dir.path(), VERSION);
    support::write_profiles(dir.path(), &[("生存服", VERSION)]);
    dir
}

/// 覆盖版本 JSON：把 `${auth_access_token}` / `${auth_uuid}` 塞进命令行，
/// 这样「脱敏」才是被真的跑到，而不是「压根没有 token 所以没有明文」。
fn write_version_with_auth(root: &Path) {
    let dir = root.join(".minecraft").join("versions").join(VERSION);
    std::fs::create_dir_all(&dir).expect("建版本目录");
    let version = serde_json::json!({
        "id": VERSION,
        "type": "release",
        "mainClass": "net.minecraft.client.main.Main",
        "assets": "5",
        "assetIndex": { "id": "5" },
        "arguments": {
            "game": [
                "--username", "${auth_player_name}",
                "--uuid", "${auth_uuid}",
                "--accessToken", "${auth_access_token}",
                "--userType", "${auth_user_type}",
                "--version", "${version_name}"
            ],
            "jvm": ["-Djava.library.path=${natives_directory}", "-cp", "${classpath}"]
        },
        "libraries": [],
        "downloads": { "client": { "size": 4, "url": "http://example.invalid/client.jar" } }
    });
    std::fs::write(
        dir.join(format!("{VERSION}.json")),
        serde_json::to_vec_pretty(&version).expect("序列化版本 JSON"),
    )
    .expect("写版本 JSON");
}

fn auth_dir(root: &Path) -> PathBuf {
    root.join(".ULL").join("data").join("auth")
}

/// 拼一条 `--json --data-dir <root> ...` 命令。
fn cli(dir: &TempDir, tail: &[&str]) -> Vec<String> {
    let mut args = vec!["--json".to_string(), "--data-dir".to_string(), dir.arg()];
    args.extend(tail.iter().map(|item| (*item).to_string()));
    args
}

/// 拼一条 `launch 生存服 --dry-run ...` 命令。
fn launch(dir: &TempDir, extra: &[&str]) -> Vec<String> {
    let mut tail = vec!["launch", "生存服", "--dry-run"];
    tail.extend_from_slice(extra);
    cli(dir, &tail)
}

/// 跑一条命令并断言退出码。
fn run(args: &[String], expected: i32) -> (String, String) {
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let (code, out, err) = run_cli(&refs);
    assert_eq!(code, expected, "args={refs:?}\nstdout={out}\nstderr={err}");
    (out, err)
}

#[test]
fn accounts_list_is_empty_at_first() {
    let dir = data_root("accounts-empty");
    let (out, _) = run(&cli(&dir, &["accounts", "list"]), 0);
    let envelope = parse_json(&out);
    assert_eq!(envelope["data"]["count"], 0);
    assert_eq!(
        envelope["data"]["accounts"].as_array().map(Vec::len),
        Some(0)
    );
    assert!(!auth_dir(dir.path()).join("accounts.json").exists());
}

#[test]
fn add_offline_writes_both_files_and_reports_hmcl_uuid() {
    let dir = data_root("accounts-add");
    let (out, _) = run(&cli(&dir, &["accounts", "add", "offline", "小明"]), 0);
    let envelope = parse_json(&out);
    let account = &envelope["data"]["account"];
    assert_eq!(account["name"], "小明");
    assert_eq!(account["kind"], "offline");
    assert_eq!(account["profile_id"], offline_uuid("小明"));

    let accounts_file = auth_dir(dir.path()).join("accounts.json");
    let credentials_file = auth_dir(dir.path()).join("credentials.dat");
    assert!(accounts_file.is_file(), "元数据文件必须落盘");
    assert!(
        credentials_file.is_file(),
        "凭据文件必须落盘（哪怕离线轨是空的）"
    );

    // list 里能看到，且输出里没有任何凭据字段。
    let (list_out, _) = run(&cli(&dir, &["accounts", "list"]), 0);
    let list = parse_json(&list_out);
    assert_eq!(list["data"]["count"], 1);
    assert_eq!(list["data"]["accounts"][0]["name"], "小明");
    for forbidden in ["access_token", "refresh_token", "client_token", "password"] {
        assert!(
            !list_out.contains(forbidden),
            "accounts list 不该出现 {forbidden}"
        );
    }
}

#[test]
fn duplicate_offline_account_is_rejected() {
    let dir = data_root("accounts-dup");
    run(&cli(&dir, &["accounts", "add", "offline", "Steve"]), 0);
    let (out, _) = run(&cli(&dir, &["accounts", "add", "offline", "steve"]), 1);
    assert_eq!(parse_json(&out)["error"]["code"], "conflict");
}

#[test]
fn add_offline_requires_a_valid_name() {
    let dir = data_root("accounts-badname");
    let (out, _) = run(&cli(&dir, &["accounts", "add", "offline"]), 1);
    assert_eq!(parse_json(&out)["error"]["code"], "usage");

    let (out, _) = run(
        &cli(&dir, &["accounts", "add", "offline", "0123456789abcdefg"]),
        1,
    );
    assert_eq!(parse_json(&out)["error"]["code"], "malformed");
}

#[test]
fn remove_deletes_account_and_reports_missing() {
    let dir = data_root("accounts-remove");
    run(&cli(&dir, &["accounts", "add", "offline", "Steve"]), 0);
    let (out, _) = run(&cli(&dir, &["accounts", "remove", "Steve"]), 0);
    assert_eq!(parse_json(&out)["data"]["removed"]["name"], "Steve");

    let (out, _) = run(&cli(&dir, &["accounts", "list"]), 0);
    assert_eq!(parse_json(&out)["data"]["count"], 0);

    let (out, _) = run(&cli(&dir, &["accounts", "remove", "Steve"]), 1);
    assert_eq!(parse_json(&out)["error"]["code"], "not_found");
}

#[test]
fn add_msa_without_client_id_is_explained() {
    // 没配 ULL_MSA_CLIENT_ID 时不该去联网，而是明确告知要配什么。
    let saved = std::env::var("ULL_MSA_CLIENT_ID").ok();
    std::env::remove_var("ULL_MSA_CLIENT_ID");
    let dir = data_root("accounts-msa");
    let (out, _) = run(&cli(&dir, &["accounts", "add", "msa"]), 1);
    if let Some(value) = saved {
        std::env::set_var("ULL_MSA_CLIENT_ID", value);
    }
    let envelope = parse_json(&out);
    assert_eq!(envelope["error"]["code"], "unsupported");
    assert!(envelope["error"]["hint"]
        .as_str()
        .unwrap_or_default()
        .contains("ULL_MSA_CLIENT_ID"));
}

#[test]
fn launch_dry_run_uses_the_only_account() {
    let dir = data_root("launch-account");
    write_version_with_auth(dir.path());
    run(&cli(&dir, &["accounts", "add", "offline", "Steve"]), 0);
    let (out, _) = run(&launch(&dir, &[]), 0);
    let data = &parse_json(&out)["data"];
    let account = &data["account"];
    assert_eq!(account["name"], "Steve");
    assert_eq!(account["kind"], "offline");
    assert_eq!(account["profile_id"], offline_uuid("Steve"));
    assert_eq!(account["registered"], true);

    // argv 里玩家名与 UUID 都对上了，token 位置是脱敏占位。
    let argv: Vec<String> = data["argv"]
        .as_array()
        .expect("argv")
        .iter()
        .map(|item| item.as_str().unwrap_or_default().to_string())
        .collect();
    let username_at = argv
        .iter()
        .position(|arg| arg == "--username")
        .expect("有 --username");
    assert_eq!(argv[username_at + 1], "Steve");
    // launch 的脱敏器把 uuid 也当敏感值处理（M18 定案：dry-run 输出是「脱敏 uuid」），
    // 真实 uuid 在下面的身份块里以可公开元数据的形式给出。
    let uuid_at = argv
        .iter()
        .position(|arg| arg == "--uuid")
        .expect("有 --uuid");
    assert_eq!(argv[uuid_at + 1], "<redacted:token>");
    // 真凭据位置：离线占位 token 是随机 32 hex，输出里必须只剩脱敏占位。
    let token_at = argv
        .iter()
        .position(|arg| arg == "--accessToken")
        .expect("有 --accessToken");
    assert_eq!(argv[token_at + 1], "<redacted:token>");
    assert!(out.contains("<redacted:token>"));
    // 命令行与展示行里不该有任何 32 位十六进制串（= 没漏出 accessToken）。
    let display = data["display"].as_str().unwrap_or_default();
    assert!(!has_hex32(&argv.join(" ")), "argv 漏了 token：{argv:?}");
    assert!(!has_hex32(display), "display 漏了 token：{display}");
    assert!(
        !out.contains("access_token"),
        "输出不该出现 access_token 字段"
    );
}

/// 输出里是否有 32 位十六进制串（token 形状）。
fn has_hex32(text: &str) -> bool {
    text.split(|ch: char| !ch.is_ascii_hexdigit())
        .any(|run| run.len() == 32)
}

#[test]
fn multiple_accounts_require_explicit_choice() {
    let dir = data_root("launch-multi");
    run(&cli(&dir, &["accounts", "add", "offline", "Steve"]), 0);
    run(&cli(&dir, &["accounts", "add", "offline", "Alex"]), 0);
    let (out, _) = run(&launch(&dir, &[]), 1);
    let envelope = parse_json(&out);
    assert_eq!(envelope["error"]["code"], "account_ambiguous");
    assert!(
        envelope["error"]["hint"]
            .as_str()
            .unwrap_or_default()
            .contains("Steve"),
        "hint 要列候选"
    );

    // 指定名字就能选出来（大小写不敏感）。
    let (out, _) = run(&launch(&dir, &["--account", "alex"]), 0);
    let data = &parse_json(&out)["data"];
    assert_eq!(data["account"]["name"], "Alex");
    assert_eq!(data["account"]["profile_id"], offline_uuid("Alex"));
}

#[test]
fn unknown_account_and_username_conflict_are_usage_errors() {
    let dir = data_root("launch-unknown");
    run(&cli(&dir, &["accounts", "add", "offline", "Steve"]), 0);
    let (out, _) = run(&launch(&dir, &["--account", "Nobody"]), 1);
    assert_eq!(parse_json(&out)["error"]["code"], "not_found");

    let (out, _) = run(&launch(&dir, &["--username", "Alex"]), 1);
    assert_eq!(parse_json(&out)["error"]["code"], "usage");
}

#[test]
fn launch_without_accounts_keeps_the_offline_placeholder() {
    let dir = data_root("launch-placeholder");
    let (out, _) = run(&launch(&dir, &["--username", "Node"]), 0);
    let data = &parse_json(&out)["data"];
    assert_eq!(data["account"]["registered"], false);
    assert_eq!(data["account"]["kind"], "offline-placeholder");
    assert_eq!(data["account"]["name"], "Node");
    let argv: Vec<&str> = data["argv"]
        .as_array()
        .expect("argv")
        .iter()
        .map(|item| item.as_str().unwrap_or_default())
        .collect();
    let at = argv
        .iter()
        .position(|arg| *arg == "--username")
        .expect("--username");
    assert_eq!(argv[at + 1], "Node");
}

#[test]
fn accounts_help_topic_exists() {
    let (out, _) = run(&["help".to_string(), "accounts".to_string()], 0);
    assert!(out.contains("credentials.dat"), "{out}");
    assert!(out.contains("accounts add offline"), "{out}");
}
