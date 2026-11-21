//! 安装结果的两种呈现形态：人类可读行 + `--json` 载荷（stderr 实时进度见 [`super::progress`]）。
//!
//! 拆出来是因为 `install` 的流程（规划 → 下载 → 登记）与排版彼此独立：
//! 前者要能读懂，后者要能被脚本断言。`--dry-run` 与真实安装共用同一份排版，
//! 差别只是多出逐文件清单与源选择。

use serde_json::json;
use unlucky_core::download::DownloadEngine;
use unlucky_core::game::install::{FileOutcome, InstallPlan, InstallReport, Registration};

use crate::commands::pack::wording::{registered_line, DRY_RUN_UNREGISTERED};
use crate::context::Ctx;
use crate::output::Report;

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
    human.extend(sources_lines(samples));
    // **M216 P2-2**：有 report 就报**运行期**告警——它开头就是 `plan.warnings` 的副本，加上执行期
    // 才知道的事（「已替换版本清单 … 旧的那份备份在 …」）。此前这里只遍历 `plan.warnings`，
    // 于是「我们动了别人 `.minecraft` 里那份清单」这句话在 `ull install` 的人读输出里**从没出现过**
    // （只有 `--json` 的 `result.warnings` 有）——「不静默破坏」就落不了地。
    // 没有 report（`--dry-run`）时退回计划期告警。
    let warnings = report.map_or(plan.warnings.as_slice(), |report| {
        report.warnings.as_slice()
    });
    for warning in warnings {
        human.push(format!("告警: {warning}"));
    }
    if let Some(manifest) = report.and_then(|report| report.manifest.as_ref()) {
        if let Some(backup) = manifest.backup() {
            human.push(format!(
                "版本清单已替换（旧的那份备份在 {}）",
                backup.display()
            ));
        }
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
        Some(registration) => {
            // M346：与 `pack import` / `install --loader` / `instances adopt` 同一份实现
            // （`pack/wording.rs` 的 `registered_line`），前半句取目录 `PackReportRegistered`。
            human.push(registered_line(
                name,
                registration.profile.id.as_str(),
                version,
                registration.created,
            ))
        }
        None => human.push(DRY_RUN_UNREGISTERED.to_string()),
    }
    human
}

/// `--dry-run` 的源选择样本 → 人读行（`源选择 客户端 jar → bmclapi → official`）。
///
/// `install` 的两条呈现（原版 [`human_lines`] 与 [`super::report_loader`] 的加载器版）画的是同一份
/// 样本、同一种排版，M346 r2 顺手把这份手写收成一处（行为逐字未变：顺序、箭头、缩进都一样）。
pub(super) fn sources_lines(samples: &[serde_json::Value]) -> Vec<String> {
    samples
        .iter()
        .map(|sample| {
            let sources: Vec<&str> = sample["candidates"]
                .as_array()
                .map(|list| {
                    list.iter()
                        .filter_map(|candidate| candidate["source"].as_str())
                        .collect()
                })
                .unwrap_or_default();
            format!(
                "  源选择 {} → {}",
                sample["label"].as_str().unwrap_or_default(),
                sources.join(" → ")
            )
        })
        .collect()
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
#[path = "tests/report.rs"]
mod tests;
