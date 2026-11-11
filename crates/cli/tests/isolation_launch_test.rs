//! D20 隔离档位的 CLI 端到端：`ull launch` 按覆盖链把派生结果填进 `--gameDir`。
//!
//! 四条路径（全局 isolated / 全局 shared / 实例 override / custom）+ 旧档案读兼容 +
//! `--dry-run` 不落盘。断言走 `--json` 输出的 `isolation` / `isolation_source` / `game_dir`，
//! 也就是真正喂给 `${game_directory}` 与子进程 CWD 的那个值。

#[path = "support/mod.rs"]
mod support;

use std::path::{Path, PathBuf};

use support::{parse_json, run_cli, TempDir};

/// 实例 id（与 `profiles.json` 里写的必须一致）。
const INSTANCE_ID: &str = "11111111-2222-4333-8444-000000000001";
/// 版本 id。
const VERSION: &str = "1.20.1";

/// 建数据根：版本本体 + `profiles.json`（一个实例「生存服」）。
fn data_root(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    write_version_with_game_dir(dir.path());
    write_profiles(dir.path(), None);
    dir
}

/// 写一份「带 `--gameDir ${game_directory}`」的最小版本 JSON + 空 client jar。
///
/// 比 `support::write_version` 多这一个参数——`--gameDir` 就是隔离档位的真实落点
/// （正式版清单 1.13+ 都有这条），没有它就无法从命令行验证派生结果。
fn write_version_with_game_dir(root: &Path) {
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
                "--version", "${version_name}",
                "--gameDir", "${game_directory}"
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
    std::fs::write(dir.join(format!("{VERSION}.jar")), b"junk").expect("写 client jar");
}

/// 写 `config/profiles.json`（可选 `game_dir` 遗留字段）。
fn write_profiles(root: &Path, legacy_game_dir: Option<&Path>) {
    let profiles_dir = root.join(".ULL").join("config");
    std::fs::create_dir_all(&profiles_dir).expect("建配置目录");
    let mut profile = serde_json::json!({
        "id": INSTANCE_ID,
        "name": "生存服",
        "version_id": VERSION,
    });
    if let Some(game_dir) = legacy_game_dir {
        profile["game_dir"] = serde_json::json!(game_dir.display().to_string());
    }
    let document = serde_json::json!({
        "schema_version": 1,
        "profiles": [profile],
    });
    std::fs::write(
        profiles_dir.join("profiles.json"),
        serde_json::to_vec_pretty(&document).expect("序列化 profiles.json"),
    )
    .expect("写 profiles.json");
}

/// 写实例设置文件（`config/instances/<id>.json`）。
fn write_instance_config(dir: &Path, value: &serde_json::Value) {
    let path = dir
        .join(".ULL")
        .join("config")
        .join("instances")
        .join(format!("{INSTANCE_ID}.json"));
    std::fs::create_dir_all(path.parent().expect("父目录")).expect("建实例配置目录");
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(value).expect("序列化实例设置"),
    )
    .expect("写实例设置");
}

/// 跑一次 `launch <instance> --dry-run --json`。
fn dry_run(dir: &TempDir) -> serde_json::Value {
    let (code, stdout, stderr) = run_cli(&[
        "launch",
        "生存服",
        "--dry-run",
        "--json",
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 0, "{stderr}");
    parse_json(&stdout)["data"].clone()
}

/// 数据根下的游戏目录。
fn game_root(dir: &TempDir) -> PathBuf {
    dir.path().join(".minecraft")
}

#[test]
fn default_mode_is_isolated_and_derives_version_dir() {
    let dir = data_root("iso-default");
    let data = dry_run(&dir);
    assert_eq!(data["isolation"], "isolated", "D20 默认隔离");
    assert_eq!(data["isolation_source"], "global");
    assert_eq!(data["instance_found"], true, "实例名解析仍然有效");
    assert_eq!(
        PathBuf::from(data["game_dir"].as_str().expect("game_dir")),
        game_root(&dir).join("versions").join(VERSION)
    );
    assert!(data["mode_change_warning"].is_null());

    // 端到端：派生值必须是命令行里 `--gameDir` 的实参（占位符已替换、家目录已脱敏，
    // 所以按路径后缀断言）。
    let argv: Vec<&str> = data["argv"]
        .as_array()
        .expect("argv")
        .iter()
        .map(|token| token.as_str().expect("token"))
        .collect();
    let index = argv
        .iter()
        .position(|token| *token == "--gameDir")
        .unwrap_or_else(|| panic!("命令行里没有 --gameDir：{argv:?}"));
    let passed = PathBuf::from(argv[index + 1]);
    assert!(
        passed.ends_with(Path::new("versions").join(VERSION)),
        "--gameDir 不是派生的隔离目录：{}",
        passed.display()
    );
}

#[test]
fn shared_mode_switches_game_dir_to_game_root() {
    let dir = data_root("iso-shared");
    let (code, _, stderr) = run_cli(&[
        "config",
        "set",
        "instances.mode",
        "shared",
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 0, "{stderr}");

    let data = dry_run(&dir);
    assert_eq!(data["isolation"], "shared");
    assert_eq!(data["isolation_source"], "global");
    assert_eq!(
        PathBuf::from(data["game_dir"].as_str().expect("game_dir")),
        game_root(&dir),
        "共享档的运行目录就是 .minecraft 根"
    );
}

#[test]
fn instance_isolation_overrides_global_mode() {
    let dir = data_root("iso-instance");
    let (code, _, stderr) = run_cli(&[
        "config",
        "set",
        "instances.mode",
        "shared",
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 0, "{stderr}");
    write_instance_config(
        dir.path(),
        &serde_json::json!({
            "schemaVersion": 1,
            "id": INSTANCE_ID,
            "versionId": VERSION,
            "isolation": "isolated",
        }),
    );

    let data = dry_run(&dir);
    assert_eq!(data["isolation"], "isolated", "实例档位压过全局");
    assert_eq!(data["isolation_source"], "instance");
    assert_eq!(
        PathBuf::from(data["game_dir"].as_str().expect("game_dir")),
        game_root(&dir).join("versions").join(VERSION)
    );
}

#[test]
fn custom_isolation_uses_the_recorded_game_dir() {
    let dir = data_root("iso-custom");
    let custom = dir.path().join("mcs").join("farm");
    write_instance_config(
        dir.path(),
        &serde_json::json!({
            "schemaVersion": 1,
            "id": INSTANCE_ID,
            "versionId": VERSION,
            "isolation": "custom",
            "gameDir": custom.display().to_string(),
        }),
    );

    let data = dry_run(&dir);
    assert_eq!(data["isolation"], "custom");
    assert_eq!(
        PathBuf::from(data["game_dir"].as_str().expect("game_dir")),
        custom
    );
}

#[test]
fn legacy_profile_game_dir_reads_as_custom() {
    // M27 之前的档案只在 profiles.json 里存 game_dir（没有实例设置文件）：
    // 显式指定过的目录不能被静默退回全局档。
    let dir = TempDir::new("iso-legacy");
    write_version_with_game_dir(dir.path());
    let legacy = dir.path().join("old-instance-dir");
    write_profiles(dir.path(), Some(&legacy));
    let data = dry_run(&dir);
    assert_eq!(data["isolation"], "custom");
    assert_eq!(
        PathBuf::from(data["game_dir"].as_str().expect("game_dir")),
        legacy
    );
}

#[test]
fn dry_run_does_not_write_instance_state() {
    // 切换检测只在真启动时落盘：`--dry-run` 不该吃掉「切换后首次启动提示一次」。
    let dir = data_root("iso-dry-run");
    let config_path = dir
        .path()
        .join(".ULL")
        .join("config")
        .join("instances")
        .join(format!("{INSTANCE_ID}.json"));
    assert!(!config_path.exists());
    let _ = dry_run(&dir);
    assert!(
        !config_path.exists(),
        "--dry-run 写盘了：{}",
        config_path.display()
    );
}

#[test]
fn unknown_instance_still_reports_available_names() {
    let dir = data_root("iso-unknown");
    let (code, _, stderr) = run_cli(&[
        "launch",
        "不存在的实例",
        "--dry-run",
        "--data-dir",
        &dir.arg(),
    ]);
    assert_ne!(code, 0, "未知实例要失败");
    assert!(stderr.contains("生存服"), "提示里要列出可用实例：{stderr}");
}

/// 读实例设置文件（不存在返回 `None`）。
fn read_instance_config(dir: &TempDir) -> Option<serde_json::Value> {
    let path = dir
        .path()
        .join(".ULL")
        .join("config")
        .join("instances")
        .join(format!("{INSTANCE_ID}.json"));
    let text = std::fs::read_to_string(path).ok()?;
    Some(serde_json::from_str(&text).expect("实例设置应为合法 JSON"))
}

/// 真启动一次（用一个不存在的 java，保证 spawn 必失败、不会真的跑起游戏），
/// 只为驱动「真启动才写盘」的那段逻辑。
fn real_launch_that_fails(dir: &TempDir) -> (i32, String, String) {
    let bogus_java = dir.path().join("no-such-java.exe");
    run_cli(&[
        "launch",
        "生存服",
        "--java",
        &bogus_java.display().to_string(),
        "--data-dir",
        &dir.arg(),
    ])
}

#[test]
fn first_real_launch_bootstraps_instance_file() {
    // 回归 P2-3（review r1）：profiles.json 有档案但没有实例设置文件（M27 之前的形态），
    // 真启动要补建基线文件，否则切换检测对这批遗留实例永远不启动。
    let dir = data_root("iso-bootstrap");
    assert!(read_instance_config(&dir).is_none(), "前置：还没有设置文件");

    let (code, _, stderr) = real_launch_that_fails(&dir);
    assert_ne!(code, 0, "假 java 必然启动失败：{stderr}");

    let config = read_instance_config(&dir).expect("首次真启动应补建设置文件");
    assert_eq!(config["versionId"], VERSION);
    assert_eq!(config["id"], INSTANCE_ID);
    // 基线：from == to，且已经标记 warned（不该平白弹一次提示）。
    assert_eq!(config["lastModeChange"]["from"], "isolated");
    assert_eq!(config["lastModeChange"]["to"], "isolated");
    assert_eq!(config["lastModeChange"]["warned"], true);
    assert!(config["lastModeChange"]["at"].is_string());
    assert!(config["lastModeChange"]["probe"]["isolatedSide"].is_number());
    assert!(config["lastModeChange"]["probe"]["sharedSide"].is_number());
    // inherit 不落盘。
    assert!(config.get("isolation").is_none(), "{config}");
}

#[test]
fn legacy_profile_dir_is_recorded_as_custom_on_bootstrap() {
    // 同一批遗留档案里「显式指定过目录」的那部分：补建时把 old profiles.game_dir → custom
    // 固化进实例文件，之后实例文件就是唯一真相。
    let dir = TempDir::new("iso-bootstrap-legacy");
    write_version_with_game_dir(dir.path());
    let legacy = dir.path().join("old-instance-dir");
    write_profiles(dir.path(), Some(&legacy));

    let (code, _, stderr) = real_launch_that_fails(&dir);
    assert_ne!(code, 0, "假 java 必然启动失败：{stderr}");

    let config = read_instance_config(&dir).expect("首次真启动应补建设置文件");
    assert_eq!(config["isolation"], "custom");
    assert_eq!(
        PathBuf::from(config["gameDir"].as_str().expect("gameDir")),
        legacy
    );
    assert_eq!(config["lastModeChange"]["to"], "custom");
    assert_eq!(config["lastModeChange"]["warned"], true, "基线不弹提示");
}
