//! `ull install <version> --loader <kind> [--loader-version <v>]`：加载器层的编排。
//!
//! ```text
//! LoaderSource::resolve（官方 meta：版本列表 → profile JSON / installer profile）
//!   → LoaderInstallPlan::build（纯计算：库清单 + loader 版本 JSON 落点）
//!   → [--dry-run 到此为止，但多带一段 processors 计划]
//!   → 原版已装？否 → vanilla::plan_and_install（先装原版）
//!   → 需要 installer？→ 依赖库 → 选 Java → 串行跑 processors（M35）
//!   → execute（loader 库并发下载 → 全量就绪才写 versions/<loader-id>/<loader-id>.json）
//!   → register（实例 versionId 指向 loader 版本）
//! ```
//!
//! 「先装原版」是**硬前置**：loader 版本 JSON 只声明 `inheritsFrom`，jar 与资源都在原版那层，
//! 原版缺失时启动链必然断裂。已装则跳过（`is_version_installed` 判据 = 版本 JSON 在盘上）。
//!
//! processors 段的顺序也是硬约束：**先跑 processors，再写版本 JSON**。版本清单存在是
//! "这版装完了"的判据，抢在 processors 前写会留下一个启动必崩却被报成已安装的实例。

use unlucky_core::addon::loader::{
    LoaderError, LoaderInstallPlan, LoaderKind, LoaderPlanInput, LoaderSource,
};
use unlucky_core::download::blocking::block_on;
use unlucky_core::download::cache::now_ms;
use unlucky_core::game::arguments::Environment;
use unlucky_core::game::install::execute;

use super::report_loader::ProcessorSection;
use super::{
    build_engine, ensure_name_free, failure_error, loader_processors, map_install_error,
    register_instance, report, report_loader, vanilla, CONCURRENCY,
};
use crate::cli::InstallArgs;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::Report;

/// 执行加载器安装。
///
/// # Errors
///
/// 用法错误、加载器未知/不支持、元数据读取失败、原版装不上、库下载失败、
/// installer processors 失败、实例登记失败。
pub(super) fn run(ctx: &Ctx, args: &InstallArgs, kind: LoaderKind) -> Result<Report, CliError> {
    let version = args.version.trim().to_string();
    if version.is_empty() {
        return Err(CliError::usage(
            "install 需要版本 id：%prog% install <version> --loader fabric",
        ));
    }

    let engine = build_engine(ctx);
    let layout = ctx.layout();
    let game = ctx.game();
    let meta_cache = layout.meta_cache();
    let now = now_ms();

    let release = block_on(async {
        let source = LoaderSource::new(&engine, &meta_cache);
        source
            .resolve(kind, &version, args.loader_version.as_deref())
            .await
    })
    .map_err(|error: LoaderError| map_loader_error(&error, ctx))?;

    let name = args
        .name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| release.version_id.clone());
    ensure_name_free(ctx, &name, &release.version_id)?;

    let plan = LoaderInstallPlan::build(LoaderPlanInput {
        layout: &game,
        release: &release,
        environment: &Environment::host(),
        vanilla_installed: game.is_version_installed(&version),
    });
    let processor_plan = loader_processors::plan(ctx, &release)?;

    if args.dry_run {
        let section = processor_plan.as_ref().map(|plan| ProcessorSection {
            plan,
            libraries: None,
            run: None,
            skipped: false,
        });
        return Ok(report_loader::build(
            ctx, &engine, now, true, &plan, None, None, &name, section,
        ));
    }

    if plan.vanilla_install_needed {
        let (_, vanilla_report) = vanilla::plan_and_install(
            ctx,
            &engine,
            &version,
            false,
            args.progress,
            args.json_progress,
        )?;
        if let Some(report) = &vanilla_report {
            if !report.is_complete() {
                return Err(failure_error(report, ctx));
            }
        }
    }

    // installer processors 段：版本 JSON 已在盘上 = 上一轮早已跑完，整段跳过。
    let mut processor_libs = None;
    let mut processor_run = None;
    let mut processor_skipped = false;
    if let Some(processor_plan) = &processor_plan {
        if game.is_version_installed(&release.version_id) {
            processor_skipped = true;
        } else {
            let (libraries, run) = loader_processors::execute(ctx, &engine, processor_plan, args)?;
            processor_libs = Some(libraries);
            processor_run = Some(run);
        }
    }

    let sink = report::progress_sink(args.progress, args.json_progress, plan.files().len());
    let install_report = block_on(execute(&engine, &plan.install, CONCURRENCY, sink))
        .map_err(|error| map_install_error(&error, ctx))?;
    if !install_report.is_complete() {
        return Err(failure_error(&install_report, ctx));
    }

    let registration = register_instance(ctx, &name, &release.version_id)?;

    let section = processor_plan
        .as_ref()
        .map(|processor_plan| ProcessorSection {
            plan: processor_plan,
            libraries: processor_libs.as_ref(),
            run: processor_run.as_ref(),
            skipped: processor_skipped,
        });
    Ok(report_loader::build(
        ctx,
        &engine,
        now,
        false,
        &plan,
        Some(&install_report),
        Some(&registration),
        &name,
        section,
    ))
}

/// 加载器错误 → CLI 错误（稳定码 `loader_*`）。
fn map_loader_error(error: &LoaderError, ctx: &Ctx) -> CliError {
    let hint = match error.hint() {
        Some(hint) => format!("{hint}（游戏根：{}）", ctx.game().root().display()),
        None => format!("游戏根：{}", ctx.game().root().display()),
    };
    CliError::user(error.kind().as_str(), error.message().to_string()).with_hint(hint)
}
