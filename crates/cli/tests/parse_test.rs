//! 子命令解析：手写解析器的语法契约。

#[path = "support/mod.rs"]
mod support;

use unlucky_cli::cli::{parse, AccountsAddKind, Command, Globals};
use unlucky_cli::error::ErrorKind;

fn invoke(args: &[&str]) -> unlucky_cli::cli::Invocation {
    let owned: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    parse(&owned).unwrap_or_else(|error| panic!("解析失败：{error}"))
}

#[test]
fn no_args_is_welcome() {
    let invocation = invoke(&[]);
    assert_eq!(invocation.command, Command::Welcome);
    assert_eq!(invocation.globals, Globals::default());
}

#[test]
fn version_flags() {
    assert_eq!(invoke(&["--version"]).command, Command::Version);
    assert_eq!(invoke(&["-V"]).command, Command::Version);
    assert_eq!(invoke(&["version"]).command, Command::Version);
}

#[test]
fn help_takes_topic_from_positional() {
    assert_eq!(invoke(&["--help"]).command, Command::Help(None));
    assert_eq!(
        invoke(&["--help", "launch"]).command,
        Command::Help(Some("launch".to_string()))
    );
    assert_eq!(
        invoke(&["help", "config"]).command,
        Command::Help(Some("config".to_string()))
    );
}

#[test]
fn globals_are_position_independent() {
    let invocation = invoke(&["instances", "list", "--json", "--quiet"]);
    assert_eq!(invocation.command, Command::InstancesList);
    assert!(invocation.globals.json);
    assert!(invocation.globals.quiet);

    let before = invoke(&["--json", "instances", "list"]);
    assert!(before.globals.json);
    assert_eq!(before.command, Command::InstancesList);
}

#[test]
fn data_dir_accepts_both_forms() {
    assert_eq!(
        invoke(&["--data-dir", "/tmp/ull", "instances", "list"])
            .globals
            .data_dir,
        Some(std::path::PathBuf::from("/tmp/ull"))
    );
    assert_eq!(
        invoke(&["instances", "list", "--data-dir=/tmp/ull"])
            .globals
            .data_dir,
        Some(std::path::PathBuf::from("/tmp/ull"))
    );
}

#[test]
fn install_flags() {
    let Command::Install(args) =
        invoke(&["install", "1.20.1", "--loader", "vanilla", "--name=x"]).command
    else {
        panic!("应为 install");
    };
    assert_eq!(args.version, "1.20.1");
    assert_eq!(args.loader.as_deref(), Some("vanilla"));
    assert_eq!(args.name.as_deref(), Some("x"));
    assert!(!args.dry_run);
    assert!(args.progress, "人类模式默认打实时进度");
}

#[test]
fn install_loader_version_flag() {
    let Command::Install(args) = invoke(&[
        "install",
        "1.20.1",
        "--loader",
        "fabric",
        "--loader-version",
        "0.15.11",
    ])
    .command
    else {
        panic!("应为 install");
    };
    assert_eq!(args.loader.as_deref(), Some("fabric"));
    assert_eq!(args.loader_version.as_deref(), Some("0.15.11"));
    // 不带该 flag 时为 None（由 core 取官方默认版本）。
    let Command::Install(default) = invoke(&["install", "1.20.1"]).command else {
        panic!("应为 install");
    };
    assert!(default.loader_version.is_none());
}

#[test]
fn install_dry_run_flag_and_progress_suppression() {
    let Command::Install(args) = invoke(&["install", "1.20.1", "--dry-run"]).command else {
        panic!("应为 install");
    };
    assert!(args.dry_run);
    assert!(args.progress);
    assert!(!args.json_progress, "人类模式打文本进度");

    // `--json`：stderr 换成 JSONL 进度（stdout 仍是单个信封）。
    let Command::Install(args) = invoke(&["--json", "install", "1.20.1"]).command else {
        panic!("应为 install");
    };
    assert!(args.progress);
    assert!(args.json_progress);

    // `--quiet`：一律不打进度。
    let Command::Install(args) = invoke(&["install", "1.20.1", "-q"]).command else {
        panic!("应为 install");
    };
    assert!(!args.progress);
    assert!(!args.dry_run);
}

#[test]
fn launch_flags_and_defaults() {
    let Command::Launch(args) = invoke(&["launch", "生存服"]).command else {
        panic!("应为 launch");
    };
    assert_eq!(args.instance, "生存服");
    assert!(!args.dry_run);
    assert_eq!(args.java, None);
    assert_eq!(args.max_memory, None);

    let Command::Launch(args) = invoke(&[
        "launch",
        "生存服",
        "--dry-run",
        "--java",
        "/opt/jdk/bin/java",
        "--username",
        "Steve",
        "--max-memory",
        "4096",
    ])
    .command
    else {
        panic!("应为 launch");
    };
    assert!(args.dry_run);
    assert_eq!(args.username.as_deref(), Some("Steve"));
    assert_eq!(args.max_memory, Some(4096));
}

#[test]
fn accounts_subcommands() {
    assert_eq!(invoke(&["accounts"]).command, Command::AccountsList);
    assert_eq!(invoke(&["accounts", "list"]).command, Command::AccountsList);

    let Command::AccountsAdd(args) = invoke(&["accounts", "add", "offline", "Steve"]).command
    else {
        panic!("应为 accounts add offline");
    };
    assert_eq!(args.kind, AccountsAddKind::Offline);
    assert_eq!(args.name.as_deref(), Some("Steve"));

    let Command::AccountsAdd(args) = invoke(&["accounts", "add", "msa"]).command else {
        panic!("应为 accounts add msa");
    };
    assert_eq!(args.kind, AccountsAddKind::Microsoft);
    assert_eq!(args.name, None);

    let Command::AccountsRemove { name } = invoke(&["accounts", "remove", "Steve"]).command else {
        panic!("应为 accounts remove");
    };
    assert_eq!(name, "Steve");
}

#[test]
fn launch_account_flag() {
    let Command::Launch(args) = invoke(&["launch", "生存服", "--account", "Steve"]).command
    else {
        panic!("应为 launch");
    };
    assert_eq!(args.account.as_deref(), Some("Steve"));
    assert_eq!(args.username, None);
}

#[test]
fn logs_and_diag_default_tails() {
    let Command::Logs(args) = invoke(&["logs", "abc"]).command else {
        panic!("应为 logs");
    };
    assert_eq!(args.tail, 200);

    let Command::Diag(args) = invoke(&["diag", "abc", "--tail", "10"]).command else {
        panic!("应为 diag");
    };
    assert_eq!(args.tail, 10);
}

#[test]
fn config_get_set_forms() {
    let Command::ConfigGet(args) = invoke(&["config", "get"]).command else {
        panic!("应为 config get");
    };
    assert_eq!(args.key, None);
    assert!(!args.show_secret);

    let Command::ConfigGet(args) =
        invoke(&["config", "get", "ui.language", "--show-secret"]).command
    else {
        panic!("应为 config get");
    };
    assert_eq!(args.key.as_deref(), Some("ui.language"));
    assert!(args.show_secret);

    let Command::ConfigSet(args) = invoke(&["config", "set", "ui.language", "en_us"]).command
    else {
        panic!("应为 config set");
    };
    assert_eq!(args.key, "ui.language");
    assert_eq!(args.value, "en_us");
}

#[test]
fn java_subcommands() {
    assert_eq!(invoke(&["java"]).command, Command::JavaList);
    assert_eq!(invoke(&["java", "list"]).command, Command::JavaList);
    assert_eq!(
        invoke(&["java", "install", "21"]).command,
        Command::JavaInstall {
            major: 21,
            vendor: None
        }
    );
    // D23：`--vendor` 钉死发行版（别名也认），`java vendors` 列出四家。
    assert_eq!(
        invoke(&["java", "install", "21", "--vendor", "oracle"]).command,
        Command::JavaInstall {
            major: 21,
            vendor: Some(unlucky_core::game::java::JdkVendor::Oracle),
        }
    );
    assert_eq!(
        invoke(&["java", "install", "17", "--vendor", "adoptium"]).command,
        Command::JavaInstall {
            major: 17,
            vendor: Some(unlucky_core::game::java::JdkVendor::Temurin),
        }
    );
    assert_eq!(invoke(&["java", "vendors"]).command, Command::JavaVendors);
}

#[test]
fn update_subcommands() {
    use unlucky_cli::cli::{UpdateApplyArgs, UpdateCheckArgs};

    assert_eq!(
        invoke(&["update", "check"]).command,
        Command::UpdateCheck(UpdateCheckArgs {})
    );
    assert_eq!(
        invoke(&["update", "apply"]).command,
        Command::UpdateApply(UpdateApplyArgs { dest: None })
    );
    // `--dest` 两种写法都要认（钉替换目标 = 取证 / 自定义安装位置）
    for args in [
        vec!["update", "apply", "--dest", "D:/tmp/ull.exe"],
        vec!["update", "apply", "--dest=D:/tmp/ull.exe"],
    ] {
        assert_eq!(
            invoke(&args).command,
            Command::UpdateApply(UpdateApplyArgs {
                dest: Some(std::path::PathBuf::from("D:/tmp/ull.exe")),
            })
        );
    }
}

/// `update` 的用法错误：没子命令 / 未知子命令 / 多余位置参数 / 未知选项——四条都是 `usage`
/// （退出码 1），不许静默退化成某个默认动作。
#[test]
fn update_usage_errors() {
    for args in [
        vec!["update"],
        vec!["update", "install"],
        vec!["update", "check", "extra"],
        vec!["update", "apply", "extra"],
        vec!["update", "apply", "--dest"],
        vec!["update", "apply", "--nope"],
    ] {
        let owned: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
        let error = parse(&owned).expect_err(&format!("{args:?} 应当报用法错误"));
        assert_eq!(error.kind(), ErrorKind::User, "{args:?}");
        assert_eq!(error.code(), "usage", "{args:?}");
    }
}

#[test]
fn usage_errors_are_user_errors() {
    let cases: &[&[&str]] = &[
        &["frobnicate"],
        &["install"],
        &["launch"],
        &["launch", "a", "b"],
        &["instances", "list", "--nope"],
        &["install", "1.20.1", "--dry-run=1"],
        &["config", "set", "only-key"],
        &["java", "install", "abc"],
        &["--data-dir"],
        &["sessions", "watch"],
        &["accounts", "add"],
        &["accounts", "add", "offline"],
        &["accounts", "add", "authlib"],
        &["accounts", "remove"],
        &["accounts", "frobnicate"],
    ];
    for args in cases {
        let owned: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
        let error = parse(&owned).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::User, "args={args:?}");
        assert_eq!(error.exit_code(), 1, "args={args:?}");
    }
}

#[test]
fn value_flags_do_not_swallow_the_next_flag() {
    // P2-2：漏了值的写法必须在解析期报用法错误，而不是把 `--dry-run` 当成 Java 路径。
    let cases: &[&[&str]] = &[
        &["launch", "x", "--java", "--dry-run"],
        &["launch", "x", "--username", "--json"],
        &["install", "1.20.1", "--name", "--loader", "fabric"],
        &["install", "1.20.1", "--loader", "--name", "x"],
        &["logs", "abc", "--tail", "--json"],
        &["--data-dir", "--json", "instances", "list"],
    ];
    for args in cases {
        let owned: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
        let error = parse(&owned).expect_err(&format!("args={args:?} 应当报用法错误"));
        assert_eq!(error.code(), "usage", "args={args:?}");
        assert!(
            error.message().contains("需要一个值"),
            "args={args:?} message={}",
            error.message()
        );
    }

    // 负数值与正常值不受影响（只拒 `--` 开头的 token）。
    let Command::Launch(args) = invoke(&["launch", "x", "--java", "/opt/jdk/bin/java"]).command
    else {
        panic!("应为 launch");
    };
    assert_eq!(
        args.java.as_deref(),
        Some(std::path::Path::new("/opt/jdk/bin/java"))
    );
}

#[test]
fn help_text_covers_every_command() {
    for topic in [
        "instances",
        "install",
        "launch",
        "sessions",
        "logs",
        "diag",
        "config",
        "java",
        "update",
    ] {
        assert!(
            unlucky_cli::cli::help::topic(topic).is_some(),
            "缺少 {topic} 的帮助"
        );
    }
    assert!(unlucky_cli::cli::help::topic("nope").is_none());
}
