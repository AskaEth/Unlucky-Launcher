//! 设置页「Java」分组（M397 从 `host/settings.rs` 整段搬来，函数体逐字不动）。

use serde_json::Value;

use crate::dto::{SettingGroupView, SettingRowView};

use super::super::env::HostEnv;
use super::{flag, int, selected, text};

/// Java（D13/D23）：自动安装/内存/选路/发行版优先级。
pub(super) fn java_group(env: &HostEnv) -> SettingGroupView {
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
