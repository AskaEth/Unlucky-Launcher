//! [`super`] 的单测：数据根键与其余键的落点分流、AI 侧写入真的生效。
//!
//! 与 `commands/config/tests.rs` 同一套造场手法：临时目录 + `Ctx::resolve_in`
//! 把便携目录钉死在临时路径里，不动真实数据根。

use std::sync::atomic::{AtomicU64, Ordering};

use super::*;
use crate::cli::Globals;
use crate::commands::agent::ports::build;
use crate::context::Ctx;

fn temp_config() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ull-core-setting-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("建临时目录");
    dir.join("launcher.json")
}

#[test]
fn schema_omits_credential_keys_and_writable_keys_match_policy() {
    let port = CoreSetting::new(temp_config());
    let schema = port.schema();
    let text = schema.to_string();
    assert!(!text.contains("ai.api_key"), "凭据键不得进 AI schema");
    assert!(text.contains("agent.mcp.port"), "M39 的新键应可写：{text}");

    let paths: Vec<String> = port.writable_keys().into_iter().map(|k| k.path).collect();
    assert!(paths.contains(&"download.concurrency".to_string()));
    assert!(!paths.contains(&"ai.api_key".to_string()));
}

#[test]
fn summary_masks_high_sensitivity_but_keeps_low() {
    let port = CoreSetting::new(temp_config());
    let summary = port.summary();
    assert_eq!(summary["ai.base_url"], json!(MASK), "High 键要掩码");
    assert_ne!(
        summary["download.concurrency"],
        json!(MASK),
        "Low 键不必掩码"
    );
}

#[test]
fn credential_key_get_is_refused() {
    let port = CoreSetting::new(temp_config());
    let err = port.get("ai.api_key").unwrap_err();
    assert!(err.contains("凭据类配置"), "{err}");
}

#[test]
fn set_then_get_round_trips_a_low_key() {
    let path = temp_config();
    let port = CoreSetting::new(path.clone());
    let change = port
        .set("download.concurrency", &json!(16))
        .expect("应写入");
    assert_eq!(change.new, json!(16));
    assert_eq!(port.get("download.concurrency").unwrap(), json!(16));
    assert!(path.is_file(), "应落盘 {}", path.display());
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

/// 一次性临时目录（`Drop` 时清理；cli 没有 dev-dependency 的 tempfile）。
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("ull-ai-setting-{tag}-{}-{seq}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
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

/// 正确的 `$schema`（认不出的前缀会让加载降级成只读）。
fn schema_url() -> String {
    use unlucky_core::setting::keys::launcher_version;
    use unlucky_core::setting::schema::{SchemaId, SchemaRef};
    SchemaRef::new(SchemaId::LAUNCHER, launcher_version()).url()
}

fn write_json(path: &Path, value: &Value) {
    std::fs::create_dir_all(path.parent().expect("有父目录")).expect("建目录");
    std::fs::write(path, value.to_string()).expect("写配置");
}

fn read_json(path: &Path) -> Value {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("读 {} 失败：{e}", path.display()));
    serde_json::from_slice(&bytes).expect("合法 JSON")
}

/// standard 模式现场：便携目录那份说 standard + 自定义 launcher home（数据根留在临时目录里）。
fn standard_fixture(temp: &TempDir) -> (PathBuf, PathBuf, PathBuf) {
    let exe = temp.path().join("exe");
    let launcher_home = temp.path().join("lhome");
    let portable = exe.join(".ULL").join("config").join("launcher.json");
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
    (exe, portable, launcher_home)
}

/// **AI 面改 `core.data_mode` 真的生效**：写便携目录那份，下一次解析换模式。
///
/// 走的是 `ports::build` 装出来的同一个端口（`set_config` 工具背后就是它）。
#[test]
fn ai_set_of_core_data_mode_takes_effect_on_the_next_resolve() {
    let temp = TempDir::new("core-ai");
    let (exe, portable, launcher_home) = standard_fixture(&temp);
    let ctx = Ctx::resolve_in(&Globals::default(), Some(exe.clone())).expect("解析数据根");
    assert_eq!(ctx.data.mode.as_str(), "standard");
    let ports = build(&ctx);

    ports
        .setting
        .set("core.data_mode", &json!("portable"))
        .expect("AI 侧写 core.data_mode");

    assert_eq!(
        read_json(&portable)["core"]["data_mode"],
        json!("portable"),
        "数据根键要落在便携目录那份"
    );
    let data_file = launcher_home.join("config").join("launcher.json");
    assert!(
        !data_file.exists() || read_json(&data_file)["core"].get("data_mode").is_none(),
        "数据根那份不该被写入 core.data_mode"
    );

    let next = Ctx::resolve_in(&Globals::default(), Some(exe)).expect("重新解析");
    assert_eq!(
        next.data.mode.as_str(),
        "portable",
        "改了没反应到此为止：下一次解析必须换模式"
    );

    // 其余键仍走数据根那份，且不污染便携目录那份。
    ports
        .setting
        .set("download.concurrency", &json!(16))
        .expect("AI 侧写普通键");
    assert_eq!(read_json(&data_file)["download"]["concurrency"], json!(16));
    assert!(
        read_json(&portable)["download"]
            .get("concurrency")
            .is_none(),
        "普通键不该写进便携目录那份"
    );
}

/// 概要的取值来源与解析同源：数据根键取便携那份（哪怕数据根那份留着陈旧值）。
#[test]
fn summary_takes_data_root_keys_from_the_portable_file() {
    let temp = TempDir::new("core-summary");
    let (exe, portable, launcher_home) = standard_fixture(&temp);
    // 老版本 AI 写错文件留下的现场：数据根那份有个陈旧的 core.data_mode。
    let data_file = launcher_home.join("config").join("launcher.json");
    write_json(
        &data_file,
        &json!({
            "$schema": schema_url(),
            "schemaVersion": 1,
            "core": { "data_mode": "custom" },
            "download": { "concurrency": 4 },
        }),
    );

    let ctx = Ctx::resolve_in(&Globals::default(), Some(exe)).expect("解析数据根");
    let port = CoreSetting::split(ctx.launcher_config(), ctx.portable_launcher_config());
    let data = port.store();
    let core = SettingStore::load(&portable, launcher_schema());

    assert_eq!(
        raw_value(&data, Some(&core), "core.data_mode"),
        Some(&json!("standard")),
        "数据根键取便携那份 = 解析读到的那份"
    );
    assert_eq!(
        raw_value(&data, Some(&core), "download.concurrency"),
        Some(&json!(4)),
        "其余键仍取数据根那份"
    );
    // `core.*` 是 M12 的 High 键，概要里照旧掩码（值不可见，但来源必须对）。
    assert_eq!(port.summary()["core.data_mode"], json!(MASK));
}

/// 便携模式下两份落点本就是同一份文件：不留第二份，免得重复加载。
#[test]
fn portable_mode_keeps_a_single_landing_file() {
    let temp = TempDir::new("core-portable");
    let ctx =
        Ctx::resolve_in(&Globals::default(), Some(temp.path().to_path_buf())).expect("解析数据根");
    assert_eq!(ctx.data.mode.as_str(), "portable");
    let port = CoreSetting::split(ctx.launcher_config(), ctx.portable_launcher_config());
    assert_eq!(
        port.path_for("core.data_mode"),
        ctx.launcher_config().as_path()
    );
}
