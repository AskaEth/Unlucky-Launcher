//! `ull java vendors`：四家发行版的可用性 / 镜像 / 当前优先级顺序（D23）。
//!
//! 从 `commands/java.rs` 拆出来（那边 list/install 已经占满一屏）。这里只读不写：
//! 不联网、不改配置，把 `mirrors.json` 的表与设置里的优先级排成一张人读 + `--json` 的对照表。
//!
//! 存在的意义是**可观测**：用户问「国内为什么默认走 Oracle 而不是微软」时，命令要能直接答
//! （顺序 + 每家的镜像与实测吞吐），而不是让人翻文档。

use serde_json::{json, Value};
use unlucky_core::download::MirrorTable;
use unlucky_core::game::java::{vendor, JdkVendor};

use super::java::{region_profile, JavaSettings};
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::Report;

/// `ull java vendors`。
///
/// # Errors
///
/// 设置文件坏掉时退化为默认值；命令本身不产生用户错误。
pub fn vendors(ctx: &Ctx) -> Result<Report, CliError> {
    let settings = JavaSettings::load(ctx);
    let domestic = region_profile(ctx).is_domestic();
    let order = vendor::order(&settings.priority, domestic);
    let table = MirrorTable::builtin();
    let rows: Vec<Value> = order
        .iter()
        .enumerate()
        .map(|(index, item)| vendor_json(&table, *item, index))
        .collect();
    let priority_source = if settings.priority.is_empty() {
        "region_default"
    } else {
        "java.vendorPriority"
    };
    let data = json!({
        "region": if domestic { "domestic" } else { "global" },
        "priority_source": priority_source,
        "order": vendor::ids(&order),
        "vendors": rows,
    });
    let mut human = vec![format!(
        "发行版顺序（{}，来源 {}）：{}",
        if domestic { "国内" } else { "国外" },
        if settings.priority.is_empty() {
            "地区默认序"
        } else {
            "java.vendorPriority"
        },
        vendor::ids(&order).join(" → ")
    )];
    for row in &rows {
        human.push(format!(
            "  {} {}  {}  支持 Java {}",
            row["rank"].as_u64().unwrap_or(0),
            row["vendor"].as_str().unwrap_or("?"),
            row["display"].as_str().unwrap_or(""),
            join_numbers(&row["majors"], "/")
        ));
        for source in row["sources"].as_array().into_iter().flatten() {
            if let Some(text) = source.as_str() {
                human.push(format!("      源：{text}"));
            }
        }
    }
    Ok(Report::new(data, human))
}

/// 一家的 `--json` 形状（源清单按家取，缺表时为空数组而不是报错）。
fn vendor_json(table: &MirrorTable, item: JdkVendor, index: usize) -> Value {
    let sources: Vec<String> = match item {
        JdkVendor::Temurin => table
            .adoptium
            .iter()
            .filter(|mirror| mirror.enabled)
            .map(|mirror| {
                format!(
                    "{}{}",
                    mirror.name,
                    if mirror.latest_only {
                        "（只留最新补丁）"
                    } else {
                        ""
                    }
                )
            })
            .chain(std::iter::once(
                "GitHub 官方（国内套 gh-proxy）".to_string(),
            ))
            .collect(),
        JdkVendor::Oracle => table
            .oracle_mirrors()
            .map(|mirror| match mirror.speed.trim() {
                "" => format!("{} {}", mirror.name, mirror.base()),
                speed => format!("{} {} {}", mirror.name, mirror.base(), speed),
            })
            .collect(),
        JdkVendor::Microsoft => table
            .microsoft_openjdk()
            .map(|microsoft| {
                let speed = if microsoft.measured_speed.trim().is_empty() {
                    "实测约 400 KB/s".to_string()
                } else {
                    microsoft.measured_speed.clone()
                };
                vec![format!(
                    "aka.ms → VS CDN（{speed}）；无国内镜像、无 GitHub 资产"
                )]
            })
            .unwrap_or_default(),
        JdkVendor::Zulu => table
            .zulu()
            .map(|zulu| vec![format!("{}（{}）", zulu.api, zulu.measured_speed)])
            .unwrap_or_default(),
    };
    json!({
        "rank": index + 1,
        "vendor": item.id(),
        "display": item.display(),
        "majors": item.hint_majors(),
        "supported": (8..=99u32).filter(|major| item.supports(*major)).collect::<Vec<_>>(),
        "sources": sources,
    })
}

/// JSON 数字数组 → `21/17/11` 这种分隔串。
fn join_numbers(value: &Value, separator: &str) -> String {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_u64)
                .map(|number| number.to_string())
                .collect::<Vec<_>>()
                .join(separator)
        })
        .unwrap_or_default()
}
