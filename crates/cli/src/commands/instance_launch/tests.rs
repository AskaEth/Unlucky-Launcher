//! [`super`] 的单测：走真入口 [`crate::run`]，断言用户/脚本拿到的那份输出。
//!
//! 口径与 `instances/tests.rs` 一致：临时 `--data-dir` + 真 `profiles.json`，只看 stdout 的
//! JSON 信封（`--json`）与退出码。回落规则的真值表在 core 的
//! `game::launch_overrides` / `launch::overrides` 测试里，这里只验 CLI 有没有如实搬运。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use unlucky_core::platform::layout::{LauncherLayout, LAUNCHER_HOME_DIR};

/// 一次性临时目录（`Drop` 时清理；cli 没有 dev-dependency 的 tempfile）。
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "ull-instance-launch-{tag}-{}-{seq}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("建临时目录");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn arg(&self) -> String {
        self.0.display().to_string()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 写 `.ULL/config/profiles.json` 并返回第一个实例的 id。
fn write_profiles(data_root: &Path, name: &str, version_id: &str) -> String {
    let dir = data_root.join(LAUNCHER_HOME_DIR).join("config");
    std::fs::create_dir_all(&dir).expect("建配置目录");
    let id = "11111111-2222-4333-8444-555555555555".to_string();
    let document = json!({
        "schema_version": 1,
        "profiles": [{ "id": id, "name": name, "version_id": version_id }],
    });
    std::fs::write(
        dir.join("profiles.json"),
        serde_json::to_vec_pretty(&document).expect("序列化 profiles.json"),
    )
    .expect("写 profiles.json");
    id
}

/// 写 `<data>/.minecraft/versions/<id>/<id>.json`（库表可带 vulkan）。
fn write_version(data_root: &Path, id: &str, vulkan: bool) {
    let path = data_root
        .join(".minecraft")
        .join("versions")
        .join(id)
        .join(format!("{id}.json"));
    std::fs::create_dir_all(path.parent().expect("版本目录")).expect("版本目录");
    let libs = if vulkan {
        r#"[{"name":"org.lwjgl:lwjgl-vulkan:3.4.3"}]"#
    } else {
        "[]"
    };
    std::fs::write(&path, format!(r#"{{"id":"{id}","libraries":{libs}}}"#)).expect("写版本 JSON");
}

/// 跑一次 CLI，返回（退出码, stdout）。
fn run_cli(args: &[&str]) -> (i32, String) {
    let owned: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let code = crate::run(&owned, &mut out, &mut err);
    (code, String::from_utf8_lossy(&out).into_owned())
}

/// 跑 `instances launch-args … --json`，返回 `data` 载荷。
fn launch_args(dir: &TempDir, rest: &[&str]) -> Value {
    let root = dir.arg();
    let mut args = vec!["instances", "launch-args", "--json", "--data-dir", &root];
    args.extend_from_slice(rest);
    let (code, stdout) = run_cli(&args);
    assert_eq!(code, 0, "{stdout}");
    let value: Value = serde_json::from_str(&stdout).expect("输出不是 JSON");
    assert_eq!(value["ok"], true, "{stdout}");
    value["data"].clone()
}

#[test]
fn read_reports_instance_layer_and_effective_values() {
    let dir = TempDir::new("read");
    let id = write_profiles(dir.path(), "生存", "1.20.4");
    let data = launch_args(&dir, &[&id]);
    assert_eq!(data["instance_id"], id.as_str());
    assert_eq!(data["updated"], false);
    assert_eq!(data["launch"]["graphics_backend"], "default");
    // 没配内存 ⇒ 生效值是出厂默认 2048（三级回落的最后一级）。
    assert_eq!(data["effective"]["memory_mb"], 2048);
    assert_eq!(data["effective"]["graphics_backend_allowed"], false);
    assert_eq!(
        data["graphics_backend_options"],
        json!(["default", "opengl", "vulkan"])
    );
}

#[test]
fn write_from_flags_then_read_back() {
    let dir = TempDir::new("write");
    let id = write_profiles(dir.path(), "生存", "26.2-snapshot-2");
    write_version(dir.path(), "26.2-snapshot-2", true);

    let data = launch_args(
        &dir,
        &[
            &id,
            "--jvm=-XX:+UseG1GC",
            // 值以 `--` 开头时必须用 `--key=value` 形式（`--game --fullscreen` 会被解析器
            // 当成「缺少取值」——那是全项目 CLI 的既有口径，见 help 里的提示）。
            "--game=--fullscreen",
            "--memory",
            "6144",
            "--backend",
            "vulkan",
        ],
    );
    assert_eq!(data["updated"], true);
    assert_eq!(data["launch"]["jvm_args"], "-XX:+UseG1GC");
    assert_eq!(data["launch"]["memory_mb"], 6144);
    assert_eq!(data["launch"]["graphics_backend"], "vulkan");
    // 26.2-snapshot-2 过了门槛（判定来自 core 的能力位 + 版本门限）。
    assert_eq!(data["effective"]["graphics_backend_allowed"], true);

    // 再读一次：落盘之后依然在（`--json` 是事实上的对外契约）。
    let again = launch_args(&dir, &[&id]);
    assert_eq!(again["updated"], false);
    assert_eq!(again["launch"]["jvm_args"], "-XX:+UseG1GC");
    assert_eq!(again["launch"]["game_args"], "--fullscreen");
}

#[test]
fn clear_restores_follow_the_global() {
    let dir = TempDir::new("clear");
    let id = write_profiles(dir.path(), "生存", "1.20.4");
    launch_args(&dir, &[&id, "--memory", "4096", "--backend", "opengl"]);
    let cleared = launch_args(&dir, &[&id, "--clear"]);
    assert_eq!(cleared["launch"]["memory_mb"], Value::Null);
    assert_eq!(cleared["launch"]["graphics_backend"], "default");
    assert_eq!(cleared["effective"]["memory_mb"], 2048);
}

#[test]
fn invalid_values_fail_with_a_usage_error() {
    let dir = TempDir::new("invalid");
    let id = write_profiles(dir.path(), "生存", "1.20.4");
    let root = dir.arg();
    for (flag, value) in [
        ("--backend", "vulkan2"),
        ("--memory", "4g"),
        ("--memory", "1"),
    ] {
        let (code, stdout) = run_cli(&[
            "instances",
            "launch-args",
            "--json",
            "--data-dir",
            &root,
            &id,
            flag,
            value,
        ]);
        assert_ne!(code, 0, "{flag} {value} 应当失败：{stdout}");
        let value: Value = serde_json::from_str(&stdout).expect("输出不是 JSON");
        assert_eq!(value["error"]["code"], "usage", "{stdout}");
    }
    // 失败的写入不许留下半截设置文件。
    let layout = LauncherLayout::new(dir.path().join(LAUNCHER_HOME_DIR));
    assert!(
        unlucky_core::game::install::instance::read_instance_config(&layout, &id)
            .expect("读")
            .is_none(),
        "非法补丁不该落盘"
    );
}

#[test]
fn unknown_instance_is_a_user_error() {
    let dir = TempDir::new("missing");
    let root = dir.arg();
    let (code, stdout) = run_cli(&[
        "instances",
        "launch-args",
        "--json",
        "--data-dir",
        &root,
        "nope",
    ]);
    assert_ne!(code, 0);
    let value: Value = serde_json::from_str(&stdout).expect("输出不是 JSON");
    assert_eq!(value["error"]["code"], "not_found", "{stdout}");
}
