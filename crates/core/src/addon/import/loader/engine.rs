//! 生产实现：**官方 meta → 原版前置 → installer processors → 库 → 版本 JSON**。
//!
//! 顺序与 `ull install --loader <kind>` 的分段表逐条同源（[`crate::addon::loader::plan`] 的
//! 计划层 + [`crate::game::install`] 的落盘层），不另起一套下载/写盘逻辑：
//!
//! ```text
//! LoaderSource::resolve（官方 meta：版本列表 → profile JSON / installer profile）
//!   → LoaderInstallPlan::build（纯计算：库清单 + loader 版本 JSON 落点）
//!   → [原版没装] RemoteSource::resolve → InstallPlan::build → execute（先装原版）
//!   → [Forge/NeoForge] ProcessorPlan::build → 依赖库 → 选 Java → 串行跑 processors
//!   → execute（loader 库并发下载 → 全量就绪才写 versions/<loader-id>/<loader-id>.json）
//! ```
//!
//! # 两道版本闸（"不许换一个版本装上去"）
//!
//! 1. **解析闸**：`resolve(..., Some(清单版本))` 之后比对 `loader_version` 与 `version_id`，
//!    任何一个不符都拒 —— 元数据上找不到"清单要的那个版本"时唯一的正确行为是报错，不是取最近的；
//! 2. **复核闸**在 [`super::install_declared_loader`]：装回来的版本 id 还要再核一次。
//!
//! 两道闸的判据都是**同一个装载器版本**（[`super::version`]），不是逐字相等：老版 Forge 里
//! **一部分 build** 的 maven 版本串带双后缀（`1.7.10-10.13.4.1614-1.7.10`），清单写的是短形
//! （`10.13.4.1614`），两者是同一次发布（P36）。归一之后交集为空仍然是「不是同一个版本」。
//! 放行之后还有一道**落盘判据**（[`super::version::check_landed`]）：版本 JSON 必须落在清单
//! 声明的那个 id 下——否则实例会登记成一个读不到的版本（M332 r1 P2-2）。
//!
//! # 一个已知的阻塞点（如实登记）
//!
//! `select_java` 与 `processors::run` 是**同步**的（扫盘 + 外调 java，进程内没有取消钩子）。
//! 它们跑在导入线程的运行时 worker 上，与 CLI/presenter 同一条执行器，区别只是那里各自有
//! 专门的后台线程。processors 段期间没有任何下载在飞（依赖库已经下完），所以占住一个 worker
//! 不会拖慢下载；取消在进入该段前后各查一次（与 presenter 的 `install_processors` 同口径）。

use crate::addon::loader::processors::{self, ProcessorPlan, ProcessorPlanInput, ProcessorRun};
use crate::addon::loader::{
    LoaderError, LoaderInstallPlan, LoaderPlanInput, LoaderRelease, LoaderSource,
};
use crate::download::cache::now_ms;
use crate::download::DownloadEngine;
use crate::game::arguments::Environment;
use crate::game::install::{
    execute_with_cancel, FileOutcome, InstallError, InstallErrorKind, InstallPlan, InstallReport,
    PlanFile, PlanInput, ProgressSink, RemoteSource,
};
use crate::platform::layout::LauncherLayout;

use super::super::report::{tell_to, ImportProgressSink, ProcessorSummary};
use super::version;
use super::{LoaderFuture, LoaderInstallError, LoaderInstallJob, LoaderInstalled, LoaderInstaller};

/// 与 `ull install` / 下载页同一个并发档（引擎侧并行度，只是排队上限）。
const CONCURRENCY: usize = 16;

/// 生产实现：把第六跳接到真实的官方元数据与下载引擎上。
pub struct EngineLoaderInstaller<'a> {
    engine: &'a DownloadEngine,
    /// 启动器数据布局：元数据缓存与托管 JDK 都从它取（`meta_cache()` / `java_managed()`）。
    ///
    /// 只收布局而不收两个路径，是为了让"这两个目录一定来自同一个数据根"成为结构事实
    /// （错配的 `meta_cache` + `java_managed` 只会装出一半可用的环境）。
    layout: &'a LauncherLayout,
}

impl<'a> EngineLoaderInstaller<'a> {
    /// 绑定引擎与启动器数据布局。
    #[must_use]
    pub const fn new(engine: &'a DownloadEngine, layout: &'a LauncherLayout) -> Self {
        Self { engine, layout }
    }

    /// 解析 → 计划 → 原版前置 → processors → 落盘。
    async fn run(&self, job: &LoaderInstallJob<'_>) -> Result<LoaderInstalled, LoaderInstallError> {
        let target = job.target;
        let meta_cache = self.layout.meta_cache();
        let mut release = LoaderSource::new(self.engine, &meta_cache)
            .resolve(
                target.kind,
                &target.mc_version,
                Some(&target.loader_version),
            )
            .await
            .map_err(loader_error)?;
        // 闸①：解析回来的**必须**是清单声明的那个装载器版本（判据见 [`version`]）。
        version::check_resolved(&release, target)?;
        // **落盘 id 钉到清单声明的那个**：实例登记用的是清单的版本 id（`ImportPlan::version_id`），
        // 所以 `versions/<id>/` 必须就是它——否则导入"成功"、点启动读不到版本 JSON，
        // 正是本模块要消灭的假功能。1.20.1 / Fabric / NeoForge 两边写法本来就相同，这里不做事。
        version::adopt_declared_id(&mut release, target);

        let plan = LoaderInstallPlan::build(LoaderPlanInput {
            layout: job.layout,
            release: &release,
            environment: &Environment::host(),
            parent_installed: job.layout.is_version_installed(&release.parent_version_id),
        })
        .map_err(loader_error)?;

        // 原版是硬前置（loader 版本 JSON 只有 `inheritsFrom`）。原版那一层的计划告警
        // （缺某类资源、已降级）也带回报告：以前它们只在**失败**文案里露一面，装完就丢。
        let vanilla_warnings = if plan.vanilla_install_needed {
            self.install_vanilla(job, &release.mc_version).await?
        } else {
            Vec::new()
        };
        // M321：processor 链的结果**带回去**（以前这里 `?` 掉就完事，`ProcessorRun` 整个丢掉）。
        let run = if release.requires_installer() {
            self.run_processors(job, &release).await?
        } else {
            None
        };

        // M321 r1：loader 库下载也走同一个进度适配器（M320 实测这段是分钟级的长尾）。
        let report = run_plan(self.engine, &plan.install, job)
            .await
            .map_err(install_error)?;
        if !report.is_complete() {
            return Err(incomplete("加载器的库", &report));
        }
        // 版本 JSON 是"这一版装完了"的唯一判据（`execute` 全量就绪后才原子写它）。
        if !job.layout.is_version_installed(&release.version_id) {
            return Err(LoaderInstallError::failed(format!(
                "{} 的库都下完了，但 versions/{}/ 里没有版本 JSON：这一版仍然启动不了",
                release.version_id, release.version_id
            ))
            .with_hint("重试一次；若反复出现，删掉 versions/ 下这个目录再重导"));
        }
        Ok(LoaderInstalled {
            detail: format!(
                "{} {} → {}",
                release.kind.label(),
                release.loader_version,
                release.version_id
            ),
            version_id: release.version_id.clone(),
            // 计划期（已含解析期 `release.warnings`）+ 原版那一层。processor 期的那几条
            // **归 `ProcessorSummary` 自己**，这里再并一次会让同一条告警进两次、两个 key。
            warnings: merge_warnings(plan.warnings(), vanilla_warnings),
            processors: run.as_ref().map(ProcessorSummary::from_run),
        })
    }

    /// 原版前置（与 `ull install` 的 vanilla 段同一实现：解析 → 计划 → 执行）。
    ///
    /// 返回原版那层的**计划告警**，由调用方铺进报告（失败时它们进错误文案，原本就有那一处）。
    async fn install_vanilla(
        &self,
        job: &LoaderInstallJob<'_>,
        mc_version: &str,
    ) -> Result<Vec<String>, LoaderInstallError> {
        let meta_cache = self.layout.meta_cache();
        let source = RemoteSource::new(self.engine, job.layout, &meta_cache, now_ms());
        let resolved = source.resolve(mc_version).await.map_err(install_error)?;
        let fetched = match resolved
            .asset_index
            .as_ref()
            .filter(|index| index.url.is_some())
        {
            Some(reference) => {
                let (index, bytes) = source.asset_index(reference).await.map_err(install_error)?;
                Some((index, job.layout.asset_index(&reference.id), bytes))
            }
            None => None,
        };
        let mut plan = InstallPlan::build(PlanInput {
            layout: job.layout,
            version_id: mc_version,
            resolved: &resolved,
            asset_index: fetched.as_ref().map(|(index, _, _)| index),
            environment: &Environment::host(),
        })
        .map_err(|error| {
            LoaderInstallError::failed(format!("原版 {mc_version} 的安装计划组不出来：{error}"))
        })?;
        if let Some((_, dest, bytes)) = fetched {
            plan.prefetch(dest, bytes);
        }
        if !plan.is_installable() {
            return Err(LoaderInstallError::failed(format!(
                "原版 {mc_version} 装不全（加载器要叠在它上面）：{}",
                plan.warnings.join("；")
            )));
        }
        // M321 r1：原版 jar + assets 是整跳最长的段（M320 实测 ≈6.1 min），进度由 `run_plan` 统一接。
        let report = run_plan(self.engine, &plan, job)
            .await
            .map_err(install_error)?;
        if !report.is_complete() {
            return Err(incomplete(&format!("原版 {mc_version} 的文件"), &report));
        }
        Ok(plan.warnings)
    }

    /// Forge / NeoForge 的 installer processors 段（版本 JSON 在盘上就整段跳过）。
    ///
    /// 返回 `Some(run)` = 这次真的跑过（或整段跳过判定完毕后的执行结果），`None` = 这一段没跑
    /// （版本 JSON 已在盘上 / profile 里没有 processor）。结果**由调用方带回报告**。
    async fn run_processors(
        &self,
        job: &LoaderInstallJob<'_>,
        release: &LoaderRelease,
    ) -> Result<Option<ProcessorRun>, LoaderInstallError> {
        if job.layout.is_version_installed(&release.version_id) {
            return Ok(None);
        }
        let (Some(profile), Some(installer)) = (
            release.install_profile.as_ref(),
            release.installer.as_deref(),
        ) else {
            return Err(LoaderInstallError::failed(format!(
                "{} {} 要跑 installer processors，但元数据里没有 install profile / installer 路径",
                release.kind.label(),
                release.loader_version
            ))
            .with_hint("重跑一次（元数据缓存可能不完整）"));
        };
        let kind = release.kind;
        let scratch = self
            .layout
            .meta_cache()
            .join("processors")
            .join(safe_name(&release.version_id));
        let plan = ProcessorPlan::build(ProcessorPlanInput {
            kind,
            mc_version: &release.mc_version,
            version_id: &release.version_id,
            profile,
            layout: job.layout,
            installer,
            scratch: &scratch,
            environment: &Environment::host(),
        })
        .map_err(processor_error)?;
        if plan.is_empty() {
            return Ok(None);
        }
        // M321 ①：依赖库下载也有进度（以前这里传 `None`，用户看到的是"卡住"）。
        let library_sink = file_progress(&job.progress, plan.libraries.len());
        let libraries =
            download_processor_libraries(self.engine, &plan.libraries, library_sink).await;
        if !libraries.is_complete() {
            return Err(incomplete("installer 依赖库", &libraries));
        }
        let java_managed = self.layout.java_managed();
        let Some(java) = processors::select_java(&java_managed, &plan.java_requirement) else {
            return Err(LoaderInstallError::failed(format!(
                "跑 installer processors 需要 {}，但一个可用的 Java 都没找到",
                plan.java_requirement.describe()
            ))
            .with_hint(format!(
                "到「Java」页装一个托管 JDK，或把 JAVA_HOME 指向可用的 JDK（托管目录 {}）",
                java_managed.display()
            )));
        };
        // 串行跑完才允许 `execute` 写版本 JSON——抢在它前面写会留下"启动必崩却报成已装"的实例。
        // M321 ②：逐步骤回调接上（与 `ull install --loader` 同一个 `StepSink` 类型与形状）。
        let run = processors::run(&plan, &java, job.steps.clone()).map_err(processor_error)?;
        Ok(Some(run))
    }
}

impl LoaderInstaller for EngineLoaderInstaller<'_> {
    fn install<'a>(
        &'a self,
        job: LoaderInstallJob<'a>,
    ) -> LoaderFuture<'a, Result<LoaderInstalled, LoaderInstallError>> {
        Box::pin(async move { self.run(&job).await })
    }
}

/// 跑一份安装计划（**非泛型的薄包装**）：`execute_with_cancel` 内部逐文件的
/// `stream::iter(..).map(async move ..)` 被装箱成 `dyn Future` 时撞 rustc 的推断限制
/// （`FnOnce is not general enough`，注解该加在那份文件里而它不在本 mission 的 scope）。
///
/// **逐文件进度在这一处统一接上**：收 `job` 而不是收 `cancel` + `sink` 两个参数，是为了让
/// 「调用点忘了传 sink」这件事不存在——两个调用点（原版前置 / loader 库）拿到的都是同一个
/// `job`，适配器只有这一处。M321 r1 之前这里第 4 个实参是 `None`（M320 实测 6.1 min 黑屏）。
pub(super) async fn run_plan(
    engine: &DownloadEngine,
    plan: &InstallPlan,
    job: &LoaderInstallJob<'_>,
) -> Result<InstallReport, InstallError> {
    let flag = job.cancel.flag();
    let sink = file_progress(&job.progress, plan.file_count());
    execute_with_cancel(engine, plan, CONCURRENCY, sink, Some(flag.as_ref())).await
}

/// processor 依赖库的下载（同样是非泛型薄包装，理由见 [`run_plan`]）。
async fn download_processor_libraries(
    engine: &DownloadEngine,
    files: &[PlanFile],
    sink: Option<ProgressSink>,
) -> InstallReport {
    processors::download_libraries(engine, files, CONCURRENCY, sink).await
}

/// 这一跳要带进报告的告警：**loader 计划期**（已含解析期）+ **原版前置那一层**，两者互不重叠。
///
/// 抽成纯函数是为了让「原版那一层的计划告警不许丢」有离线判据（`install_vanilla` 要真引擎）。
/// **processor 期不在这里**：那几条归 [`ProcessorSummary::warnings`]，由 `note_processors`
/// 单独铺一次（两边都填会让同一条告警在报告里出现两次）。
pub(super) fn merge_warnings(plan: &[String], vanilla: Vec<String>) -> Vec<String> {
    let mut warnings = plan.to_vec();
    warnings.extend(vanilla);
    warnings
}

/// 安装期逐文件进度 → 导入进度（[`ImportStage::Loader`](super::super::report::ImportStage)）。
///
/// 三处都走它（installer 依赖库 / 原版 jar+assets / loader 库）。引擎的回调是**并发**的，
/// `done` 用原子计数：同一时刻两个文件各自完成时各加一次，不会"完成了两个只数一个"。
pub(super) fn file_progress(
    sink: &Option<ImportProgressSink>,
    total: usize,
) -> Option<ProgressSink> {
    let sink = sink.clone()?;
    let done = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    Some(std::sync::Arc::new(move |outcome: &FileOutcome| {
        let done = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        tell_to(
            &sink,
            super::super::report::ImportStage::Loader,
            done,
            total,
            Some(outcome.name.clone()),
        );
    }))
}

/// 安装错误 → 这一跳的失败（**取消单列一档**，不是失败）。
fn install_error(error: InstallError) -> LoaderInstallError {
    if error.kind() == InstallErrorKind::Cancelled {
        return LoaderInstallError::Cancelled;
    }
    let failed = LoaderInstallError::failed(error.message().to_string());
    match error.hint() {
        Some(hint) => failed.with_hint(hint),
        None => failed,
    }
}

/// 加载器元数据/计划错误 → 失败（`loader_*` 大类码原样带进说明）。
fn loader_error(error: LoaderError) -> LoaderInstallError {
    let failed =
        LoaderInstallError::failed(format!("{}（{}）", error.message(), error.kind().as_str()));
    match error.hint() {
        Some(hint) => failed.with_hint(hint),
        None => failed,
    }
}

/// processor 错误 → 失败（保留 `processor_*` 大类码，便于按码搜日志）。
fn processor_error(error: processors::ProcessorError) -> LoaderInstallError {
    let failed =
        LoaderInstallError::failed(format!("{}（{}）", error.message(), error.kind().as_str()));
    match error.hint() {
        Some(hint) => failed.with_hint(hint),
        None => failed,
    }
}

/// 「有文件没下全」→ 失败（列前三条明细，与 CLI / presenter 同一形状）。
fn incomplete(what: &str, report: &InstallReport) -> LoaderInstallError {
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
    LoaderInstallError::failed(format!(
        "{what}没下全（{} 个失败，共 {} 个）：{}",
        report.failed(),
        report.files.len(),
        details.join("；")
    ))
    .with_hint("网络恢复后重试一次：已下好的文件会按 sha1 命中跳过，不会重下")
}

/// 版本 id → 可做目录名的片段（`+` 与路径分隔符都换掉，与 CLI / presenter 同规则）。
fn safe_name(raw: &str) -> String {
    raw.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_end_matches('.')
        .to_string()
}
