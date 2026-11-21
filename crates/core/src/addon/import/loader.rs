//! 第六跳：**真的把清单声明的加载器装上**（M311）。
//!
//! # 为什么这一跳必须存在
//!
//! M111~M310 的前五跳（识别格式 / 解析清单 / 下载校验 / 落盘合并 / 导入侧诊断）都是真的，
//! 唯独加载器只被**翻译成计划**：旧文档里那句「本模块不装加载器」听起来是分工，实际后果是
//! 「导入成功、实例登记、看起来一切正常，点启动报 `VersionManifest` + 路径」——用户看不出
//! 是因为导入从不装加载器。所以这里接第六跳，判据三条：
//!
//! | 要求 | 落点 |
//! |------|------|
//! | 加载器版本与清单声明**一致**（不一致就拒，不许"取最近的"） | `install_declared_loader` 的复核闸 + `engine` 的解析闸，判据见 `version`（M332：同一装载器版本的两种写法算一致） |
//! | 安装可失败且**失败可归因** | [`LoaderInstallError`] → `import_loader` 大类 + 可执行 hint |
//! | 失败时实例**不许**登记成「看起来能启动」 | 装载器排在落盘之前：失败 ⇒ [`super::pipeline`] 走 `abandoned`（目标目录一个字节没写、不登记） |
//!
//! # 为什么排在「落盘」之前（这一条是失败语义的命门）
//!
//! 装载器落在**共享分区**（`versions/` / `libraries/` / `assets/`，D20：任何隔离档都不按实例
//! 复制），与实例内容目录无关。所以它可以先装：装不上时暂存目录直接删掉、目标目录**一个字节
//! 都没写**（报告说得出「未被写入」，而不是"写完又回滚"），实例也就不会留下一条指向不存在的
//! 版本 JSON 的档案。反过来（先落盘再装）只能整包回滚，用户等半天的 mods 白下。
//!
//! # 为什么是注入点而不是直接调引擎
//!
//! 与 [`super::fetch`] 同一条理由：**测试不能联网**。生产实现是 [`EngineLoaderInstaller`]
//! （官方 meta + M27 下载引擎 + M35 processors，与 `ull install --loader` 同一套计划层），
//! 单测注入假件即可断言「导入真的请求了装载器」「装载器给的版本 id 与清单不符时拒」。
//!
//! # 刻意不做的事
//!
//! - **不重写**既有安装路径：装载器版本 JSON、库清单、原版前置、installer processors 全部走
//!   `addon::loader` 与 `game::install` 的现成实现（本模块只编排顺序）；
//! - **不改清单**：`LoaderTarget` 里的版本号来自整合包，不允许实现方回写或挑选；反过来，
//!   落盘 id 要**钉成清单声明的那个**（`version::adopt_declared_id`）：实例登记用的是清单的
//!   版本 id，`versions/<id>/` 必须就是它，否则又回到"导入成功、点启动读不到版本 JSON"；
//! - **不登记实例**：登记仍由 [`super::pipeline`] 在装载器成功之后做。

use std::future::Future;
use std::pin::Pin;

use crate::addon::loader::processors::StepSink;
use crate::game::layout::GameLayout;

use super::cancel::{ImportCancel, Stopped};
use super::error::ImportError;
use super::plan::ImportPlan;
use super::report::{
    tell, ImportProgressSink, ImportReport, ImportStage, LoaderTarget, ProcessorSummary,
};
use super::ImportRequest;

mod engine;
mod version;

#[cfg(test)]
mod tests;

pub use engine::EngineLoaderInstaller;

/// trait 方法返回的装箱 future（与 [`super::fetch::FetchFuture`] 同一形状：core 不引
/// `async-trait`）。
///
/// # 为什么**没有** `+ Send`
///
/// 生产实现的 future 里要 `await` 两份既有执行器（`game::install::execute_with_cancel` 与
/// `loader::processors::download_libraries`），它们内部逐文件的 `stream::iter(..).map(async ..)`
/// 在**被装箱成 `dyn Future` 时**会撞上 rustc 的推断限制
/// （`implementation of FnOnce is not general enough`；注解该加在那两处闭包上，而那两份文件都
/// 不在本 mission 的 scope 里）。因此这里不要求 `Send` —— 也**不需要**：两处调用方（GUI 的
/// 导入线程、CLI）都在自己的线程里 `block_on` 整条管线，从不把这条 future 跨线程搬运。
pub type LoaderFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// 一次「装加载器」的输入。
pub struct LoaderInstallJob<'a> {
    /// 装载器要落进去的游戏目录布局（`versions/` `libraries/` `assets/`）。
    ///
    /// 这是**启动器的游戏根**，不是实例内容目录：D20 的三个共享分区恒在游戏根下，
    /// 实例只决定 `gameDir`（mods / config 落哪）。
    pub layout: &'a GameLayout,
    /// 清单声明的加载器（种类 + 原版版本 + 加载器版本 + 版本 id 全来自清单）。
    pub target: &'a LoaderTarget,
    /// 取消令牌：长段（原版下载 / 库下载 / processors）里要看它。
    pub cancel: &'a ImportCancel,
    /// processor 的**逐步骤**回调（M321）：导入侧的 `with_steps(...)` 原样转交给
    /// `processors::run`。`None` = 调用方不要逐步输出，链本身照跑。
    pub steps: Option<StepSink>,
    /// 装载器段的**逐文件**进度（原版 jar+assets / installer 依赖库 / loader 库）→ 导入进度回调。
    ///
    /// 与 `steps` 分开是刻意的：一个是「第几步跑完了」，一个是「第几个文件到位了」，
    /// 界面在两种粒度上用它们。
    pub progress: Option<ImportProgressSink>,
}

/// 装成功之后**可核对**的事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoaderInstalled {
    /// 真的落盘的版本 id（`1.20.1-fabric-0.15.11`）。
    pub version_id: String,
    /// 人读摘要（进报告注记：`Fabric 0.15.11 → 1.20.1-fabric-0.15.11`）。
    pub detail: String,
    /// 解析期 / 计划期 / 原版前置的告警（**实现方不许丢**，编排层原样铺进
    /// [`ImportReport::warnings`]，于是人读输出与 `--json` 都能看到）。
    ///
    /// **不含 processor 期**：那几条归 [`LoaderInstalled::processors`] 的
    /// [`ProcessorSummary::warnings`]，由 `note_processors` 单独铺一次——两边都填会让同一条
    /// 告警在报告里出现两次（`import::loader::tests` 的 `count == 1` 断言钉着这一点）。
    pub warnings: Vec<String>,
    /// processor 链的逐步骤结果（不需要跑 installer 的加载器为 `None`）。
    pub processors: Option<ProcessorSummary>,
}

/// 这一跳的两种结局：**取消不是失败**（与管线其余部分同一口径）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoaderInstallError {
    /// 用户在装载器下载途中点了取消。
    Cancelled,
    /// 装不上。
    Failed {
        /// 人读原因（必须含"是哪个加载器的哪个版本"）。
        message: String,
        /// 下一步（重试 / 换个加载器版本 / 先装 Java）。
        hint: Option<String>,
    },
}

impl LoaderInstallError {
    /// 装不上（没人读说明之外的恢复动作）。
    #[must_use]
    pub fn failed(message: impl Into<String>) -> Self {
        Self::Failed {
            message: message.into(),
            hint: None,
        }
    }

    /// 补一句恢复动作（**失败要指向下一步**，不是只说"失败了"）。
    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        if let Self::Failed { hint: slot, .. } = &mut self {
            *slot = Some(hint.into());
        }
        self
    }

    /// 人读说明（`Cancelled` 也有，便于日志）。
    #[must_use]
    pub fn message(&self) -> &str {
        match self {
            Self::Cancelled => "用户取消（装载器安装途中）",
            Self::Failed { message, .. } => message,
        }
    }

    /// 恢复动作。
    #[must_use]
    pub fn hint(&self) -> Option<&str> {
        match self {
            Self::Cancelled => None,
            Self::Failed { hint, .. } => hint.as_deref(),
        }
    }
}

/// 装加载器的执行体（**注入点**）。
///
/// 实现方必须做到三件事：①按 [`LoaderInstallJob::target`] 里**清单给的**版本装，别的版本
/// 一律拒；②装完 `versions/<version_id>/<version_id>.json` 真的在盘上；③在长段里看取消。
pub trait LoaderInstaller: Send + Sync {
    /// 装一次。
    ///
    /// # Errors
    ///
    /// 用户取消（[`LoaderInstallError::Cancelled`]）、元数据/网络/Java/磁盘任一处失败
    /// （[`LoaderInstallError::Failed`]）。
    fn install<'a>(
        &'a self,
        job: LoaderInstallJob<'a>,
    ) -> LoaderFuture<'a, Result<LoaderInstalled, LoaderInstallError>>;
}

/// 装不上时**总要说得出下一步**（实现方没给 hint 时由这里兜底）。
///
/// 为什么兜底在编排层而不是实现方：网络类失败（`loader_network`）在元数据层拿不到任何
/// 恢复动作，但用户手里其实有两条路——重试，或者先把加载器单独装好再把包装到已有实例上。
fn default_recovery(target: &LoaderTarget) -> String {
    let label = target.kind.label();
    let mut hint = format!("先确认网络能连上 {label} 的官方源再重试一次；");
    hint.push_str(&format!(
        "也可以先单独装好 {label} {}（版本 id 就是 {}），再把这份整合包装到那条实例上",
        target.loader_version, target.version_id
    ));
    hint
}

/// 第六跳的入口（管线只调这一行）：清单声明了加载器就**真装**，否则什么都不做。
///
/// # Errors
///
/// - [`Stopped::Cancelled`]：装载器安装途中被取消（管线按"取消"收尾：目标目录未写入）；
/// - [`Stopped::Failed`]：装不上 / 装回来的不是清单声明的那个版本（`import_loader`），
///   或清单声明了加载器但调用方没接安装器**且**判成了必需（见下）。
///
/// # 「没接安装器」为什么不报错
///
/// 离线导入、只用 core 做解析的调用方（扫描/预检、测试）不该被迫接一个安装器。这时**不装**
/// 但**必须说清没装**（[`ImportReport::note_loader_not_installed`] + `loader_installed = false`）：
/// 静默跳过正是这个 mission 要消灭的假功能。
pub(super) async fn install_declared_loader(
    request: &ImportRequest<'_>,
    plan: &ImportPlan,
    report: &mut ImportReport,
) -> Result<(), Stopped> {
    let Some(target) = plan.loader.as_ref() else {
        return Ok(());
    };
    let Some(seam) = request.loader_install.as_ref() else {
        report.note_loader_not_installed(target);
        return Ok(());
    };
    tell(&request.progress, ImportStage::Loader, 0, 0, None);
    let job = LoaderInstallJob {
        layout: seam.layout,
        target,
        cancel: &request.cancel,
        // M321：这两份以前是 `None` / 没有字段，于是 processor 链跑四十秒一声不响。
        steps: request.steps.clone(),
        progress: request.progress.clone(),
    };
    match seam.installer.install(job).await {
        Ok(done) => {
            // **版本一致性闸**（判据见 [`version`]）：实现方装回来的必须是清单声明的那个版本。
            // 这里再核一次，是因为"找不到就用最近的"最容易藏在实现方里——core 这一侧
            // 宁可整次导入失败，也不许把另一个版本当成清单要求的版本。
            //
            // M332（P36）：判据是**同一个装载器版本**，不是逐字相等——老版 Forge 的 maven 版本串
            // 带双后缀（`1.7.10-forge-10.13.4.1614-1.7.10`），清单写的是短形
            // （`1.7.10-forge-10.13.4.1614`），两者是同一次发布。归一后交集为空仍然拒
            // （`10.13.4.1615` 装回 `1614` 照旧红）。
            version::check_installed(target, &done.version_id)?;
            // 闸③（落盘判据，M332 r1 P2-2）：放行「另一种写法」之后，**清单声明的那个 id 下必须
            // 真的有版本 JSON**——管线登记用的是清单 id，落在别的写法下等于登记一个读不到清单的
            // 实例（正是本模块要消灭的假功能）。生产实现把落盘 id 钉成清单写法，所以这里通常恒过。
            version::check_landed(seam.layout, target, &done.version_id)?;
            report.note_loader_installed(target, &done.detail);
            // M321：实现方带回来的告警与 processor 链的结果**一条都不许丢**。
            // 三份告警同一出口（`report.warnings`）⇒ 人读输出与 `--json` 看到的是同一份数据，
            // GUI 也走这条（presenter 的 convert 直接映射 `report.warnings`）。
            report.note_loader_warnings(&done.warnings);
            if let Some(processors) = done.processors {
                report.note_processors(processors);
            }
            Ok(())
        }
        Err(LoaderInstallError::Cancelled) => Err(Stopped::Cancelled),
        Err(error @ LoaderInstallError::Failed { .. }) => {
            let named = ImportError::loader(format!(
                "加载器 {} {} 没装上：{}；实例未登记（先装好它再重导，或直接把这份整合包装到已有实例上）",
                target.kind.label(),
                target.loader_version,
                error.message()
            ));
            Err(Stopped::Failed(
                named.with_hint(
                    error
                        .hint()
                        .map(str::to_string)
                        .unwrap_or_else(|| default_recovery(target)),
                ),
            ))
        }
    }
}
