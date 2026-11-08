//! 两条安装路线（原版 / 加载器）——M105 从 `install.rs` 搬出，守 400 行硬线。
//!
//! 搬移是**纯搬移**：函数体一行没改，只多了本文件自己的 `use`。
//! 父模块（`install`）的私有项对子模块可见，所以 `Job` / `CANCELLED` / `job_error` /
//! `ensure_complete` / `sink_for` 一律不重新导出，直接用。

use std::sync::atomic::Ordering;
use std::sync::Arc;

use unlucky_core::addon::loader::{
    LoaderInstallPlan, LoaderKind as CoreLoaderKind, LoaderPlanInput, LoaderSource,
};
use unlucky_core::download::blocking::block_on;
use unlucky_core::download::cache::now_ms;
use unlucky_core::download::DownloadEngine;
use unlucky_core::game::arguments::Environment;
use unlucky_core::game::install::{execute_with_cancel, InstallPlan, PlanInput, RemoteSource};

use crate::dto::{CreateRequest, InstallStage, LoaderKind};

use super::super::install_processors::{self, StageInput};
use super::super::meter::sink_for;
use super::{ensure_complete, install_error, job_error, Job, CANCELLED, CONCURRENCY};

/// 原版安装（`resolve → plan → [prefetch 资源索引] → execute`）。
pub(super) fn install_vanilla(
    job: &Job<'_>,
    engine: &DownloadEngine,
    request: &CreateRequest,
) -> Result<String, String> {
    let env = job.env;
    let version = request.version.trim();
    let meta_cache = env.layout.meta_cache();
    let now = now_ms();
    let sink = sink_for(job.tx.clone(), Arc::clone(job.meter));
    block_on(async {
        let source = RemoteSource::new(engine, &env.game, &meta_cache, now);
        let resolved = source.resolve(version).await.map_err(install_error)?;
        let fetched = match resolved
            .asset_index
            .as_ref()
            .filter(|index| index.url.is_some())
        {
            Some(reference) => {
                let (index, bytes) = source.asset_index(reference).await.map_err(install_error)?;
                Some((index, env.game.asset_index(&reference.id), bytes))
            }
            None => None,
        };
        let mut plan = InstallPlan::build(PlanInput {
            layout: &env.game,
            version_id: version,
            resolved: &resolved,
            asset_index: fetched.as_ref().map(|(index, _, _)| index),
            environment: &Environment::host(),
        })
        .map_err(|error| error.to_string())?;
        if let Some((_, dest, bytes)) = fetched {
            plan.prefetch(dest, bytes);
        }
        if !plan.is_installable() {
            return Err(format!(
                "版本 {version} 装不全：{}",
                plan.warnings.join("；")
            ));
        }
        let (bytes_total, _) = plan.total_bytes();
        {
            let mut guard = job.meter.lock().map_err(|_| "进度锁失效".to_string())?;
            guard.progress.files_total = u64::try_from(plan.file_count()).unwrap_or(u64::MAX);
            guard.progress.bytes_total = bytes_total;
            guard.stage(
                InstallStage::Downloading,
                &format!("开始下载 {} 个文件…", plan.file_count()),
            );
        }
        if job.cancel.load(Ordering::SeqCst) {
            return Err(CANCELLED.to_string());
        }
        let report = execute_with_cancel(
            engine,
            &plan,
            CONCURRENCY,
            Some(sink),
            Some(job.cancel.as_ref()),
        )
        .await
        .map_err(|error| job_error(job, error))?;
        ensure_complete(&report)?;
        Ok::<String, String>(version.to_string())
    })
}

/// 加载器安装（原版前置 → installer processors → 加载器库 → 版本 JSON 落盘）。
pub(super) fn install_loader(
    job: &Job<'_>,
    engine: &DownloadEngine,
    request: &CreateRequest,
) -> Result<String, String> {
    let env = job.env;
    let version = request.version.trim();
    let kind = CoreLoaderKind::parse(request.loader.as_str())
        .ok_or_else(|| format!("未知加载器 `{}`", request.loader.as_str()))?;
    let meta_cache = env.layout.meta_cache();
    let loader_version = request.loader_version.trim().to_string();
    let explicit = (!loader_version.is_empty()).then_some(loader_version.as_str());
    let release = block_on(async {
        LoaderSource::new(engine, &meta_cache)
            .resolve(kind, version, explicit)
            .await
    })
    .map_err(|error| error.to_string())?;
    job.journal.line(&format!(
        "加载器解析：{} {} → {}",
        release.kind.label(),
        release.loader_version,
        release.version_id
    ));

    let plan = LoaderInstallPlan::build(LoaderPlanInput {
        layout: &env.game,
        release: &release,
        environment: &Environment::host(),
        vanilla_installed: env.game.is_version_installed(version),
    });
    if plan.vanilla_install_needed {
        let mut vanilla = request.clone();
        vanilla.loader = LoaderKind::Vanilla;
        install_vanilla(job, engine, &vanilla)?;
    }
    // 两段（加载器库 + 处理器依赖库）在动手前一次预记账：进度条只走一条单调曲线，
    // 处理器依赖库下完不会先满格、再在加载器库开始时回落到一半。
    {
        let mut guard = job.meter.lock().map_err(|_| "进度锁失效".to_string())?;
        let (bytes, _) = plan.install.total_bytes();
        guard.expect(plan.files().len(), bytes);
    }
    // Forge/NeoForge：installer processors 必须先跑完，`execute` 才允许写版本 JSON。
    if release.requires_installer() {
        let input = StageInput {
            kind: release.kind,
            mc_version: &release.mc_version,
            version_id: &release.version_id,
            profile: release.install_profile.as_ref(),
            installer: release.installer.as_deref(),
            installed: env.game.is_version_installed(&release.version_id),
        };
        // `None` = 这一版不需要跑处理器（版本 JSON 已在盘上 / profile 没有 client 侧步骤），
        // 进度与日志都由处理器段自己写全，这里只消费返回值。
        let _ = install_processors::stage(
            env,
            engine,
            &input,
            &install_processors::CORE,
            &job.reporter(),
            job.cancel,
        )?;
    }
    {
        let mut guard = job.meter.lock().map_err(|_| "进度锁失效".to_string())?;
        guard.stage(
            InstallStage::Downloading,
            &format!("开始下载加载器库（{} 个文件）…", plan.files().len()),
        );
    }
    if job.cancel.load(Ordering::SeqCst) {
        return Err(CANCELLED.to_string());
    }
    let sink = sink_for(job.tx.clone(), Arc::clone(job.meter));
    let report = block_on(async {
        execute_with_cancel(
            engine,
            &plan.install,
            CONCURRENCY,
            Some(sink),
            Some(job.cancel.as_ref()),
        )
        .await
    })
    .map_err(|error| job_error(job, error))?;
    ensure_complete(&report)?;
    Ok(release.version_id)
}
