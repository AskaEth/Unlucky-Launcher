//! [`super`] 的单测：数据根键与其余键的落点分流、dump 合并、`problems` 通道。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use super::{dump, get, set};
use crate::cli::Globals;
use crate::context::Ctx;

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
            "ull-config-test-{tag}-{}-{seq}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("建临时目录");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 正确的 `$schema`（写成真前缀，加载才不会降级）。
fn schema_url() -> String {
    use unlucky_core::setting::keys::launcher_version;
    use unlucky_core::setting::schema::{SchemaId, SchemaRef};
    SchemaRef::new(SchemaId::LAUNCHER, launcher_version()).url()
}

fn write_json(path: &Path, value: &Value) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("建目录");
    }
    std::fs::write(path, value.to_string()).expect("写配置");
}

fn read_json(path: &Path) -> Value {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("读 {} 失败：{e}", path.display()));
    serde_json::from_slice(&bytes).expect("合法 JSON")
}

fn ctx_with(globals: Globals, exe_dir: &Path) -> Ctx {
    Ctx::resolve_in(&globals, Some(exe_dir.to_path_buf())).expect("解析数据根")
}

/// 便携模式：两个落点是同一份文件，`core.data_mode` 改了立刻是生效值。
#[test]
fn portable_mode_keeps_core_keys_in_one_file() {
    let temp = TempDir::new("portable");
    let ctx = ctx_with(Globals::default(), temp.path());
    assert_eq!(ctx.data.mode.as_str(), "portable");
    assert_eq!(
        ctx.setting_path_for("core.data_mode"),
        ctx.launcher_config(),
        "便携模式下两份路径本来就该重合"
    );

    set(&ctx, "core.data_mode", "standard").expect("写 core.data_mode");
    let file = temp
        .path()
        .join(".ULL")
        .join("config")
        .join("launcher.json");
    assert!(file.is_file(), "应落在 {}", file.display());
    assert_eq!(read_json(&file)["core"]["data_mode"], json!("standard"));

    let report = get(&ctx, Some("core.data_mode"), true).expect("读回来");
    // `core.*` 在 schema 里标了 `AiPolicy::sensitive()`（High），读路径默认掩码成
    // `<redacted>`，所以要看生效值得像用户那样加 `--show-secret`。
    assert_eq!(report.data["value"], json!("standard"));
    assert_eq!(
        report.data["path"],
        json!(file.display().to_string()),
        "read 与 write 必须同一份"
    );
}

/// standard 模式：数据根键走便携目录那份（与解析同源），其余键走数据根。
#[test]
fn standard_mode_routes_core_keys_to_the_portable_file() {
    let temp = TempDir::new("standard");
    let exe = temp.path().join("exe");
    let portable = exe.join(".ULL").join("config").join("launcher.json");
    let launcher_home = temp.path().join("lhome");
    write_json(
        &portable,
        &json!({
            "$schema": schema_url(),
            "schemaVersion": 1,
            "core": {
                "data_mode": "standard",
                "launcher_home": launcher_home.display().to_string(),
            },
        }),
    );

    let ctx = ctx_with(Globals::default(), &exe);
    assert_eq!(ctx.data.mode.as_str(), "standard");
    assert_eq!(ctx.data.launcher_home, launcher_home);
    assert_ne!(
        ctx.setting_path_for("core.data_mode"),
        ctx.launcher_config()
    );

    let report = get(&ctx, Some("core.data_mode"), true).expect("读 core.data_mode");
    assert_eq!(
        report.data["value"],
        json!("standard"),
        "get 展示的必须是真正生效的值"
    );
    assert_eq!(report.data["path"], json!(portable.display().to_string()));

    // 普通键仍写数据根。
    set(&ctx, "download.concurrency", "16").expect("写 download.concurrency");
    let core_file = ctx.launcher_config();
    assert_eq!(read_json(&core_file)["download"]["concurrency"], json!(16));

    // 数据根键写便携目录那份，且不动数据根那份的 core.*。
    set(&ctx, "core.data_mode", "portable").expect("写 core.data_mode");
    assert_eq!(read_json(&portable)["core"]["data_mode"], json!("portable"));
    assert!(
        read_json(&core_file)["core"].get("data_mode").is_none(),
        "数据根那份不该被写入 core.data_mode"
    );
}

/// `--data-dir` 模式：数据根由命令行钉死，core.* 仍落便携目录那份并给出提示。
#[test]
fn data_dir_flag_routes_core_keys_to_portable_and_says_so() {
    let temp = TempDir::new("datadir");
    let exe = temp.path().join("exe");
    std::fs::create_dir_all(&exe).expect("建 exe 目录");
    let data = temp.path().join("data");
    let globals = Globals {
        data_dir: Some(data.clone()),
        ..Globals::default()
    };
    let ctx = ctx_with(globals, &exe);
    assert_eq!(ctx.data.mode.as_str(), "custom");
    assert_eq!(ctx.data.launcher_home, data.join(".ULL"));

    let portable = exe.join(".ULL").join("config").join("launcher.json");
    set(&ctx, "core.data_mode", "standard").expect("写 core.data_mode");
    assert!(portable.is_file(), "应落在 {}", portable.display());
    assert!(!data
        .join(".ULL")
        .join("config")
        .join("launcher.json")
        .exists());

    let report = get(&ctx, Some("core.data_mode"), true).expect("读回来");
    assert_eq!(report.data["value"], json!("standard"));
    assert!(
        report.human.iter().any(|line| line.contains("--data-dir")),
        "{:?}",
        report.human
    );
}

/// 全量 dump：`core.*` 取便携目录那份的生效值，并标明两份文件与数据根。
#[test]
fn dump_merges_core_keys_from_the_portable_file() {
    let temp = TempDir::new("dump");
    let exe = temp.path().join("exe");
    let portable = exe.join(".ULL").join("config").join("launcher.json");
    let launcher_home = temp.path().join("home2");
    write_json(
        &portable,
        &json!({
            "$schema": schema_url(),
            "schemaVersion": 1,
            "core": {
                "data_mode": "standard",
                "launcher_home": launcher_home.display().to_string(),
            },
        }),
    );
    // 数据根那份写一个普通键（默认值不落盘，dump 里只有文件里真有的键）。
    write_json(
        &launcher_home.join("config").join("launcher.json"),
        &json!({
            "$schema": schema_url(),
            "schemaVersion": 1,
            "download": { "concurrency": 4 },
        }),
    );
    let ctx = ctx_with(Globals::default(), &exe);

    let report = dump(&ctx, true).expect("全量 dump");
    assert_eq!(
        report.data["config"]["core"]["data_mode"],
        json!("standard")
    );
    assert_eq!(
        report.data["core_path"],
        json!(portable.display().to_string())
    );
    assert_eq!(report.data["data_root"]["mode"], json!("standard"));
    assert_eq!(
        report.data["config"]["download"]["concurrency"],
        json!(4),
        "非 core.* 键仍取数据根那份"
    );
}

/// 认不出的 `$schema` 前缀：值照读 + `problems` 里有 UnknownSchema，不再静默回落。
#[test]
fn unknown_schema_prefix_warns_and_still_shows_file_values() {
    let temp = TempDir::new("badschema");
    let exe = temp.path().join("exe");
    let portable = exe.join(".ULL").join("config").join("launcher.json");
    write_json(
        &portable,
        &json!({
            "$schema": "https://unlucky-launcher.local/schema/launcher/1.2.0.json",
            "schemaVersion": 1,
            "core": { "data_mode": "portable" },
            "download": { "concurrency": 32 },
        }),
    );
    let ctx = ctx_with(Globals::default(), &exe);

    let report = get(&ctx, Some("download.concurrency"), false).expect("读普通键");
    assert_eq!(report.data["value"], json!(32), "值不该被回落成默认 8");
    let problems = report.data["problems"].as_array().expect("problems 数组");
    assert!(
        problems
            .iter()
            .any(|problem| problem.as_str().is_some_and(|p| p.contains("$schema"))),
        "{problems:?}"
    );
    assert!(
        report.human.iter().any(|line| line.contains("$schema")),
        "{:?}",
        report.human
    );
}

/// M161：`ull config get update.auto_check` 读的是**主配置**，而且会把 M159 遗留的
/// `cache/update/prefs.json` 先迁进来（迁移收据在 `migration` 里可查）。
///
/// 这条是 mission「`ull config get update.auto_check` 真能读到」的端到端证明：
/// 旧文件里 `false` → 迁移 → 主配置 → CLI 输出 `false`（**不是**默认的 `true`）。
#[test]
fn the_update_switch_is_migrated_and_readable_from_the_cli() {
    let temp = TempDir::new("update-prefs");
    let exe = temp.path().join("exe");
    let ctx = ctx_with(Globals::default(), &exe);

    // 旧域内文件（M159 的落点）：开关是关的。
    let legacy = ctx.layout().cache().join("update").join("prefs.json");
    write_json(&legacy, &json!({ "auto_check": false }));

    let report = get(&ctx, Some("update.auto_check"), false).expect("读 update.auto_check");
    assert_eq!(
        report.data["value"],
        json!(false),
        "旧文件里的 false 必须读到（默认值是 true，读成默认就是丢值）"
    );
    assert_eq!(
        report.data["path"],
        json!(ctx.launcher_config().display().to_string()),
        "落点必须是主配置（schema API），不是更新域自己的文件"
    );
    assert_eq!(
        report.data["migration"]["id"],
        json!("migrated"),
        "迁移要走一遍并如实报出来：{report:?}"
    );
    let receipt = report.data["migration"]["receipt"]
        .as_str()
        .expect("收据路径")
        .to_string();
    assert!(Path::new(&receipt).is_file(), "收据可查：{receipt}");
    assert!(!legacy.exists(), "旧文件必须退役");
    assert!(
        report
            .human
            .iter()
            .any(|line| line.contains("已把旧更新偏好迁入主配置")),
        "{:?}",
        report.human
    );

    // 第二遍：什么都不做（幂等），值仍是 false。
    let again = get(&ctx, Some("update.auto_check"), false).expect("再读一遍");
    assert_eq!(again.data["value"], json!(false));
    assert!(
        again.data.get("migration").is_none(),
        "第二遍不该再报迁移：{again:?}"
    );

    // `channels` 里只有 stable 时，写 beta 要当面被拒（而不是静默写成「设了没生效」）。
    let error = set(&ctx, "update.channel", "beta").expect_err("beta 不在允许值里");
    assert!(error.to_string().contains("beta"), "{error}");

    // 允许值写得进去，且读得回来。
    set(&ctx, "update.channel", "stable").expect("写 stable");
    let channel = get(&ctx, Some("update.channel"), false).expect("读 update.channel");
    assert_eq!(channel.data["value"], json!("stable"));
}
