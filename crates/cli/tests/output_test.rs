//! `--json` 输出形状与真实 core 轻量路径（只读命令 + dry-run）。

#[path = "support/mod.rs"]
mod support;

use support::{parse_json, run_cli, write_profiles, write_session, write_version, TempDir};

#[test]
fn version_json_has_os_summary() {
    let (code, stdout, _) = run_cli(&["--version", "--json"]);
    assert_eq!(code, 0);
    let value = parse_json(&stdout);
    assert_eq!(value["ok"], true);
    // M82：自称的名字跟着被调用的二进制走（test harness 里是测试二进制名，`ull` 里是 `ull`）；
    // argv[0] → 名字的映射由 cli::program 的单测钉死。
    assert_eq!(
        value["data"]["name"],
        unlucky_cli::cli::program::name(),
        "{stdout}"
    );
    assert_eq!(value["data"]["version"], env!("CARGO_PKG_VERSION"));
    // D19/D21：os 对象来自 core::platform::os_info（family + 两档 support）。
    assert!(value["data"]["os"]["family"].is_string());
    let support = value["data"]["os"]["support"].as_str().unwrap_or_default();
    assert!(matches!(support, "one" | "unsupported"), "{support}");
}

#[test]
fn welcome_without_subcommand_prints_version_and_help() {
    let (code, stdout, _) = run_cli(&[]);
    assert_eq!(code, 0);
    // banner 用被调用的二进制名（M82），不写死 `ull`。
    let banner = format!("{} ", unlucky_cli::cli::program::name());
    assert!(stdout.starts_with(&banner), "{stdout}");
    assert!(stdout.contains("用法:"), "{stdout}");

    let (code, stdout, _) = run_cli(&["--json"]);
    assert_eq!(code, 0);
    let value = parse_json(&stdout);
    assert!(value["data"]["help"]
        .as_str()
        .unwrap_or_default()
        .contains("用法:"));
}

#[test]
fn instances_list_json_shape() {
    let dir = TempDir::new("instances");
    write_version(dir.path(), "1.20.1");
    write_profiles(dir.path(), &[("生存服", "1.20.1"), ("空实例", "1.19.4")]);

    let (code, stdout, _) = run_cli(&["instances", "list", "--json", "--data-dir", &dir.arg()]);
    assert_eq!(code, 0);
    let value = parse_json(&stdout);
    let instances = value["data"]["instances"]
        .as_array()
        .expect("instances 数组");
    assert_eq!(instances.len(), 2);
    assert_eq!(instances[0]["name"], "生存服");
    assert_eq!(instances[0]["version_present"], true);
    assert_eq!(instances[1]["version_present"], false);
    let installed = value["data"]["installed_versions"]
        .as_array()
        .expect("已安装版本");
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0], "1.20.1");
}

#[test]
fn install_rejects_unknown_loader_before_touching_network() {
    let dir = TempDir::new("install-loader");
    // 样本必须是一个**任何词表里都没有**的拼法：`liteloader` 自 M222 起是受支持加载器，
    // 拿它当「未知」会让这条用例变成真去拉 dl.liteloader.com 的联网测试。
    let (code, stdout, _) = run_cli(&[
        "install",
        "1.20.1",
        "--loader",
        "ull-invalid-loader-test",
        "--json",
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 1, "未知加载器是用户错误");
    let value = parse_json(&stdout);
    assert_eq!(value["error"]["code"], "loader_unknown");
    assert!(
        !dir.path()
            .join(".ULL")
            .join("config")
            .join("profiles.json")
            .exists(),
        "拒绝时不该登记任何实例"
    );
}

#[test]
fn install_reports_name_conflict_without_downloading() {
    let dir = TempDir::new("install-conflict");
    write_profiles(dir.path(), &[("生存服", "1.19.4")]);

    // 同名但绑定别的版本 → 冲突，且这一步在联网/下载之前就返回。
    let (code, stdout, _) = run_cli(&[
        "install",
        "1.20.1",
        "--name",
        "生存服",
        "--json",
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 1);
    let value = parse_json(&stdout);
    assert_eq!(value["error"]["code"], "instance_exists");
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("1.19.4"),
        "错误里应说明冲突版本：{stdout}"
    );
}

#[test]
fn install_requires_a_version_id() {
    let dir = TempDir::new("install-empty");
    let (code, _, stderr) = run_cli(&["install", " ", "--data-dir", &dir.arg()]);
    assert_eq!(code, 1);
    assert!(stderr.contains("版本 id"), "stderr={stderr}");
}

#[test]
fn launch_dry_run_prints_assembled_command() {
    let dir = TempDir::new("dryrun");
    write_version(dir.path(), "1.20.1");
    write_profiles(dir.path(), &[("生存服", "1.20.1")]);

    let (code, stdout, _) = run_cli(&[
        "launch",
        "生存服",
        "--dry-run",
        "--username",
        "Steve",
        "--max-memory",
        "2048",
        "--json",
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 0, "{stdout}");
    let value = parse_json(&stdout);
    let data = &value["data"];
    assert_eq!(data["dry_run"], true);
    assert_eq!(data["instance_found"], true);
    let argv = data["argv"].as_array().expect("argv 数组");
    let joined = argv
        .iter()
        .filter_map(|item| item.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(joined.contains("-Xmx2048"), "{joined}");
    assert!(
        joined.contains("net.minecraft.client.main.Main"),
        "{joined}"
    );
    assert!(joined.contains("--username Steve"), "{joined}");
    // 凭据绝不落到 dry-run 输出里。
    assert!(!joined.contains("accessToken "), "{joined}");
    assert!(data["classpath_entries"].as_u64().unwrap_or(0) >= 1);
}

#[test]
fn sessions_logs_and_diag_read_the_index() {
    let dir = TempDir::new("sessions");
    let sid = write_session(dir.path());

    let (code, stdout, _) = run_cli(&["sessions", "list", "--json", "--data-dir", &dir.arg()]);
    assert_eq!(code, 0);
    let value = parse_json(&stdout);
    let sessions = value["data"]["sessions"].as_array().expect("sessions 数组");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["session_id"], sid);
    assert_eq!(sessions[0]["exit_code"], 1);

    let (code, stdout, _) = run_cli(&[
        "logs",
        &sid,
        "--tail",
        "1",
        "--json",
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 0);
    let value = parse_json(&stdout);
    let events = value["data"]["events"].as_array().expect("events 数组");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["kind"], "stderr");

    // 前缀匹配唯一即可（脚本里手打全 id 太长）。
    let (code, stdout, _) = run_cli(&["diag", "20261030", "--json", "--data-dir", &dir.arg()]);
    assert_eq!(code, 0, "{stdout}");
    let value = parse_json(&stdout);
    assert_eq!(value["data"]["session_id"], sid);
    let candidates = value["data"]["diagnosis"]["candidates"]
        .as_array()
        .expect("candidates 数组");
    assert!(!candidates.is_empty(), "OOM 日志应命中规则：{stdout}");
    assert_eq!(value["data"]["diagnosis"]["needs_more_info"], false);
}

#[test]
fn logs_json_payload_is_redacted_like_human_output() {
    // P2-1：`--json` 是机器消费路径，不该比人类输出拿到更多原文（主目录用户名段）。
    let dir = TempDir::new("logs-redact");
    let sid = write_session(dir.path());
    let events = dir
        .path()
        .join(".ULL")
        .join("data")
        .join("agent")
        .join("sessions")
        .join("game")
        .join(format!("{sid}.jsonl"));
    let extra = serde_json::json!({
        "v": 1, "seq": 9, "ts": "2026-10-30T10:15:07Z", "t_ms": 6000,
        "kind": "stdout", "session_id": sid,
        "payload": { "line": r"cat C:\Users\Bob\secret.txt failed", "level": "info" }
    });
    let mut text = std::fs::read_to_string(&events).expect("读事件流");
    text.push_str(&serde_json::to_string(&extra).expect("序列化事件"));
    text.push('\n');
    std::fs::write(&events, text).expect("写事件流");

    let (code, stdout, stderr) = run_cli(&["logs", &sid, "--json", "--data-dir", &dir.arg()]);
    assert_eq!(code, 0, "{stderr}");
    assert!(!stdout.contains("Bob"), "JSON 载荷泄出用户名：{stdout}");
    assert!(stdout.contains("***"), "JSON 载荷未脱敏：{stdout}");
}

#[test]
fn config_get_set_round_trip() {
    let dir = TempDir::new("config");
    let data_dir = dir.arg();

    let (code, stdout, _) = run_cli(&[
        "config",
        "get",
        "ui.language",
        "--json",
        "--data-dir",
        &data_dir,
    ]);
    assert_eq!(code, 0);
    let value = parse_json(&stdout);
    assert_eq!(value["data"]["key"], "ui.language");
    assert_eq!(value["data"]["value"], "zh_cn");

    let (code, stdout, _) = run_cli(&[
        "config",
        "set",
        "ui.language",
        "en_us",
        "--json",
        "--data-dir",
        &data_dir,
    ]);
    assert_eq!(code, 0, "{stdout}");
    let value = parse_json(&stdout);
    assert_eq!(value["data"]["old"], "zh_cn");
    assert_eq!(value["data"]["new"], "en_us");

    let (_, stdout, _) = run_cli(&[
        "config",
        "get",
        "ui.language",
        "--json",
        "--data-dir",
        &data_dir,
    ]);
    assert_eq!(parse_json(&stdout)["data"]["value"], "en_us");
}

#[test]
fn error_envelope_shape() {
    let dir = TempDir::new("errors");
    let (code, stdout, _) = run_cli(&["launch", "nope", "--json", "--data-dir", &dir.arg()]);
    assert_eq!(code, 1);
    let value = parse_json(&stdout);
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "not_found");
    assert_eq!(value["error"]["kind"], "user");
    assert!(value["error"]["message"]
        .as_str()
        .unwrap_or_default()
        .contains("nope"));
    assert!(value["error"]["hint"].is_string());
}
