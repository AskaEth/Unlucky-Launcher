//! 退出码与输出通道（0 成功 / 1 用户错误 / 2 内部错误）。

#[path = "support/mod.rs"]
mod support;

use support::{run_cli, write_version, TempDir};
use unlucky_cli::error::{CliError, ErrorKind, EXIT_INTERNAL, EXIT_OK, EXIT_USER};

#[test]
fn exit_code_constants_match_kinds() {
    assert_eq!(EXIT_OK, 0);
    assert_eq!(EXIT_USER, 1);
    assert_eq!(EXIT_INTERNAL, 2);
    assert_eq!(ErrorKind::User.exit_code(), 1);
    assert_eq!(ErrorKind::Internal.exit_code(), 2);
    assert_eq!(CliError::usage("x").exit_code(), 1);
    assert_eq!(CliError::user("io", "x").exit_code(), 1);
    assert_eq!(CliError::internal("bug", "x").exit_code(), 2);
}

#[test]
fn internal_errors_serialize_as_internal() {
    let value = CliError::internal("serialize", "序列化失配").to_json();
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["kind"], "internal");
    assert_eq!(value["error"]["code"], "serialize");
    assert!(value["error"].get("hint").is_none());
}

#[test]
fn success_exits_zero() {
    let dir = TempDir::new("code-ok");
    let (code, _, _) = run_cli(&["instances", "list", "--data-dir", &dir.arg()]);
    assert_eq!(code, 0);
}

#[test]
fn user_error_exits_one() {
    let dir = TempDir::new("code-user");
    let cases: Vec<Vec<String>> = vec![
        vec!["frobnicate".to_string()],
        vec!["install".to_string()],
        vec![
            "launch".to_string(),
            "nope".to_string(),
            "--data-dir".to_string(),
            dir.arg(),
        ],
        vec![
            "logs".to_string(),
            "deadbeef".to_string(),
            "--data-dir".to_string(),
            dir.arg(),
        ],
        // `java install` 已实装（会真联网下载）——用户错误只取参数校验这一层：
        // 大版本号离谱时在联网之前就拒，测试因此保持零网络（M27 踩坑记录 G 节同款教训）。
        vec![
            "java".to_string(),
            "install".to_string(),
            "4".to_string(),
            "--data-dir".to_string(),
            dir.arg(),
        ],
        vec![
            "--data-dir".to_string(),
            "relative/path".to_string(),
            "instances".to_string(),
            "list".to_string(),
        ],
    ];
    for args in cases {
        let (code, _, stderr) = run_cli(&args.iter().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(code, 1, "args={args:?} stderr={stderr}");
        assert!(stderr.contains("错误:"), "args={args:?} stderr={stderr}");
    }
}

#[test]
fn human_and_json_go_to_different_streams() {
    let dir = TempDir::new("code-streams");
    write_version(dir.path(), "1.20.1");

    // 人类模式：结果进 stdout。
    let (code, stdout, stderr) = run_cli(&["instances", "list", "--data-dir", &dir.arg()]);
    assert_eq!(code, 0);
    assert!(stdout.contains("实例"), "{stdout}");
    assert!(stderr.is_empty(), "{stderr}");

    // 错误进 stderr，不污染 stdout。
    let (code, stdout, stderr) = run_cli(&["launch", "nope", "--data-dir", &dir.arg()]);
    assert_eq!(code, 1);
    assert!(stdout.is_empty(), "{stdout}");
    assert!(stderr.contains("错误:"), "{stderr}");
}

#[test]
fn quiet_suppresses_human_output_only() {
    let dir = TempDir::new("code-quiet");
    let (code, stdout, stderr) =
        run_cli(&["instances", "list", "--quiet", "--data-dir", &dir.arg()]);
    assert_eq!(code, 0);
    assert!(stdout.is_empty(), "{stdout}");
    assert!(stderr.is_empty(), "{stderr}");

    // 错误不受 --quiet 影响（脚本仍要能看到原因）。
    let (code, _, stderr) = run_cli(&["launch", "nope", "--quiet", "--data-dir", &dir.arg()]);
    assert_eq!(code, 1);
    assert!(stderr.contains("错误:"), "{stderr}");
}
