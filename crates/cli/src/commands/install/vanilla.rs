//! `ull install <version>`（无加载器）：M27 原版安装流程的宿主。
//!
//! ```text
//! RemoteSource::resolve（版本清单缓存 → 版本 JSON → 继承链）
//!   → RemoteSource::asset_index（资源索引，顺便拿字节给下面预取）
//!   → InstallPlan::build（纯计算：文件清单 / 总大小 / 角色）
//!   → [--dry-run 到此为止]
//!   → execute（跳过已就绪 → 引擎并发下载 → 原子写版本 JSON）
//!   → register（profiles.json + config/instances/<id>.json）
//! ```
//!
//! 异步整段包在一次 `block_on` 里：CLI 不依赖 tokio（D18 依赖树纪律），
//! reqwest 的连接池也不会跨运行时而失效。

use unlucky_core::download::blocking::block_on;
use unlucky_core::download::cache::now_ms;
use unlucky_core::download::DownloadEngine;
use unlucky_core::game::arguments::Environment;
use unlucky_core::game::install::{
    execute, InstallError, InstallErrorKind, InstallPlan, InstallReport, PlanInput, RemoteSource,
};

use super::{
    build_engine, ensure_name_free, failure_error, map_install_error, report, CONCURRENCY,
};
use crate::cli::InstallArgs;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::Report;

/// 执行 `install`（原版）。
///
/// # Errors
///
/// 用法错误、版本不存在、网络失败、校验不符、写盘失败、实例清单损坏。
pub(super) fn run(ctx: &Ctx, args: &InstallArgs) -> Result<Report, CliError> {
    let version = args.version.trim().to_string();
    if version.is_empty() {
        return Err(CliError::usage(
            "install 需要版本 id：%prog% install <version>",
        ));
    }
    let name = args
        .name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| version.clone());
    ensure_name_free(ctx, &name, &version)?;

    let engine = build_engine(ctx);
    let now = now_ms();
    let (plan, report) = plan_and_install(
        ctx,
        &engine,
        &version,
        args.dry_run,
        args.progress,
        args.json_progress,
    )?;

    if let Some(report) = &report {
        if !report.is_complete() {
            return Err(failure_error(report, ctx));
        }
    }

    let registration = if args.dry_run {
        None
    } else {
        Some(super::register_instance(ctx, &name, &version)?)
    };

    Ok(report::build(
        ctx,
        &engine,
        now,
        args.dry_run,
        &plan,
        report.as_ref(),
        registration.as_ref(),
        &name,
        &version,
    ))
}

/// 规划 + 执行原版安装（`--dry-run` 只规划），供加载器安装当"前置步骤"复用。
///
/// # Errors
///
/// 版本不存在、网络失败、清单装不全、写盘失败。
pub(super) fn plan_and_install(
    ctx: &Ctx,
    engine: &DownloadEngine,
    version: &str,
    dry_run: bool,
    progress: bool,
    json_progress: bool,
) -> Result<(InstallPlan, Option<InstallReport>), CliError> {
    let game = ctx.game();
    let meta_cache = ctx.layout().meta_cache();
    let now = now_ms();

    block_on(async {
        let source = RemoteSource::new(engine, &game, &meta_cache, now);
        let resolved = source.resolve(version).await?;
        let fetched = match resolved
            .asset_index
            .as_ref()
            .filter(|index| index.url.is_some())
        {
            Some(reference) => {
                let (index, bytes) = source.asset_index(reference).await?;
                Some((index, game.asset_index(&reference.id), bytes))
            }
            None => None,
        };
        let mut plan = InstallPlan::build(PlanInput {
            layout: &game,
            version_id: version,
            resolved: &resolved,
            asset_index: fetched.as_ref().map(|(index, _, _)| index),
            environment: &Environment::host(),
        })?;
        if let Some((_, dest, bytes)) = fetched {
            plan.prefetch(dest, bytes);
        }
        if dry_run {
            return Ok::<Attempt, InstallError>(Attempt { plan, report: None });
        }
        // 计划里没有客户端 jar（旧版清单缺 downloads.client）→ 拒绝"装出个空壳"。
        if !plan.is_installable() {
            let reasons = if plan.warnings.is_empty() {
                "清单缺少客户端 jar 的下载信息".to_string()
            } else {
                plan.warnings.join("；")
            };
            return Err(InstallError::new(
                InstallErrorKind::Metadata,
                format!("版本 {version} 装不全：{reasons}"),
            )
            .with_hint(
                "旧版（pre-1.6）清单兜底待补；可先用官方启动器装好该版本再用 `%prog% launch`",
            ));
        }
        let sink = report::progress_sink(progress, json_progress, plan.file_count());
        let report = execute(engine, &plan, CONCURRENCY, sink).await?;
        Ok(Attempt {
            plan,
            report: Some(report),
        })
    })
    .map(|attempt| (attempt.plan, attempt.report))
    .map_err(|error: InstallError| map_install_error(&error, ctx))
}

/// 一次安装尝试的产物（`--dry-run` 只有计划）。
struct Attempt {
    plan: InstallPlan,
    report: Option<InstallReport>,
}
