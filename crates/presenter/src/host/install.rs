//! 安装：新建实例向导 / 下载页「安装」→ core 安装管线 → 登记实例。
//!
//! 与 CLI `ull install` 同序：`RemoteSource::resolve → InstallPlan::build → execute → register`。
//! Forge/NeoForge 多一段 installer processors（[`super::install_processors`]）：
//! `ProcessorPlan::build → 依赖库下载 → 选 Java → 串行 java -cp`，**跑完才写版本 JSON**。
//! 差异只在线程模型：进度经 [`super::job::HostTask::Install`] 回流（文件计数 + 字节 + 速度/ETA），
//! 且整段跑在后台线程里（`block_on` 不占 UI 线程）。
//!
//! 加载器范围：原版 + Fabric/Quilt/Forge/NeoForge（四家，与 CLI 对齐）。OptiFine/LiteLoader
//! 两家 core 侧都还没有安装管线，仍在 GUI 里显式拒装并说清去哪装。
//! 失败一律落 `.ULL/logs/install.log`（[`super::install_journal`]），文案里带日志路径。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use unlucky_core::game::install::{InstallError, InstallReport};

use crate::dto::{CreateRequest, InstallStage, LoaderKind};
use crate::toast::{Toast, ToastKind};

use super::env::HostEnv;
use super::install_journal::Journal;
use super::install_processors;
use super::job::{spawn, HostTask, JobSender};
use super::meter::Meter;

mod routes;

use routes::{install_loader, install_vanilla};

/// 文件级并发上限（与 CLI 同值：16 路压满带宽又不至于打爆镜像）。
pub const CONCURRENCY: usize = 16;

/// 用户取消的稳定文案。
///
/// 三件事一句话说清（D21 的口径：半成品不许冒充成品，断点材料不留在半截状态）：
/// ① 怎么停的（`execute_with_cancel` 不再起新文件，在飞的那个就地掐断）；
/// ② 什么被留着（**已经下完**的文件，重装续用）；
/// ③ 什么一定没发生（实例未登记、版本清单没落盘；在飞文件的 `.ullpart` 由 `PartFile::Drop` 清掉）。
pub const CANCELLED: &str =
    "已取消：剩下的文件没有开始下载，已下好的文件保留（重装续用），实例未登记";

/// 一次安装任务的运行上下文（后台线程内一路传下去，免得每个函数排七八个参数）。
struct Job<'a> {
    env: &'a HostEnv,
    tx: &'a JobSender,
    meter: &'a Arc<std::sync::Mutex<Meter>>,
    cancel: &'a Arc<AtomicBool>,
    journal: &'a Journal,
}

impl Job<'_> {
    /// processor 段的进度出口（同一套 channel/meter/journal）。
    fn reporter(&self) -> install_processors::Reporter<'_> {
        install_processors::Reporter {
            tx: self.tx,
            meter: self.meter,
            journal: self.journal,
        }
    }
}

/// 加载器徽标（`原版` / `Fabric 0.16.5`）。
pub fn loader_label(kind: LoaderKind, version: &str) -> String {
    if kind == LoaderKind::Vanilla || version.trim().is_empty() {
        kind.label().to_string()
    } else {
        format!("{} {}", kind.label(), version)
    }
}

/// GUI 侧支持的加载器（五选：原版 + 四家加载器，与 CLI 的 `--loader` 词表一致）。
///
/// 写成穷举 `matches!` 而不是 `true`：将来 core 加了第六种加载器，这里默认拒绝，
/// 由 [`unsupported_reason`] 给出明确说法，而不是让它走进不属于它的安装分支。
pub fn supported(kind: LoaderKind) -> bool {
    matches!(
        kind,
        LoaderKind::Vanilla
            | LoaderKind::Fabric
            | LoaderKind::Quilt
            | LoaderKind::Forge
            | LoaderKind::NeoForge
    )
}

/// 不支持的加载器 → 给用户的可执行建议。
pub fn unsupported_reason(kind: LoaderKind) -> String {
    format!(
        "{} 还没有接入安装管线（目前支持原版 + Fabric/Quilt/Forge/NeoForge）；请先在实例页导入已有版本，或换成受支持的加载器。",
        kind.label()
    )
}

/// 后台安装（向导提交 / 下载页行内安装共用）。
///
/// `epoch` 是本次任务的代次（[`super::CoreHost`] 的 `install_epoch`）：它随每条进度回流，
/// 让 host 丢掉「已被取消 / 被下一轮顶掉」的旧任务的后续事件。
pub fn spawn_install(
    env: HostEnv,
    tx: JobSender,
    request: CreateRequest,
    cancel: Arc<AtomicBool>,
    epoch: u64,
) {
    spawn("install", move || {
        let meter = Arc::new(std::sync::Mutex::new(Meter::new(&request, epoch)));
        let journal = Journal::open(&env);
        journal.line(&format!(
            "—— 开始安装 {}（{} {}）——",
            request.instance_name(),
            loader_label(request.loader, ""),
            request.version
        ));
        let job = Job {
            env: &env,
            tx: &tx,
            meter: &meter,
            cancel: &cancel,
            journal: &journal,
        };
        let result = run(&job, &request);
        let mut progress = meter
            .lock()
            .map(|guard| guard.progress.clone())
            .unwrap_or_else(|_| Meter::new(&request, epoch).progress);
        match result {
            Ok(name) => {
                journal.line(&format!("完成：实例 {name} 已登记"));
                progress.stage = InstallStage::Done;
                progress.files_done = progress.files_total;
                progress.message = format!("已安装 {name}（{}）", progress.loader);
                let _ = tx.send(HostTask::InstallDone {
                    progress: Box::new(progress),
                    result: Ok(name),
                    epoch,
                });
                let _ = tx.send(HostTask::Toast(Box::new(Toast {
                    id: 0,
                    kind: ToastKind::Success,
                    text: "安装完成，实例已登记".to_string(),
                    remaining_ms: 0,
                })));
            }
            Err(reason) => {
                let cancelled = reason == CANCELLED;
                progress.stage = if cancelled {
                    InstallStage::Cancelled
                } else {
                    InstallStage::Failed
                };
                // 失败文案带上日志路径：面板只有一行、Toast 4 秒就消失，
                // 装日志是用户事后能自己回看的唯一渠道（进度板之外）。
                let text = if cancelled {
                    journal.line(CANCELLED);
                    reason
                } else {
                    journal.line(&format!("安装未完成：{reason}"));
                    format!("{reason}（日志：{}）", journal.path().display())
                };
                progress.message.clone_from(&text);
                // 版本安装这条路的失败不是内容库的分类（它是本地/运行期错误），归到 Other：
                // 大类字段仍然要填，面板才不会把「装坏了」误画成「没联网」。
                progress.failure_kind = Some(crate::dto::ContentFailureKind::Other);
                let _ = tx.send(HostTask::InstallDone {
                    progress: Box::new(progress),
                    result: Err(crate::dto::ContentFailure::new(
                        crate::dto::ContentFailureKind::Other,
                        text,
                    )),
                    epoch,
                });
            }
        }
    });
}

/// 真安装（同步跑完，调用方在后台线程里）。
fn run(job: &Job<'_>, request: &CreateRequest) -> Result<String, String> {
    if request.version.trim().is_empty() {
        return Err("没有选择版本".to_string());
    }
    if !supported(request.loader) {
        return Err(unsupported_reason(request.loader));
    }
    let name = request.instance_name();
    let env = job.env;
    let engine = super::engine::engine(env);
    {
        let mut guard = job.meter.lock().map_err(|_| "进度锁失效".to_string())?;
        guard.stage(InstallStage::Planning, "正在解析版本清单…");
        let _ = job.tx.send(HostTask::Install {
            progress: Box::new(guard.progress.clone()),
            epoch: guard.epoch,
        });
    }

    // 段边界检查（M105）：`execute_with_cancel` 自己会掐断在飞的文件，这里管的是**段与段之间**
    // ——原版段与加载器段、文件段与登记之间，取消都必须在下一段开工前生效。
    if job.cancel.load(Ordering::SeqCst) {
        return Err(CANCELLED.to_string());
    }
    let version_id = if request.loader == LoaderKind::Vanilla {
        install_vanilla(job, &engine, request)?
    } else {
        install_loader(job, &engine, request)?
    };

    // **取消之后绝不登记实例**（M105）：文件段已经在跑了，下完那一刻用户可能早已点了取消；
    // 这里不拦的话，取消会留下一个「用户以为没装、其实已经装好并登记」的实例——
    // 那正是「半成品污染」里最难解释的一种（列表里凭空多一条）。
    if job.cancel.load(Ordering::SeqCst) {
        job.journal
            .line("已取消：文件段跑完但用户已取消，不登记实例（版本清单是否落盘见上）");
        return Err(CANCELLED.to_string());
    }

    {
        let mut guard = job.meter.lock().map_err(|_| "进度锁失效".to_string())?;
        guard.stage(InstallStage::Registering, "正在登记实例…");
        let _ = job.tx.send(HostTask::Install {
            progress: Box::new(guard.progress.clone()),
            epoch: guard.epoch,
        });
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

/// 安装错误 → 用户可读文案（**取消不算失败**：给那句稳定文案，不给「失败 + 日志路径」）。
///
/// 判据用 job 自己的取消标志而不是错误大类：core 的 `execute_with_cancel` 只在阶段边界与
/// 每个文件开工前看标志，取消与「最后一个文件刚好失败」可能同时成立——那种情况下用户点了取消，
/// 文案就该说取消（否则他会以为取消导致了失败）。
fn job_error(job: &Job<'_>, error: InstallError) -> String {
    if job.cancel.load(Ordering::SeqCst) {
        return CANCELLED.to_string();
    }
    install_error(error)
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
