//! 加载器安装的呈现形态（人类可读行 + `--json` 载荷）。
//!
//! 与 [`super::report`] 的分工：那边是"原版安装"（客户端 jar + 资源 + 库），这边是
//! "叠在原版上的 loader 层"（只有库）。共用同一套进度回调与字节格式化。

use serde_json::{json, Value};
use unlucky_core::addon::loader::LoaderInstallPlan;
use unlucky_core::download::DownloadEngine;
use unlucky_core::game::install::{InstallReport, Registration};

use super::report::{human_bytes, sources_lines};
use crate::commands::pack::wording::{registered_line, DRY_RUN_UNREGISTERED};
use crate::context::Ctx;
use crate::output::Report;

use processors::{processor_json, processor_lines};

/// processors 段（M35）：类型与两块渲染在 [`processors`]（M346 r3 拆出，纯搬移）。
#[path = "report_loader/processors.rs"]
mod processors;

pub use processors::ProcessorSection;

/// 排版一次加载器安装结果。
///
/// 参数多是因为载荷本身就多：计划（loader 版本 + 库）、processors 段、执行结果、实例登记、
/// `--dry-run` 的源选择样本。拆成结构体会让调用方多写十行，不如保持扁平。
#[allow(clippy::too_many_arguments)]
pub fn build(
    ctx: &Ctx,
    engine: &DownloadEngine,
    now: u64,
    dry_run: bool,
    plan: &LoaderInstallPlan,
    report: Option<&InstallReport>,
    vanilla: Option<&InstallReport>,
    registration: Option<&Registration>,
    name: &str,
    processors: Option<ProcessorSection<'_>>,
) -> Report {
    let samples = if dry_run {
        plan.install.source_samples(engine, now)
    } else {
        Vec::new()
    };
    let mut data = json!({
        "dry_run": dry_run,
        "loader": plan.release.to_json(),
        "vanilla_install_needed": plan.vanilla_install_needed,
        "game_root": plan.install.game_root.display().to_string(),
        "plan": plan.summary_json(),
        "result": report.map(|report| json!({
            "downloaded": report.downloaded(),
            "skipped": report.skipped(),
            "failed": report.failed(),
            "bytes": report.bytes(),
            "version_json": report.version_json.as_ref().map(|path| path.display().to_string()),
            "warnings": report.warnings,
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
        data["files"] = json!(plan.install.files_json());
        data["sources"] = json!(samples);
    }
    if let Some(section) = &processors {
        data["processors"] = processor_json(section);
    }
    if let Some(block) = vanilla_json(vanilla) {
        data["vanilla"] = block;
    }

    Report::new(
        data,
        human_lines(
            dry_run,
            plan,
            report,
            vanilla,
            registration,
            &samples,
            name,
            processors.as_ref(),
        ),
    )
}

/// `--json` 的 `vanilla` 段：先装原版那一步的告警 + 清单结局（`None` = 这次没先装原版）。
///
/// **M216 r2 P2-2**：原版 report 在 `is_complete()` 之后就被丢了 ⇒ 「替换了共享 `.minecraft`
/// 里的原版清单」在加载器安装的 JSON 里没有痕迹。
fn vanilla_json(vanilla: Option<&InstallReport>) -> Option<Value> {
    let report = vanilla?;
    Some(json!({
        "warnings": report.warnings,
        "version_json": report.version_json.as_ref().map(|path| path.display().to_string()),
        "manifest": report.manifest.as_ref().map(|write| json!({
            "kind": write.as_str(),
            "backup": write.backup().map(|path| path.display().to_string()),
        })),
    }))
}

/// 人读行里「先装原版」那一段（**M216 r2 P2-2**，与 [`vanilla_json`] 同源）：以前
/// `human_lines` 只遍历 `plan.warnings()`，原版那份 report 的告警与「清单被替换」都没说。
fn vanilla_lines(vanilla: Option<&InstallReport>) -> Vec<String> {
    let Some(report) = vanilla else {
        return Vec::new();
    };
    let mut lines: Vec<String> = report
        .warnings
        .iter()
        .map(|warning| format!("告警（原版）: {warning}"))
        .collect();
    if let Some(backup) = report.manifest.as_ref().and_then(|write| write.backup()) {
        lines.push(format!(
            "原版版本清单已替换（旧的那份备份在 {}）",
            backup.display()
        ));
    }
    lines
}

fn human_lines(
    dry_run: bool,
    plan: &LoaderInstallPlan,
    report: Option<&InstallReport>,
    vanilla: Option<&InstallReport>,
    registration: Option<&Registration>,
    samples: &[serde_json::Value],
    name: &str,
    processors: Option<&ProcessorSection<'_>>,
) -> Vec<String> {
    let release = &plan.release;
    let mut human = Vec::new();
    human.push(format!(
        "{} {}《{} {}》（继承原版 {}，主类 {}）",
        if dry_run {
            "加载器安装计划"
        } else {
            "已安装加载器"
        },
        release.kind.label(),
        release.mc_version,
        release.loader_version,
        release.mc_version,
        release.version_json.main_class.as_deref().unwrap_or("?"),
    ));
    human.push(format!(
        "  版本 id {} → {}",
        release.version_id,
        plan.install.version_json_path.display()
    ));
    human.push(format!(
        "  原版 {}",
        if plan.vanilla_install_needed {
            "未安装：执行时先装原版"
        } else {
            "已安装：跳过"
        }
    ));
    let (total_bytes, unknown) = plan.install.total_bytes();
    human.push(format!(
        "  loader 库 {} 个 / {}{}",
        plan.install.file_count(),
        human_bytes(total_bytes),
        if unknown > 0 {
            format!(" + {unknown} 个大小未知")
        } else {
            String::new()
        }
    ));
    human.extend(sources_lines(samples));
    for warning in plan.warnings() {
        human.push(format!("告警: {warning}"));
    }
    human.extend(vanilla_lines(vanilla));
    if let Some(section) = processors {
        human.extend(processor_lines(section));
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
            human.push(format!("loader 版本清单: {}", path.display()));
        }
    }
    match registration {
        Some(registration) => {
            // M346：与 `pack import` / `instances adopt` 同一份实现（`pack/wording.rs`，前缀读目录）。
            human.push(registered_line(
                name,
                registration.profile.id.as_str(),
                &registration.profile.version_id,
                registration.created,
            ))
        }
        None => human.push(DRY_RUN_UNREGISTERED.to_string()),
    }
    human
}

#[cfg(test)]
#[path = "tests/report_loader.rs"]
mod tests;
