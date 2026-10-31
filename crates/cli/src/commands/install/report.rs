//! 安装结果的两种呈现形态：人类可读行 + `--json` 载荷，外加 stderr 实时进度。
//!
//! 拆出来是因为 `install` 的流程（规划 → 下载 → 登记）与排版彼此独立：
//! 前者要能读懂，后者要能被脚本断言。`--dry-run` 与真实安装共用同一份排版，
//! 差别只是多出逐文件清单与源选择。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use serde_json::json;
use unlucky_core::download::DownloadEngine;
use unlucky_core::game::install::{
    FileOutcome, InstallPlan, InstallReport, ProgressSink, Registration,
};

use crate::context::Ctx;
use crate::output::{progress_line, Report};

/// 每处理多少个文件打一行进度（3000 个资源下约 90 行，够用又不刷屏）。
const PROGRESS_EVERY: usize = 32;

/// 进度里最多逐条打印几条失败（其余交给最终错误汇总）。
const MAX_PROGRESS_FAILURES: usize = 5;

/// 实时进度回调（只写 stderr，不污染 `--json` 的 stdout）。
///
/// 两种形态：人类模式打文本行；`--json` 模式打 **JSONL**（一行一条事件，`event` 字段分流），
/// 这样脚本既能吃 stdout 的最终信封，也能实时跟进而不必解析文本。
///
/// 失败行只打前几条：整源不可达时会一口气失败上千个文件，全打出来既没用又淹没原因。
pub fn progress_sink(enabled: bool, json: bool, total: usize) -> Option<ProgressSink> {
    if !enabled {
        return None;
    }
    let done = Arc::new(AtomicUsize::new(0));
    let failures = Arc::new(AtomicUsize::new(0));
    Some(Arc::new(move |outcome: &FileOutcome| {
        let index = done.fetch_add(1, Ordering::Relaxed) + 1;
        if let Some(failure) = &outcome.failure {
            let seen = failures.fetch_add(1, Ordering::Relaxed);
            if seen < MAX_PROGRESS_FAILURES {
                emit(
                    json,
                    json!({
                        "event": "file_failed",
                        "done": index,
                        "total": total,
                        "name": outcome.name,
                        "role": outcome.role.as_str(),
                        "kind": failure.kind.as_str(),
                        "message": failure.message,
                    }),
                    &format!("失败 {index}/{total} {}：{}", outcome.name, failure.message),
                );
            } else if seen == MAX_PROGRESS_FAILURES {
                emit(
                    json,
                    json!({"event": "progress_truncated", "suppressed_after": MAX_PROGRESS_FAILURES}),
                    "（更多失败不再逐条打印，见最终错误汇总）",
                );
            }
            return;
        }
        if index.is_multiple_of(PROGRESS_EVERY) || index == total {
            emit(
                json,
                json!({"event": "progress", "done": index, "total": total}),
                &format!("下载 {index}/{total}"),
            );
        }
    }))
}

/// 一条进度：JSONL 模式打序列化后的一行，人类模式打短文本。
fn emit(json_mode: bool, record: serde_json::Value, text: &str) {
    if json_mode {
        progress_line(&record.to_string());
    } else {
        progress_line(text);
    }
}

/// 排版一次安装结果。
#[allow(clippy::too_many_arguments)]
pub fn build(
    ctx: &Ctx,
    engine: &DownloadEngine,
    now: u64,
    dry_run: bool,
    plan: &InstallPlan,
    report: Option<&InstallReport>,
    registration: Option<&Registration>,
    name: &str,
    version: &str,
) -> Report {
    let samples = if dry_run {
        plan.source_samples(engine, now)
    } else {
        Vec::new()
    };
    let mut data = json!({
        "dry_run": dry_run,
        "version_id": plan.version_id,
        "jar_id": plan.jar_id,
        "game_root": plan.game_root.display().to_string(),
        "plan": plan.summary_json(),
        "result": report.map(|report| json!({
            "downloaded": report.downloaded(),
            "skipped": report.skipped(),
            "failed": report.failed(),
            "bytes": report.bytes(),
            "version_json": report.version_json.as_ref().map(|path| path.display().to_string()),
            "warnings": report.warnings,
            "files": report.files.iter().map(outcome_json).collect::<Vec<_>>(),
        })),
        "instance": registration.map(|registration| json!({
            "id": registration.profile.id.as_str(),
            "name": registration.profile.name,
            "version_id": registration.profile.version_id,
            "created": registration.created,
            "config_path": registration.config_path.display().to_string(),
            "profiles_path": ctx.profiles_config().display().to_string(),
        })),
    });
    if dry_run {
        data["files"] = json!(plan.files_json());
        data["sources"] = json!(samples);
    }

    Report::new(
        data,
        human_lines(dry_run, plan, report, registration, &samples, name, version),
    )
}

/// 单个文件的执行结果（`--json` 的 `result.files`；脚本据此分流"哪几个没下成"）。
fn outcome_json(outcome: &FileOutcome) -> serde_json::Value {
    json!({
        "role": outcome.role.as_str(),
        "name": outcome.name,
        "dest": outcome.dest.display().to_string(),
        "status": outcome.status.as_str(),
        "bytes": outcome.bytes,
        "source": outcome.source,
        "error": outcome.failure.as_ref().map(|failure| json!({
            "kind": failure.kind.as_str(),
            "message": failure.message,
        })),
    })
}

fn human_lines(
    dry_run: bool,
    plan: &InstallPlan,
    report: Option<&InstallReport>,
    registration: Option<&Registration>,
    samples: &[serde_json::Value],
    name: &str,
    version: &str,
) -> Vec<String> {
    let mut human = Vec::new();
    let (total_bytes, unknown) = plan.total_bytes();
    human.push(if dry_run {
        format!("安装计划：{}（jar {}）", plan.version_id, plan.jar_id)
    } else {
        format!("已安装 {}（jar {}）", plan.version_id, plan.jar_id)
    });
    for (role, (count, bytes)) in plan.by_role() {
        human.push(format!(
            "  {:<8} {} 个 / {}",
            role.label(),
            count,
            human_bytes(bytes)
        ));
    }
    human.push(format!(
        "  合计 {} 个文件 / {}{}（游戏根 {}）",
        plan.file_count(),
        human_bytes(total_bytes),
        if unknown > 0 {
            format!(" + {unknown} 个大小未知")
        } else {
            String::new()
        },
        plan.game_root.display()
    ));
    for sample in samples {
        let sources: Vec<&str> = sample["candidates"]
            .as_array()
            .map(|list| {
                list.iter()
                    .filter_map(|candidate| candidate["source"].as_str())
                    .collect()
            })
            .unwrap_or_default();
        human.push(format!(
            "  源选择 {} → {}",
            sample["label"].as_str().unwrap_or_default(),
            sources.join(" → ")
        ));
    }
    for warning in &plan.warnings {
        human.push(format!("告警: {warning}"));
    }
    if let Some(report) = report {
        human.push(format!(
            "下载 {} 个 / 跳过 {} 个已就绪 / 失败 {} 个（本次 {}）",
            report.downloaded(),
            report.skipped(),
            report.failed(),
            human_bytes(report.bytes())
        ));
        if let Some(path) = &report.version_json {
            human.push(format!("版本清单: {}", path.display()));
        }
    }
    match registration {
        Some(registration) => human.push(format!(
            "已登记实例 {}（{}）→ 版本 {}（{}）",
            name,
            registration.profile.id.as_str(),
            version,
            if registration.created {
                "新建"
            } else {
                "已存在，复用"
            }
        )),
        None => human.push("（--dry-run：未下载任何文件，未登记实例）".to_string()),
    }
    human
}

/// 字节 → 人读（MB/GB 一位小数，KB 取整；不做假精度）。
pub fn human_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{} KB", bytes / KB)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::human_bytes;

    #[test]
    fn formats_sizes_without_fake_precision() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(1023), "1023 B");
        assert_eq!(human_bytes(1024), "1 KB");
        assert_eq!(human_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(human_bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
    }
}
