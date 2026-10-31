//! 设置页分组：按 `core::setting` schema 的键生成行，写回走 `SettingStore::set`。
//!
//! 三件纪律（AGENTS §6）：
//! 1. 读写同一份 `<数据根>/config/launcher.json`（与 `ull config set` / AI `set_config` 同源）；
//! 2. 不自己拼 JSON，也不绕 schema（非法键/越界值由 `store.set` 拒绝）；
//! 3. 这里只认「键名 + 字符串值」，领域语义留在 core。
//!
//! 分组 id 与设置页子导航同名：`download` / `java` / `data` / `ai` / `mcp`。

use serde_json::Value;
use unlucky_core::game::java::JdkVendor;

use crate::dto::{SettingGroupView, SettingRowView};

use super::env::HostEnv;

/// 全部设置分组（设置页右栏一次拿全，切分组不再请求）。
pub fn groups(env: &HostEnv) -> Vec<SettingGroupView> {
    vec![
        download_group(env),
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

/// 下载（D7）：并发/重试/地区/镜像偏好/代理。
fn download_group(env: &HostEnv) -> SettingGroupView {
    let concurrency = int(env, "download.concurrency", 8).to_string();
    let retry = int(env, "download.retry_limit", 3).to_string();
    let concurrency_options = ["4", "8", "16"];
    let retry_options = ["0", "1", "3", "5"];
    let regions = ["auto", "cn", "global"];
    let mirrors = ["auto", "official", "mirror"];
    let proxies = ["none", "system", "manual"];
    let region_value = text(env, "download.region");
    let mirror_value = text(env, "download.mirror_preference");
    let proxy_value = text(env, "download.proxy_mode");
    SettingGroupView::new("download", "下载与镜像（D7）")
        .with(SettingRowView::choice(
            "download.concurrency",
            "并发数",
            "同时下载的文件数（海量小文件，16 路足够压满带宽）",
            &concurrency_options,
            selected(&concurrency_options, &concurrency),
        ))
        .with(SettingRowView::choice(
            "download.retry_limit",
            "单文件重试次数",
            "失败后换源重试的上限",
            &retry_options,
            selected(&retry_options, &retry),
        ))
        .with(SettingRowView::choice(
            "download.region",
            "地区判定",
            "auto = 时区+语言+探测；决定是否优先国内镜像",
            &regions,
            selected(
                &regions,
                if region_value.is_empty() {
                    "auto"
                } else {
                    &region_value
                },
            ),
        ))
        .with(SettingRowView::choice(
            "download.mirror_preference",
            "镜像偏好",
            "auto = 按健康度评分；official = 官方源优先",
            &mirrors,
            selected(
                &mirrors,
                if mirror_value.is_empty() {
                    "auto"
                } else {
                    &mirror_value
                },
            ),
        ))
        .with(SettingRowView::choice(
            "download.proxy_mode",
            "代理模式",
            "none / system（跟随系统）/ manual（下面填地址）",
            &proxies,
            selected(
                &proxies,
                if proxy_value.is_empty() {
                    "none"
                } else {
                    &proxy_value
                },
            ),
        ))
        .with(SettingRowView::text(
            "download.proxy_url",
            "HTTP 代理",
            "manual 模式生效；留空则用系统代理",
            &text(env, "download.proxy_url"),
        ))
        .with(SettingRowView::action(
            "download.action.detect_region",
            "重新检测地区",
            "清掉 24h 地区缓存，下次下载重新判定",
            "detect_region",
        ))
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

/// 数据目录（D6 三模式）。
fn data_group(env: &HostEnv) -> SettingGroupView {
    let modes = ["portable", "standard", "custom"];
    let mode = env.mode().as_str().to_string();
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
            &text(env, "core.data_custom_root"),
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

/// AI（D4）：开关/端点/模型/YOLO。
fn ai_group(env: &HostEnv) -> SettingGroupView {
    let providers = ["openai_compatible", "anthropic", "ollama"];
    let provider = text(env, "ai.provider");
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
            &providers,
            selected(
                &providers,
                if provider.is_empty() {
                    "openai_compatible"
                } else {
                    &provider
                },
            ),
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
///
/// 未知键、值类型不符、越界、落盘失败时返回用户可读文案。
pub fn set(env: &HostEnv, key: &str, value: &str) -> Result<(), String> {
    if key == "download.action.detect_region" {
        return detect_region(env);
    }
    let parsed = parse_value(key, value)?;
    let mut store = env.store();
    store
        .set(key, parsed)
        .map_err(|error| format!("设置 {key} 失败：{error}"))?;
    store
        .save()
        .map_err(|error| format!("保存设置失败：{error}"))?;
    Ok(())
}

/// 键 → 正确类型的 JSON 值（schema 会再校验范围/枚举）。
fn parse_value(key: &str, value: &str) -> Result<Value, String> {
    if matches!(key, "download.concurrency" | "download.retry_limit") {
        let number: u32 = value
            .trim()
            .parse()
            .map_err(|_| format!("{key} 需要整数：{value}"))?;
        return Ok(Value::from(number));
    }
    if matches!(key, "java.default_memory_mb" | "agent.mcp.port") {
        let number: u32 = value
            .trim()
            .parse()
            .map_err(|_| format!("{key} 需要整数：{value}"))?;
        return Ok(Value::from(number));
    }
    if matches!(
        key,
        "java.auto_install"
            | "ai.enabled"
            | "ai.yolo"
            | "agent.mcp.enabled"
            | "agent.mcp.allow_write"
    ) {
        let flag = match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "on" | "yes" => true,
            "0" | "false" | "off" | "no" => false,
            _ => return Err(format!("{key} 需要布尔值：{value}")),
        };
        return Ok(Value::Bool(flag));
    }
    Ok(Value::String(value.to_string()))
}

/// 清掉 24h 地区缓存（下次建引擎重新判定）。
fn detect_region(env: &HostEnv) -> Result<(), String> {
    let path = env.layout.cache().join("region.json");
    if path.is_file() {
        std::fs::remove_file(&path).map_err(|error| format!("清地区缓存失败：{error}"))?;
    }
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
    let _ = JdkVendor::ALL;
    Ok(())
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
