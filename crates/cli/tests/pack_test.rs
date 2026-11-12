//! `ull pack import` 的三态契约：退出码 0 / 3 / 1 + `--json` 信封恒有结构。
//!
//! **全部离线、全部不依赖 zip**：
//!
//! - MultiMC / Prism 的真实形态就是**目录**，直接拷 → `0`；
//! - mrpack 也能以**目录**打开（`PackArchive` 对目录与 zip 一视同仁），造一条 `downloads` 为空的
//!   `files[]` 条目 ⇒ 补不出来 ⇒ `3`，不会发任何请求；
//! - 坏包用**一个不是 zip 的普通文件** ⇒ 开包即拒 ⇒ `1`（具体的恶意样本分类在 core 的
//!   `addon/tests/import_failures.rs`，这里只钉 CLI 的退出码与 JSON 契约）。

#[path = "support/mod.rs"]
mod support;

use std::path::{Path, PathBuf};

use serde_json::Value;
use support::{run_cli, TempDir};

/// 写一个文件（自动建父目录）。
fn write_file(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("建目录");
    }
    std::fs::write(path, bytes).expect("写文件");
}

/// 造一个最小 MultiMC / Prism 实例目录。
fn multimc_instance(root: &Path) -> PathBuf {
    let instance = root.join("mmc");
    write_file(
        &instance.join("mmc-pack.json"),
        br#"{"formatVersion": 1, "components": [
            {"uid": "net.minecraft", "version": "1.20.1"},
            {"uid": "net.fabricmc.fabric-loader", "version": "0.15.11"},
            {"uid": "org.lwjgl3", "version": "3.3.2"}]}"#,
    );
    write_file(
        &instance.join("instance.cfg"),
        b"InstanceType=OneSix\nname=cli pack\n",
    );
    write_file(&instance.join(".minecraft/mods/a.jar"), b"jar bytes");
    instance
}

/// 造一个目录形式的 mrpack：一条远端条目，但 `downloads` 为空（补不出来 ⇒ 缺文件）。
fn mrpack_directory(root: &Path) -> PathBuf {
    let source = root.join("mr-src");
    write_file(
        &source.join("modrinth.index.json"),
        br#"{"game": "minecraft", "formatVersion": 1, "versionId": "1.0.0",
             "name": "cli mrpack", "summary": "dir-form mrpack",
             "files": [{"path": "mods/ghost.jar",
                        "hashes": {"sha1": "0123456789abcdef0123456789abcdef01234567",
                                   "sha512": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"},
                        "env": {"client": "required", "server": "required"},
                        "downloads": [], "fileSize": 11}],
             "dependencies": {"minecraft": "1.20.1", "fabric-loader": "0.15.11"}}"#,
    );
    write_file(&source.join("overrides/config/known.toml"), b"k = 1\n");
    source
}

fn parse_json(stdout: &str) -> Value {
    serde_json::from_str(stdout)
        .unwrap_or_else(|error| panic!("stdout 不是合法 JSON（{error}）：{stdout}"))
}

#[test]
fn imports_a_multimc_instance_and_exits_zero() {
    let data = TempDir::new("pack-ok");
    let source = multimc_instance(data.path());
    let target = data.path().join("inst");
    let (code, stdout, stderr) = run_cli(&[
        "pack",
        "import",
        &source.display().to_string(),
        "--target",
        &target.display().to_string(),
        "--data-dir",
        &data.arg(),
        "--no-register",
        "--json",
    ]);

    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");
    let value = parse_json(&stdout);
    assert_eq!(value["ok"], Value::Bool(true));
    assert_eq!(value["data"]["status"], Value::String("success".into()));
    assert_eq!(value["data"]["format"], Value::String("multimc".into()));
    assert_eq!(
        value["data"]["loader"]["kind"],
        Value::String("fabric".into())
    );
    // 走过的阶段都记下来了。
    let stages = value["data"]["stages"].as_array().expect("stages 数组");
    assert!(stages.iter().any(|stage| stage == "probe"), "{stages:?}");
    assert!(stages.iter().any(|stage| stage == "commit"), "{stages:?}");
    // 内容真的落地了。
    assert_eq!(
        std::fs::read(target.join("mods/a.jar")).expect("a.jar"),
        b"jar bytes"
    );
    // 未支持的 component 逐项进 JSON。
    assert_eq!(
        value["data"]["rejected_components"][0]["uid"],
        Value::String("org.lwjgl3".into())
    );
}

#[test]
fn missing_files_make_it_partial_and_exit_three() {
    let data = TempDir::new("pack-partial");
    let source = mrpack_directory(data.path());
    let target = data.path().join("inst");
    let (code, stdout, stderr) = run_cli(&[
        "pack",
        "import",
        &source.display().to_string(),
        "--target",
        &target.display().to_string(),
        "--data-dir",
        &data.arg(),
        "--no-register",
        "--json",
    ]);

    assert_eq!(
        code, 3,
        "部分成功是 3，不是 0/1：stdout={stdout} stderr={stderr}"
    );
    let value = parse_json(&stdout);
    assert_eq!(value["data"]["status"], Value::String("partial".into()));
    assert_eq!(
        value["data"]["missing"].as_array().map(Vec::len),
        Some(1),
        "补不出来的条目要如实记缺：{stdout}"
    );
    // 覆盖内容照装。
    assert!(target.join("config/known.toml").is_file());
}

#[test]
fn failed_import_exits_one_and_still_emits_structured_json() {
    let data = TempDir::new("pack-fail");
    let pack = data.path().join("broken.zip");
    // 一个根本不是 zip 的文件：开包即拒。
    write_file(&pack, b"this is not a zip archive");

    let target = data.path().join("inst");
    let (code, stdout, stderr) = run_cli(&[
        "pack",
        "import",
        &pack.display().to_string(),
        "--target",
        &target.display().to_string(),
        "--data-dir",
        &data.arg(),
        "--no-register",
        "--json",
    ]);

    assert_eq!(code, 1, "stdout={stdout} stderr={stderr}");
    // 失败也**必须**是结构化 JSON（不抛栈、不打人类可读文本到 stdout）。
    let value = parse_json(&stdout);
    assert_eq!(value["ok"], Value::Bool(true));
    assert_eq!(value["data"]["status"], Value::String("failed".into()));
    assert!(
        value["data"]["error"]["code"].is_string(),
        "失败要带稳定码：{stdout}"
    );
    assert_eq!(value["data"]["rolled_back"], Value::Bool(true));
    // 零残留。
    assert!(!target.exists(), "致命失败后目标目录必须不存在");
    // 失败也不是"崩了"：stderr 里不该出现 panic 之类的栈信息。
    assert!(!stderr.contains("panicked"), "{stderr}");
}

#[test]
fn other_commands_still_exit_zero() {
    // `Outcome::exit_code` 是给 pack import 用的；其它命令的行为不许变。
    let data = TempDir::new("pack-regression");
    let (code, stdout, _) = run_cli(&["instances", "list", "--data-dir", &data.arg()]);
    assert_eq!(code, 0, "{stdout}");
    let (code, _, _) = run_cli(&["--help"]);
    assert_eq!(code, 0);
    let (code, _, _) = run_cli(&["config", "get", "--data-dir", &data.arg()]);
    assert_eq!(code, 0);
}

#[test]
fn help_lists_the_pack_command() {
    let (code, stdout, _) = run_cli(&["help", "pack"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("pack import"), "{stdout}");
    assert!(stdout.contains("部分成功"), "{stdout}");
    let (code, stdout, _) = run_cli(&["--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("pack import"), "总帮助里也要有：{stdout}");
}

/// **M216 P3-3 + §5**：不带 `--target` 时落点是**新实例目录**（不是共享 `.minecraft` 根），
/// 而且登记出来的实例被绑到那个目录（`isolation=custom` + `gameDir`），否则导入内容永远不会
/// 被启动用到。
///
/// 抓得住：CLI 自己兜底回 `ctx.game().root()`（第一条断言会变成 `.minecraft`）；不建实例目录而
/// 直接铺共享根；不做绑定（`isolation` 仍是 `null`）；绑定写错目录。抓不住：真实启动那一条
/// `derive_game_dir` 的派生（core 的 `isolation.rs` 用例钉，这里只断言设置文件的内容）。
#[test]
fn the_default_target_is_a_new_instance_dir_and_the_instance_is_bound_to_it() {
    let data = TempDir::new("pack-default-target");
    let source = multimc_instance(data.path());
    let (code, stdout, stderr) = run_cli(&[
        "pack",
        "import",
        &source.display().to_string(),
        "--name",
        "My Pack",
        "--data-dir",
        &data.arg(),
        "--json",
    ]);
    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");

    // **M216 r2**：绑定成败也进 `--json`（结构化，不必猜文案）。
    let value = parse_json(&stdout);
    assert_eq!(
        value["data"]["instance_bound"],
        Value::Bool(true),
        "缺省导入应当把实例绑到内容目录并报 true：{stdout}"
    );

    let instance_dir = data.path().join(".ULL/data/instances/My-Pack");
    assert!(
        instance_dir.join("mods/a.jar").is_file(),
        "缺省落点应当是 <data>/instances/My-Pack：{stdout}"
    );
    assert!(
        !data.path().join(".minecraft/mods/a.jar").exists(),
        "不许再往共享 .minecraft 根铺：{stdout}"
    );

    // 登记的实例被绑到这个目录（§5）。
    let profiles: Value = serde_json::from_str(
        &std::fs::read_to_string(data.path().join(".ULL/config/profiles.json"))
            .expect("profiles.json"),
    )
    .expect("profiles.json 是 JSON");
    assert_eq!(
        profiles["profiles"][0]["name"],
        Value::String("My Pack".into())
    );
    let id = profiles["profiles"][0]["id"]
        .as_str()
        .expect("登记出来的实例有 id")
        .to_string();
    let config: Value = serde_json::from_str(
        &std::fs::read_to_string(
            data.path()
                .join(".ULL/config/instances")
                .join(format!("{id}.json")),
        )
        .expect("实例设置文件"),
    )
    .expect("实例设置是 JSON");
    assert_eq!(
        config["isolation"],
        Value::String("custom".into()),
        "isolation 应当是 custom：{config}"
    );
    assert_eq!(
        config["gameDir"].as_str().map(Path::new),
        Some(instance_dir.as_path()),
        "gameDir 应当指向导入目录：{config}"
    );
}

/// **M216 P2-1 / r2**：`--help` 里的缺省落点必须是**真路径**。
///
/// 「数据根」在这份帮助自己的定义里是「含 `.ULL/` 与 `.minecraft/` 的父目录」（`--data-dir` 那行），
/// 所以实例目录的真路径是 `<数据根>/.ULL/data/instances/<目录名>`。r1 写成了
/// `<数据根>/instances/<实例名>` —— 少一层 `.ULL/data`，照读会去一个不存在的位置找包内容。
///
/// 抓得住：缺 `.ULL/data` 这一层（下面第二条断言就是抓它的——少写时该子串恰好出现）；
/// 文案退回「共享游戏目录」；目录名归一化这件事没提。抓不住：`model.rs` 的字段文档
/// （编译期文档，测试读不到），以及真实 `--data-dir` 下的绝对路径拼接（那由上面那条端到端
/// 用例按真目录断言）。
#[test]
fn help_describes_the_new_default_target_with_the_real_path() {
    let (code, stdout, _) = run_cli(&["help", "pack"]);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("<数据根>/.ULL/data/instances/<实例目录名>"),
        "帮助里没写真路径 <数据根>/.ULL/data/instances/<实例目录名>：{stdout}"
    );
    assert!(
        !stdout.contains("<数据根>/instances/"),
        "帮助里的落点少了 `.ULL/data` 这一层（r2 抓的就是这个错法）：{stdout}"
    );
    assert!(
        !stdout.contains("缺省 = 数据根的共享游戏目录"),
        "帮助里还留着旧缺省落点：{stdout}"
    );
}
