//! 安装：新建实例向导 / 下载页「安装」→ core 安装管线（原版 + Fabric/Quilt）→ 登记实例。
//!
//! 与 CLI `ull install` 同序：`RemoteSource::resolve → InstallPlan::build → execute → register`。
//! 差异：进度经 [`super::job::HostTask::Install`] 回流（文件计数 + 字节 + 速度/ETA），
//! 且整段跑在后台线程里（`block_on` 不占 UI 线程）。
//!
//! 加载器范围：原版 + Fabric/Quilt（纯库层）。Forge/NeoForge 要外调 java 跑 installer
//! processors（M35 的执行器在 CLI 侧），GUI 这一轮**显式拒装并说清去哪装**，
//! 而不是装出一个启动必崩的骨架。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use unlucky_core::addon::loader::{
    LoaderInstallPlan, LoaderKind as CoreLoaderKind, LoaderPlanInput, LoaderSource,
};
use unlucky_core::download::blocking::block_on;
use unlucky_core::download::cache::now_ms;
use unlucky_core::download::DownloadEngine;
use unlucky_core::game::arguments::Environment;
use unlucky_core::game::install::{
    execute, InstallError, InstallPlan, InstallReport, PlanInput, RemoteSource,
};

use crate::dto::{CreateRequest, InstallStage, LoaderKind};
use crate::toast::{Toast, ToastKind};

use super::env::HostEnv;
use super::job::{spawn, HostTask, JobSender};
use super::meter::{sink_for, Meter};

/// 文件级并发上限（与 CLI 同值：16 路压满带宽又不至于打爆镜像）。
pub const CONCURRENCY: usize = 16;

/// 用户取消的稳定文案（阶段边界生效）。
pub const CANCELLED: &str = "已取消（阶段边界停下；已下载的文件保留，重装会续用）";

/// 加载器徽标（`原版` / `Fabric 0.16.5`）。
pub fn loader_label(kind: LoaderKind, version: &str) -> String {
    if kind == LoaderKind::Vanilla || version.trim().is_empty() {
        kind.label().to_string()
    } else {
        format!("{} {}", kind.label(), version)
    }
}

/// GUI 侧支持的加载器（Forge/NeoForge 需要 installer processors）。
pub fn supported(kind: LoaderKind) -> bool {
    matches!(
        kind,
        LoaderKind::Vanilla | LoaderKind::Fabric | LoaderKind::Quilt
    )
}

/// 不支持的加载器 → 给用户的可执行建议。
pub fn unsupported_reason(kind: LoaderKind) -> String {
    format!(
        "{} 需要 installer processors（外调 java 改 jar），GUI 这一轮还没接入；请先在终端跑 `ull install <版本> --loader {}`，装完回实例页刷新即可启动。",
        kind.label(),
        kind.as_str()
    )
}

/// 后台安装（向导提交 / 下载页行内安装共用）。
pub fn spawn_install(env: HostEnv, tx: JobSender, request: CreateRequest, cancel: Arc<AtomicBool>) {
    spawn("install", move || {
        let meter = Arc::new(std::sync::Mutex::new(Meter::new(&request)));
        let result = run(&env, &tx, &request, &meter, &cancel);
        let mut progress = meter
            .lock()
            .map(|guard| guard.progress.clone())
            .unwrap_or_else(|_| Meter::new(&request).progress);
        match result {
            Ok(name) => {
                progress.stage = InstallStage::Done;
                progress.files_done = progress.files_total;
                progress.message = format!("已安装 {name}（{}）", progress.loader);
                let _ = tx.send(HostTask::InstallDone {
                    progress: Box::new(progress),
                    result: Ok(name),
                });
                let _ = tx.send(HostTask::Toast(Box::new(Toast {
                    id: 0,
                    kind: ToastKind::Success,
                    text: "安装完成，实例已登记".to_string(),
                    remaining_ms: 0,
                })));
            }
            Err(reason) => {
                progress.stage = if reason == CANCELLED {
                    InstallStage::Cancelled
                } else {
                    InstallStage::Failed
                };
                progress.message.clone_from(&reason);
                let _ = tx.send(HostTask::InstallDone {
                    progress: Box::new(progress),
                    result: Err(reason),
                });
            }
        }
    });
}

/// 真安装（同步跑完，调用方在后台线程里）。
fn run(
    env: &HostEnv,
    tx: &JobSender,
    request: &CreateRequest,
    meter: &Arc<std::sync::Mutex<Meter>>,
    cancel: &Arc<AtomicBool>,
) -> Result<String, String> {
    if request.version.trim().is_empty() {
        return Err("没有选择版本".to_string());
    }
    if !supported(request.loader) {
        return Err(unsupported_reason(request.loader));
    }
    let name = request.instance_name();
    let engine = super::engine::engine(env);
    {
        let mut guard = meter.lock().map_err(|_| "进度锁失效".to_string())?;
        guard.stage(InstallStage::Planning, "正在解析版本清单…");
        let _ = tx.send(HostTask::Install(Box::new(guard.progress.clone())));
    }

    if cancel.load(Ordering::SeqCst) {
        return Err(CANCELLED.to_string());
    }
    let version_id = if request.loader == LoaderKind::Vanilla {
        install_vanilla(env, &engine, request, tx, meter, cancel)?
    } else {
        install_loader(env, &engine, request, tx, meter, cancel)?
    };

    {
        let mut guard = meter.lock().map_err(|_| "进度锁失效".to_string())?;
        guard.stage(InstallStage::Registering, "正在登记实例…");
        let _ = tx.send(HostTask::Install(Box::new(guard.progress.clone())));
    }
    let created_at =
        unlucky_core::launch::time::format_rfc3339_millis(unlucky_core::launch::time::now());
    unlucky_core::game::install::instance::register(
        &env.layout,
        &name,
        &version_id,
        Some(&created_at),
    )
    .map_err(|error| format!("登记实例失败：{error}"))?;
    Ok(name)
}

/// 原版安装（`resolve → plan → [prefetch 资源索引] → execute`）。
fn install_vanilla(
    env: &HostEnv,
    engine: &DownloadEngine,
    request: &CreateRequest,
    tx: &JobSender,
    meter: &Arc<std::sync::Mutex<Meter>>,
    cancel: &Arc<AtomicBool>,
) -> Result<String, String> {
    let version = request.version.trim();
    let meta_cache = env.layout.meta_cache();
    let now = now_ms();
    let sink = sink_for(tx.clone(), Arc::clone(meter));
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
            let mut guard = meter.lock().map_err(|_| "进度锁失效".to_string())?;
            guard.progress.files_total = u64::try_from(plan.file_count()).unwrap_or(u64::MAX);
            guard.progress.bytes_total = bytes_total;
            guard.stage(
                InstallStage::Downloading,
                &format!("开始下载 {} 个文件…", plan.file_count()),
            );
        }
        if cancel.load(Ordering::SeqCst) {
            return Err(CANCELLED.to_string());
        }
        let report = execute(engine, &plan, CONCURRENCY, Some(sink))
            .await
            .map_err(install_error)?;
        ensure_complete(&report)?;
        Ok::<String, String>(version.to_string())
    })
}

/// 加载器安装（原版前置 → 加载器库 → 版本 JSON 落盘）。
fn install_loader(
    env: &HostEnv,
    engine: &DownloadEngine,
    request: &CreateRequest,
    tx: &JobSender,
    meter: &Arc<std::sync::Mutex<Meter>>,
    cancel: &Arc<AtomicBool>,
) -> Result<String, String> {
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

    let plan = LoaderInstallPlan::build(LoaderPlanInput {
        layout: &env.game,
        release: &release,
        environment: &Environment::host(),
        vanilla_installed: env.game.is_version_installed(version),
    });
    if plan.vanilla_install_needed {
        let mut vanilla = request.clone();
        vanilla.loader = LoaderKind::Vanilla;
        install_vanilla(env, engine, &vanilla, tx, meter, cancel)?;
    }
    {
        let mut guard = meter.lock().map_err(|_| "进度锁失效".to_string())?;
        guard.progress.files_total += u64::try_from(plan.files().len()).unwrap_or(u64::MAX);
        let (bytes, _) = plan.install.total_bytes();
        guard.progress.bytes_total = guard.progress.bytes_total.max(bytes);
        guard.stage(
            InstallStage::Downloading,
            &format!("开始下载加载器库（{} 个文件）…", plan.files().len()),
        );
    }
    if cancel.load(Ordering::SeqCst) {
        return Err(CANCELLED.to_string());
    }
    let sink = sink_for(tx.clone(), Arc::clone(meter));
    let report = block_on(async {
        execute(engine, &plan.install, CONCURRENCY, Some(sink))
            .await
            .map_err(install_error)
    })?;
    ensure_complete(&report)?;
    Ok(release.version_id)
}

/// 安装错误 → 用户可读文案。
fn install_error(error: InstallError) -> String {
    match error.hint() {
        Some(hint) => format!("{}（{hint}）", error.message()),
        None => error.message().to_string(),
    }
}

/// 有文件失败就报错（列前三条明细）。
fn ensure_complete(report: &InstallReport) -> Result<(), String> {
    if report.is_complete() {
        return Ok(());
    }
    let details: Vec<String> = report
        .failures(3)
        .iter()
        .filter_map(|outcome| {
            outcome
                .failure
                .as_ref()
                .map(|failure| format!("{}：{}", outcome.name, failure.message))
        })
        .collect();
    Err(format!(
        "{} 个文件没下成（共 {} 个）：{}",
        report.failed(),
        report.files.len(),
        details.join("；")
    ))
}

/// 安装目录的短显示（诊断/Toast 用）。
pub fn game_root(env: &HostEnv) -> PathBuf {
    env.game.root().to_path_buf()
}

#[cfg(test)]
#[path = "install_tests.rs"]
mod tests;
