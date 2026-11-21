//! 设置页分组：按 `core::setting` schema 的键生成行，写回走 `SettingStore::set`。
//!
//! 三件纪律（AGENTS §6）：① 读写同一份 `<数据根>/config/launcher.json`（与 `ull config set` /
//! AI `set_config` 同源）；② 不自己拼 JSON、不绕 schema（非法键/越界值由 `store.set` 拒绝）；
//! ③ 这里只认「键名 + 字符串值」，领域语义留在 core。
//!
//! 分组 id 与设置页子导航同名：`download` / `java` / `data` / `ai` / `mcp`。

use std::path::{Path, PathBuf};

use serde_json::Value;
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::spec::ValueKind;
use unlucky_core::setting::store::SettingStore;

use crate::dto::SettingGroupView;

use self::{ai::ai_group, data::data_group, java::java_group, mcp::mcp_group};
use super::env::HostEnv;
// 搬进 `settings/*.rs` 的函数体里仍写着 `super::engine` / `super::env`（搬移前 `super` 正是 `host`）；
// 在这里把这两个名字绑一遍，搬移就只动 `mod` / `use`，函数体一个字符都不动（M397）。
use super::{engine, env};

// M193：下载组的行与「设置 → 引擎调优」的翻译搬进子模块（`settings.rs` 顶在 400 行硬线上，
// 理由与 `settings_tests.rs` 同源，见那个文件的文件头）。
pub(crate) mod download;
// M397：其余四组的 builder 同样按组下沉，本文件只留 facade（`groups` / `set`）与读写原语。
mod ai;
mod data;
mod java;
mod mcp;

/// 全部设置分组（设置页右栏一次拿全，切分组不再请求）。
pub fn groups(env: &HostEnv) -> Vec<SettingGroupView> {
    vec![
        download::group(env),
        java_group(env),
        data_group(env),
        ai_group(env),
        mcp_group(env),
    ]
}

/// 读一个键的字符串值（缺省给空串）。
fn text(env: &HostEnv, key: &str) -> String {
    env.store()
        .doc()
        .effective_str(key)
        .unwrap_or_default()
        .to_string()
}

/// 读一个键的整数（缺省给 `fallback`）。
fn int(env: &HostEnv, key: &str, fallback: i64) -> i64 {
    env.store()
        .effective(key)
        .and_then(Value::as_i64)
        .unwrap_or(fallback)
}

/// 读一个键的布尔（缺省给 `fallback`）。
fn flag(env: &HostEnv, key: &str, fallback: bool) -> bool {
    env.store()
        .effective(key)
        .and_then(Value::as_bool)
        .unwrap_or(fallback)
}

/// 枚举值在选项表里的序号（找不到给 0）。
fn selected(options: &[&str], value: &str) -> i32 {
    options
        .iter()
        .position(|option| *option == value)
        .map(|index| i32::try_from(index).unwrap_or(0))
        .unwrap_or(0)
}

/// 写一条设置。`value` 是字符串形态，按 schema 键的类型转换后交 `store.set`。
///
/// # Errors
/// 未知键、值类型不符、越界、落盘失败时返回用户可读文案。
pub fn set(env: &HostEnv, key: &str, value: &str) -> Result<(), String> {
    set_at(env, exe_dir().as_deref(), key, value)
}

/// [`set`] 的参数化入口（`exe_dir` 决定数据根四键写哪份；单测注入假 exe 目录）。**M216 ①**：
/// 老实现写 `env.store()`（解析后那份），而启动只读便携那份 ⇒ 改「存储模式」下次启动看不见。
fn set_at(env: &HostEnv, exe_dir: Option<&Path>, key: &str, value: &str) -> Result<(), String> {
    if key == super::engine::DETECT_REGION_ACTION {
        return detect_region(env);
    }
    // M204：Java 组的「写回完成，重发快照」动作键。**不写盘**——页面那一侧已把三键一次原子写进
    // `launcher.json`（`pages/settings/java/store.rs::save`），这里返回 `Ok(())` 就能让
    // `dispatch.rs` 重发 `SettingsChanged`。键的取舍见 `crate::dto::JAVA_REFRESH_ACTION`。
    if key == crate::dto::JAVA_REFRESH_ACTION {
        return Ok(());
    }
    let parsed = parse_value(key, value)?;
    let path = crate::settings_bridge::setting_path(&env.launcher_config(), exe_dir, key);
    let mut store = SettingStore::load(&path, launcher_schema());
    store
        .set(key, parsed)
        .map_err(|error| format!("设置 {key} 失败：{error}"))?;
    store
        .save()
        .map_err(|error| format!("保存设置失败：{error}"))?;
    Ok(())
}

/// 本进程的 exe 目录（拿不到就退回 `None`：落在解析后那份，见 `setting_path`）。
fn exe_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.parent().map(Path::to_path_buf)
}

/// 键 → 正确类型的 JSON 值（schema 会再校验范围/枚举）。
///
/// **按 schema 的 `ValueKind` 取类型，不硬编码键名**（复审 P1-1）：硬编码会让 Float 键被当 String
/// 送进 `store.set` ⇒ `InvalidValue` ⇒ `save()` 不执行。守门：`every_float_key_in_the_schema_can_be_written_back`。
fn parse_value(key: &str, value: &str) -> Result<Value, String> {
    let schema = unlucky_core::setting::keys::launcher_schema();
    let Some(spec) = schema.spec(key) else {
        // 未知键：原样交下去，由 `store.set` 用 schema 报「未知键」（这里不猜类型）。
        return Ok(Value::String(value.to_string()));
    };

    let trimmed = value.trim();
    match &spec.kind {
        ValueKind::Bool => match trimmed.to_ascii_lowercase().as_str() {
            "1" | "true" | "on" | "yes" => Ok(Value::Bool(true)),
            "0" | "false" | "off" | "no" => Ok(Value::Bool(false)),
            _ => Err(format!("{key} 需要布尔值：{value}")),
        },
        ValueKind::Int { .. } => trimmed
            .parse::<i64>()
            .map(Value::from)
            .map_err(|_| format!("{key} 需要整数：{value}")),
        ValueKind::Float { .. } => match trimmed.parse::<f64>() {
            Ok(number) if number.is_finite() => Ok(Value::from(number)),
            _ => Err(format!("{key} 需要有限数字：{value}")),
        },
        // 文本 / 枚举 / 路径 / 列表 / 任意 JSON：字符串原样交下去（越界与词表由 schema 判）。
        _ => Ok(Value::String(value.to_string())),
    }
}

/// 清掉 24h 地区缓存并让共享引擎失效（下次请求重新判定；自更新那份由 `dispatch` 重建 Factory）。
fn detect_region(env: &HostEnv) -> Result<(), String> {
    let path = env.layout.cache().join("region.json");
    if path.is_file() {
        std::fs::remove_file(&path).map_err(|error| format!("清地区缓存失败：{error}"))?;
    }
    super::engine::forget(env);
    Ok(())
}

/// **M216 ① 的回合用例**（不并进 `settings_tests.rs`：那是另一个文件，不在本条的文字范围里）。
#[cfg(test)]
mod data_mode_tests {
    use unlucky_core::platform::data_dir::{DataMode, ResolvedData};

    use super::data::data_group_at;
    use super::*;

    /// 写回与启动解析同源：切到 standard 再切回 portable，**两次都真的生效**；页面显示同源。
    /// 抓得住：四键的落点（写进「解析后」那份 ⇒ 启动读到旧值 ⇒ 红）、不写副本、其余键不改道。
    /// 抓不住：真实 `current_exe()`、`redirect.json` 跟随、GUI 派发。
    #[test]
    fn data_mode_written_back_is_the_one_the_next_start_reads() {
        let root = std::env::temp_dir().join(format!("ull216-{}-{}", std::process::id(), line!()));
        let exe_dir = root.join("exe");
        std::fs::create_dir_all(exe_dir.join(".ULL/config")).expect("建目录");
        let portable = crate::host::env::portable_config_path(&exe_dir);
        std::fs::write(&portable, r#"{"core":{"data_mode":"portable"}}"#).expect("写便携配置");
        // 「以为自己在 Standard 模式」：解析后的数据根在别处。
        let running = HostEnv::from_data(ResolvedData {
            mode: DataMode::Standard,
            launcher_home: root.join("appdata/ULL"),
            game_home: root.join("appdata/.minecraft"),
            redirected_from: None,
        });
        let write = |mode: &str| set_at(&running, Some(&exe_dir), "core.data_mode", mode);
        let write_key = |key: &str, value: &str| set_at(&running, Some(&exe_dir), key, value);
        let at_startup = || crate::host::env::request_from_portable(&exe_dir).mode;
        assert_eq!(at_startup(), DataMode::Portable, "起点");
        assert_eq!(write("standard"), Ok(()));
        assert_eq!(at_startup(), DataMode::Standard, "切出后启动读得到");
        assert_eq!(write("portable"), Ok(()));
        assert_eq!(at_startup(), DataMode::Portable, "切回也一样");
        assert!(!running.launcher_config().is_file(), "四键不写副本");
        assert_eq!(write_key("java.default_memory_mb", "4096"), Ok(()));
        let text = std::fs::read_to_string(running.launcher_config()).expect("读配置");
        assert!(text.contains("4096"), "非数据根键仍落解析后那份：{text}");

        let shared = root.join("shared").display().to_string();
        assert_eq!(write_key("core.data_custom_root", &shared), Ok(()));
        let group = data_group_at(&running, Some(&exe_dir));
        let row = group.rows.iter().find(|r| r.key == "core.data_custom_root");
        assert_eq!(row.expect("有这一行").value_text, shared, "显示同源");
        std::fs::remove_dir_all(&root).ok();
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
