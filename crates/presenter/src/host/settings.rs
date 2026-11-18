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

use crate::dto::{SettingGroupView, SettingRowView};

use super::env::HostEnv;

// M193：下载组的行与「设置 → 引擎调优」的翻译搬进子模块（`settings.rs` 顶在 400 行硬线上，
// 理由与 `settings_tests.rs` 同源，见那个文件的文件头）。
pub(crate) mod download;

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

/// Java（D13/D23）：自动安装/内存/选路/发行版优先级。
fn java_group(env: &HostEnv) -> SettingGroupView {
    let memory = int(env, "java.default_memory_mb", 2048).to_string();
    let memory_options = ["1024", "2048", "4096", "8192"];
    let priority: Vec<String> = env
        .store()
        .effective("java.vendorPriority")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let priority_text = if priority.is_empty() {
        "地区默认序（Temurin → …）".to_string()
    } else {
        priority.join(" → ")
    };
    let roots = super::engine::java_scan_roots(env);
    SettingGroupView::new("java", "Java（D13）")
        .with(SettingRowView::toggle(
            "java.auto_install",
            "缺 Java 时自动下载",
            "按回退链装一个托管 JDK（基准 JDK17）",
            flag(env, "java.auto_install", true),
        ))
        .with(SettingRowView::choice(
            "java.default_memory_mb",
            "默认最大堆内存",
            "实例没单独指定时用这个值（MB）",
            &memory_options,
            selected(&memory_options, &memory),
        ))
        .with(SettingRowView::text(
            "java.selected_path",
            "指定 Java 可执行文件",
            "留空 = 自动挑（托管 > JAVA_HOME > PATH）",
            &text(env, "java.selected_path"),
        ))
        .with(SettingRowView::text(
            "java.installRoot",
            "托管 JDK 安装根",
            "留空 = 按数据目录模式派生（portable 进数据根 / standard 进系统目录）",
            &text(env, "java.installRoot"),
        ))
        .with(SettingRowView::text(
            "java.extra_args",
            "附加 JVM 参数",
            "原样拼到命令行尾部",
            &text(env, "java.extra_args"),
        ))
        .with(SettingRowView::read_only(
            "发行版优先级（java.vendorPriority）",
            &priority_text,
        ))
        .with(SettingRowView::read_only(
            "托管根",
            &roots
                .iter()
                .map(|root| root.display().to_string())
                .collect::<Vec<_>>()
                .join(" · "),
        ))
}

/// 数据目录（D6 三模式）。**M216 ①**：`core.data_custom_root` 读**写回的那份**（便携配置），否则
/// 改完自定义根页面读「解析后」那份、输入凭空消失；`core.data_mode` 仍显示生效模式。
fn data_group(env: &HostEnv) -> SettingGroupView {
    data_group_at(env, exe_dir().as_deref())
}

/// [`data_group`] 的参数化入口（`exe_dir` 决定自定义根那行读哪份配置；单测注入用）。
fn data_group_at(env: &HostEnv, exe_dir: Option<&Path>) -> SettingGroupView {
    let modes = ["portable", "standard", "custom"];
    let mode = env.mode().as_str().to_string();
    let custom_root = exe_dir
        .map(super::env::request_from_portable)
        .and_then(|request| request.custom_root)
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    SettingGroupView::new("data", "数据目录（D6 三模式）")
        .with(SettingRowView::choice(
            "core.data_mode",
            "存储模式",
            "便携（运行目录）/ 标准（系统应用数据目录）/ 自定义",
            &modes,
            selected(&modes, &mode),
        ))
        .with(SettingRowView::text(
            "core.data_custom_root",
            "自定义根目录",
            "custom 模式生效（绝对路径）",
            &custom_root,
        ))
        .with(SettingRowView::read_only(
            "启动器数据目录",
            &env.data.launcher_home.display().to_string(),
        ))
        .with(SettingRowView::read_only(
            "游戏目录",
            &env.data.game_home.display().to_string(),
        ))
        .with(SettingRowView::read_only(
            "生效模式",
            &format!("{}（改数据根需重启启动器）", env.mode()),
        ))
}

/// AI（D4）：开关/端点/模型/密钥状态/YOLO。
/// **选项表直接取 schema 的 `AI_PROVIDERS`**（M174）：原先手写三项、漏了 `disabled`，于是被别的
/// 写者设成 `disabled` 的机器在这一页会显示成 `openai_compatible`（`selected()` 找不到就回 0）。
/// **密钥只显示状态不显示值**：`ai.api_key` 是 `locked` 凭据，进 DTO 就等于进事件流/日志。
fn ai_group(env: &HostEnv) -> SettingGroupView {
    let providers = unlucky_core::setting::keys::AI_PROVIDERS;
    let provider = text(env, "ai.provider");
    let chosen = if provider.is_empty() {
        "openai_compatible"
    } else {
        provider.as_str()
    };
    let key_state = match text(env, "ai.api_key").trim().chars().count() {
        0 => "未配置（在 iced 设置页填写）".to_string(),
        chars => format!("已配置（{chars} 个字符）"),
    };
    SettingGroupView::new("ai", "AI 助手（D4）")
        .with(SettingRowView::toggle(
            "ai.enabled",
            "启用 AI 助手",
            "关闭后规则引擎诊断仍可用",
            flag(env, "ai.enabled", true),
        ))
        .with(SettingRowView::choice(
            "ai.provider",
            "模型端点",
            "OpenAI 兼容（含本地 llama.cpp）/ Anthropic / Ollama",
            providers,
            selected(providers, chosen),
        ))
        .with(SettingRowView::text(
            "ai.base_url",
            "端点 base_url",
            "如 http://127.0.0.1:8080/v1",
            &text(env, "ai.base_url"),
        ))
        .with(SettingRowView::text(
            "ai.model",
            "模型名",
            "如 deepseek-chat",
            &text(env, "ai.model"),
        ))
        .with(SettingRowView::read_only("API 密钥", &key_state))
        .with(SettingRowView::toggle(
            "ai.yolo",
            "YOLO 模式",
            "跳过确认弹窗（审计照记；账户/凭据操作永不 YOLO）",
            flag(env, "ai.yolo", false),
        ))
}

/// M39 的对外 MCP server（默认关、默认只读）。
fn mcp_group(env: &HostEnv) -> SettingGroupView {
    let port = int(env, "agent.mcp.port", 47_823);
    SettingGroupView::new("mcp", "MCP server（M39）")
        .with(SettingRowView::toggle(
            "agent.mcp.enabled",
            "对外提供 MCP server",
            "127.0.0.1 上的 Streamable HTTP（stdio 需显式启动）",
            flag(env, "agent.mcp.enabled", false),
        ))
        .with(SettingRowView::text(
            "agent.mcp.port",
            "端口",
            "1024~65535，只绑 127.0.0.1",
            &port.to_string(),
        ))
        .with(SettingRowView::toggle(
            "agent.mcp.allow_write",
            "允许外部客户端写配置",
            "默认只读；写仍受 schema 白名单与分级确认约束",
            flag(env, "agent.mcp.allow_write", false),
        ))
        .with(SettingRowView::read_only(
            "客户端地址",
            &format!("http://127.0.0.1:{port}/mcp（令牌见 agent.mcp.token，AI 不可写）"),
        ))
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

/// 设置页「重置发行版优先级」用（清空 enum_list）。
///
/// # Errors
///
/// 落盘失败时返回用户可读文案。
pub fn clear_vendor_priority(env: &HostEnv) -> Result<(), String> {
    let mut store = env.store();
    store
        .set("java.vendorPriority", Value::Array(Vec::new()))
        .and_then(|()| store.save())
        .map_err(|error| format!("清空发行版优先级失败：{error}"))?;
    Ok(())
}

/// **M216 ① 的回合用例**（不并进 `settings_tests.rs`：那是另一个文件，不在本条的文字范围里）。
#[cfg(test)]
mod data_mode_tests {
    use unlucky_core::platform::data_dir::{DataMode, ResolvedData};

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
