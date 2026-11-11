//! 加载器安装的呈现形态（人类可读行 + `--json` 载荷）。
//!
//! 与 [`super::report`] 的分工：那边是"原版安装"（客户端 jar + 资源 + 库），这边是
//! "叠在原版上的 loader 层"（只有库）。共用同一套进度回调与字节格式化。

use serde_json::{json, Value};
use unlucky_core::addon::loader::processors::{ProcessorPlan, ProcessorRun};
use unlucky_core::addon::loader::LoaderInstallPlan;
use unlucky_core::download::DownloadEngine;
use unlucky_core::game::install::{InstallReport, Registration};

use super::report::human_bytes;
use crate::context::Ctx;
use crate::output::Report;

/// processors 段（M35）的报告输入：计划 + 依赖库下载结果 + 执行结果。
///
/// `--dry-run` 只有 `plan`；幂等跳过时 `libraries`/`run` 都是 `None` 且 `skipped = true`。
pub struct ProcessorSection<'a> {
    /// 执行计划（dry-run 也带上：将执行的 processor 清单 + 所需 Java）。
    pub plan: &'a ProcessorPlan,
    /// processor 依赖库的下载结果。
    pub libraries: Option<&'a InstallReport>,
    /// 执行结果。
    pub run: Option<&'a ProcessorRun>,
    /// 整段跳过（版本 JSON 已在盘上 = processors 早已跑完）。
    pub skipped: bool,
}

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

/// processors 段的 JSON 载荷（计划 + 依赖库结果 + 逐步骤结果）。
fn processor_json(section: &ProcessorSection<'_>) -> serde_json::Value {
    let mut node = section.plan.to_json();
    if let Some(object) = node.as_object_mut() {
        object.insert("skipped".to_string(), json!(section.skipped));
        object.insert(
            "libraries_result".to_string(),
            json!(section.libraries.map(|report| json!({
                "downloaded": report.downloaded(),
                "skipped": report.skipped(),
                "failed": report.failed(),
                "bytes": report.bytes(),
            }))),
        );
        object.insert(
            "run".to_string(),
            json!(section.run.map(ProcessorRun::to_json)),
        );
    }
    node
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
        Some(registration) => human.push(format!(
            "已登记实例 {}（{}）→ 版本 {}（{}）",
            name,
            registration.profile.id.as_str(),
            registration.profile.version_id,
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

/// processors 段的人读行（计划 / 依赖库 / 逐步骤 / 告警）。
fn processor_lines(section: &ProcessorSection<'_>) -> Vec<String> {
    let plan = section.plan;
    let mut lines = Vec::new();
    lines.push(format!(
        "  installer processors {} 步，需要 {}（installer {}）",
        plan.step_count(),
        plan.java_requirement.describe(),
        plan.installer.display()
    ));
    if section.skipped {
        lines.push("  版本清单已存在：processors 整段跳过（幂等）".to_string());
    }
    if let Some(libraries) = section.libraries {
        lines.push(format!(
            "  processor 依赖库 {} 个：下载 {} / 跳过 {} / 失败 {}",
            plan.libraries.len(),
            libraries.downloaded(),
            libraries.skipped(),
            libraries.failed()
        ));
    }
    if let Some(run) = section.run {
        lines.push(format!(
            "  Java {}（{}）{}",
            run.java
                .major
                .map_or_else(|| "未知".to_string(), |major| major.to_string()),
            run.java.origin.as_str(),
            run.java.binary.display()
        ));
        let total = run.steps.len();
        for step in &run.steps {
            lines.push(format!(
                "    [{}/{}] {} → {}{}",
                step.index,
                total,
                step.label,
                if step.status.as_str() == "skipped" {
                    "跳过（产物已就绪）".to_string()
                } else {
                    format!("执行 {} ms", step.elapsed_ms)
                },
                if step.outputs > 0 {
                    format!("（产物 {} 个）", step.outputs)
                } else {
                    String::new()
                }
            ));
        }
        for warning in &run.warnings {
            lines.push(format!("  告警: {warning}"));
        }
    }
    for warning in &plan.warnings {
        lines.push(format!("  告警: {warning}"));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use unlucky_core::addon::loader::{
        LoaderInstallPlan, LoaderKind, LoaderPlanInput, LoaderRelease, LoaderSupport,
    };
    use unlucky_core::game::arguments::Environment;
    use unlucky_core::game::install::ManifestWrite;
    use unlucky_core::game::layout::GameLayout;
    use unlucky_core::game::manifest::VersionJson;

    /// 一份最小的 Fabric 加载器计划（零网络：只算路径，不落盘）。
    fn loader_plan(root: &std::path::Path) -> LoaderInstallPlan {
        let layout = GameLayout::new(root);
        let release = LoaderRelease {
            kind: LoaderKind::Fabric,
            mc_version: "1.20.1".to_string(),
            parent_version_id: "1.20.1".to_string(),
            loader_version: "0.15.11".to_string(),
            version_id: "1.20.1-fabric-0.15.11".to_string(),
            version_json: VersionJson::from_json_str(
                r#"{"id":"1.20.1-fabric-0.15.11","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient","inheritsFrom":"1.20.1","libraries":[]}"#,
            )
            .expect("loader 清单应可解析"),
            support: LoaderSupport::JsonRoute,
            installer: None,
            install_profile: None,
            warnings: Vec::new(),
        };
        LoaderInstallPlan::build(LoaderPlanInput {
            layout: &layout,
            release: &release,
            environment: &Environment::host(),
            parent_installed: true,
        })
        .expect("计划应可组")
    }

    /// **M216 r2 P2-2**：装加载器时「先装原版」那一步替换了共享 `.minecraft` 里的原版清单，
    /// 这句话必须出现在**加载器安装**的人读输出里（此前只出现在纯原版安装的输出里）。
    ///
    /// 抓得住：`human_lines` 只遍历 `plan.warnings()`（= r1 状态）；备份路径没渲出来；
    /// `--json` 的 `vanilla` 段没了。抓不住：真实 `ull install --loader …` 整条链路（要网络），
    /// 以及「原版 report 是否真被 `loader.rs` 传进来」那步接线（见报告）。
    #[test]
    fn a_replaced_vanilla_manifest_is_said_out_loud_in_the_loader_output() {
        let root = std::env::temp_dir().join(format!("ull216-loader-{}", std::process::id()));
        let plan = loader_plan(&root);
        let backup = PathBuf::from("/g/.minecraft/versions/1.20.1/1.20.1.json.ullbak-99");
        let vanilla = InstallReport {
            files: Vec::new(),
            version_json: Some(PathBuf::from("/g/.minecraft/versions/1.20.1/1.20.1.json")),
            manifest: Some(ManifestWrite::Replaced {
                backup: backup.clone(),
            }),
            warnings: vec!["已替换版本清单 /g/.minecraft/versions/1.20.1/1.20.1.json".to_string()],
        };
        let text = human_lines(false, &plan, None, Some(&vanilla), None, &[], "n", None).join("\n");
        assert!(
            text.contains("已替换版本清单"),
            "原版那段的告警没进人读输出：{text}"
        );
        assert!(
            text.contains(&backup.display().to_string()),
            "原版清单的备份路径没进人读输出：{text}"
        );
        // 没先装原版时不许凭空多说话。
        let plain = human_lines(false, &plan, None, None, None, &[], "n", None).join("\n");
        assert!(!plain.contains("原版版本清单已替换"), "{plain}");
        // `--json` 的 `vanilla` 段（结构化，脚本可读）。
        let block = vanilla_json(Some(&vanilla)).expect("应当有 vanilla 段");
        assert_eq!(block["manifest"]["kind"], Value::String("replaced".into()));
        assert_eq!(
            block["manifest"]["backup"].as_str(),
            Some(backup.to_string_lossy().as_ref())
        );
        assert!(vanilla_json(None).is_none());
        std::fs::remove_dir_all(&root).ok();
    }
}
