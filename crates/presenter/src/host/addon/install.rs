//! 「一键装入指定实例」（M109；M186 起把前置一起装上）。
//!
//! ## 网络只走 code 的在线内容源（M162 r1）
//!
//! 解析「哪个文件」用 `core::addon::online`（同一个引擎传输层 ⇒ 信号量、源健康度、退避都在
//! 引擎里），下载用 `online::download_verified`（sha1 + sha512 双检）。本文件**不再**自己拼
//! `api.modrinth.com` 的 URL，也不再自己 `serde_json::from_str`。
//!
//! ## 它到底做了什么（诚实版）
//!
//! | 内容类型 | 做什么 | 状态 |
//! |---|---|---|
//! | Mod / 资源包 / 光影 | 解析「该项目 + 它的 `required` 前置」在目标实例（版本 + 加载器）下的文件 → core 下载引擎下到 `mods/` `resourcepacks/` `shaderpacks/` → 进度经 `InstallProgressView` 回流 | 已接（M186 起含前置） |
//! | 整合包 | 一律 `Failed` + 一句原因 | **未做**：core 没有「远程整合包装进实例」的管线（M111） |
//! | CurseForge 文件解析 | 一律 `Failed` + 一句原因 | **未做**：需要带 key 的文件端点，本轮只做了 CF 的**检索** |
//!
//! ## 前置的两档失败与重试名单（M186 / M191）
//!
//! 解析在 [`super::plan`] + `core::addon::online::resolve`：某个前置「不兼容所选版本/加载器」
//! 与「取不到（网络 / 项目不存在）」是**两条**记录，本文件把两档分开写进失败文案，并把失败项
//! 列成「可只重试这 N 个」。**这不是自动重试**：重试要用户点（M191 的界面入口）。
//!
//! ## 「只重试这些」真的只重试这些（M191）
//!
//! 上一轮失败时，那份 `DependencyPlan` 被 [`plan::remember`] 记下来；`ContentInstallRequest.retry`
//! 为真的第二次请求走 [`run`] 的重试入口——它调 core 的 `DependencyPlan::retry_request()`，
//! 于是重解**只请求失败的那几个项目**（`present` 重新扫一遍目录，已装好的不会被重下）。
//! 没有记录时回一条失败，**不**退化成「整单重装」：那会把已装好的再下一遍、请求数放大。
//!
//! 下载阶段的失败**也进同一套名单**（M186 r1）：以前 `download_verified` 一失败就整单中断，
//! 用户既看不到「主文件已装、缺 fabric-api」，也拿不到可重试的项目。现在逐个文件下、失败记进
//! `DependencyPlan::record_failure`（`Unreachable` + 叶子成因），已下好的不回滚，最后一起收口。
//!
//! ## 光影与数据包：支持到哪一步（M186 核对过，别写成「都支持」）
//!
//! | 内容 | 检索 | 装入 | 差在哪 |
//! |---|---|---|---|
//! | 光影 | 支持（`project_type:shader`）；命中项的加载器标签现在认得 `iris` / `optifine`（`KNOWN_LOADERS`） | 支持（落 `shaderpacks/`） | 加载器**过滤**仍筛不了：`LoaderKind::OptiFine` 没有 facet（`loader_facet` 返回 `None`），所以「只看 Iris 光影」只能靠标签目测 |
//! | 数据包 | core 的 `ContentType::Datapack` 有 `datapack` facet | **不支持** | DTO 的 `ContentKind` 没有这一项；数据包还要落到「某个世界的 `datapacks/`」，现在没有选择世界这一步 |
//!
//! ## 为什么进度只有阶段级，没有字节条
//!
//! 字节级进度要走 `ProgressEmitter`（core 的 tokio 广播），而 presenter 的依赖表里没有
//! `tokio` / `futures-util`（`Cargo.toml` 不在本 mission 的 scope），拿不到接收端的 `recv()`。
//! 所以这里给的是**阶段 + 文件名**（`Planning` → `Downloading` → `Done`/`Failed`），
//! 面板在「进度未知」时本来就只画一行说明、不画条（`download/install.rs` 的既定口径）。
//! 已登记为后续项：presenter 补 `futures-util` 之后换成字节级。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use unlucky_core::addon::online::{
    resolve, EngineArtifactFetcher, EngineTransport, Modrinth, PlannedNode,
};
use unlucky_core::download::blocking::block_on;
use unlucky_core::game::install::instance::read_instance_config;
use unlucky_core::game::isolation::derive_game_dir;

use crate::dto::{
    loader_facet, ContentFailure, ContentFailureKind, ContentInstallRequest, ContentSource,
    InstallProgressView, InstallStage, InstanceSummary,
};

use super::super::env::HostEnv;
use super::super::job::{HostTask, JobSender};
mod collect;

pub(super) use collect::{collect, Placement, Ports};

use super::{downloads, plan};

/// 跑一次装入（同步；由 [`super::spawn_install`] 放进后台线程）。
pub(super) fn install(
    env: &HostEnv,
    tx: &JobSender,
    request: &ContentInstallRequest,
    cancel: &AtomicBool,
    epoch: u64,
) {
    let mut progress = InstallProgressView {
        version: request.title.clone(),
        instance: request.instance.clone(),
        loader: request.loader.label().to_string(),
        // 文件数要等解析完才知道（含前置），这里留 0 = 未知（面板画不确定态）。
        files_total: 0,
        ..InstallProgressView::new()
    };

    // 整合包：core 没有这条管线（M111），**不**假装能装。UI 侧按钮本来就是禁用的，
    // 这里再挡一次是为了「绕过 UI 直接发指令」也不会静默做半件事。
    // 重试模式不看 `kind`：那种模式下落盘目录来自上一次的计划（见 `run`）。
    if !request.retry && request.kind.dir().is_none() {
        fail(
            tx,
            progress,
            ContentFailure::unsupported("整合包安装尚未开放；内容库对整合包只做检索"),
            epoch,
        );
        return;
    }

    let _ = tx.send(HostTask::Install {
        progress: Box::new(progress.clone()),
        epoch,
    });
    match run(env, request, cancel, &mut progress, tx, epoch) {
        Ok(done) => {
            finish(&mut progress);
            progress.message = done;
            let _ = tx.send(HostTask::InstallDone {
                progress: Box::new(progress),
                result: Ok(request.title.clone()),
                epoch,
            });
        }
        Err(failure) => {
            progress.stage = if cancel.load(Ordering::SeqCst) {
                InstallStage::Cancelled
            } else {
                InstallStage::Failed
            };
            progress.message = failure.detail.clone();
            progress.failure_kind = Some(failure.kind);
            let _ = tx.send(HostTask::InstallDone {
                progress: Box::new(progress),
                result: Err(failure),
                epoch,
            });
        }
    }
}

fn fail(tx: &JobSender, mut progress: InstallProgressView, failure: ContentFailure, epoch: u64) {
    progress.stage = InstallStage::Failed;
    progress.message = failure.detail.clone();
    progress.failure_kind = Some(failure.kind);
    let _ = tx.send(HostTask::InstallDone {
        progress: Box::new(progress),
        result: Err(failure),
        epoch,
    });
}

/// 真正干活的那一段：解析（含前置）→ 逐个下载 → 汇总。
///
/// 返回「装了哪些 / 跳过了哪些」的人读描述；解析阶段的两档失败由
/// [`plan::failure_of_plan`] 变成一条带大类的失败（**前置缺了不算装成**）。
///
/// 两条解析入口，别混：
///
/// | 入口 | 请求从哪来 | 发几次 |
/// |---|---|---|
/// | 首次装入 | 根项目 + 过滤条件（[`plan::request`]） | 一次，从根开始 BFS |
/// | **只重试失败项** | 上一轮计划的 `retry_request()`（[`plan::retry_plan`]） | 一次，**只含失败的那几个项目** |
fn run(
    env: &HostEnv,
    request: &ContentInstallRequest,
    cancel: &AtomicBool,
    progress: &mut InstallProgressView,
    tx: &JobSender,
    epoch: u64,
) -> Result<String, ContentFailure> {
    let instance = find_instance(env, &request.instance)?;

    // 「只重试这些」：输入是上一轮那份计划，**不**重新发起根项目安装。
    // 没有记录时（换了实例 / 进程重启过）回一条失败，而不是退化成「整单重装」。
    if request.retry {
        let Some(saved) = plan::remembered(env, &request.instance) else {
            return Err(local(RETRY_LOST));
        };
        // 面板头继续显示那次的显示名（不是项目 id：r1 复审 N5）
        progress.version = saved.title.clone();
        let present = plan::present_files(&saved.dir);
        let core_engine = super::super::engine::engine(env);
        let dependency_plan = plan::retry_plan(&saved.plan, present, |retry| {
            let transport = EngineTransport::new(&core_engine);
            let client = Modrinth::new(&transport);
            block_on(resolve(&client, retry))
        });
        let fetcher = EngineArtifactFetcher::new(&core_engine);
        let title = |id: &str| plan::fetch_title(env, id);
        let ports = Ports {
            env,
            fetcher: &fetcher,
            title: &title,
        };
        return collect(
            &ports,
            request,
            dependency_plan,
            Placement {
                dir: &saved.dir,
                root: plan::Root {
                    id: &saved.root,
                    title: &saved.title,
                },
            },
            cancel,
            progress,
            tx,
            epoch,
        );
    }

    let game_dir = game_dir_of(env, &instance)?;
    let dir = request
        .kind
        .dir()
        .ok_or_else(|| local("该内容类型不支持单文件装入"))?;
    let target_dir = game_dir.join(dir);
    std::fs::create_dir_all(&target_dir)
        .map_err(|error| local(format!("建目录 {} 失败：{error}", target_dir.display())))?;
    if request.source != ContentSource::Modrinth {
        return Err(ContentFailure::unsupported(
            "CurseForge 的文件解析未接通（要带 key 的文件端点；见报告「没验的部分」）",
        ));
    }

    // 解析：根项目 + 只跟 `required` 的前置（顺序与父子关系由 core 给，这里不重排）。
    let resolve_request = plan::request(
        &request.project,
        &request.game_version,
        loader_facet(request.loader),
        plan::present_files(&target_dir),
    );
    let core_engine = super::super::engine::engine(env);
    let dependency_plan = {
        let transport = EngineTransport::new(&core_engine);
        let client = Modrinth::new(&transport);
        block_on(resolve(&client, &resolve_request))
    };
    let fetcher = EngineArtifactFetcher::new(&core_engine);
    let title = |id: &str| plan::fetch_title(env, id);
    let ports = Ports {
        env,
        fetcher: &fetcher,
        title: &title,
    };
    collect(
        &ports,
        request,
        dependency_plan,
        Placement {
            dir: &target_dir,
            root: plan::Root {
                id: &request.project,
                title: &request.title,
            },
        },
        cancel,
        progress,
        tx,
        epoch,
    )
}

/// 没有可重试的记录时那句话（**不**退化成「整单重装」）。
const RETRY_LOST: &str =
    "没有可重试的记录：这次失败发生在别的实例上、或进程重启过。请重新装入那一次。";

/// 完成态：`files_done` 对齐 `files_total`。
///
/// 别在这里另写数字：以前写死 `1`，装 3 个文件后终态是 `1/3`，下载页只要「已知」就画确定态
/// 进度条 ⇒ 装完了条还停在 1/3（全部跳过时更糟，`1/0`）。
pub(super) fn finish(progress: &mut InstallProgressView) {
    progress.stage = InstallStage::Done;
    progress.files_done = progress.files_total;
}

/// 进度面板上的那一行文件名（前置标出来，用户才知道多出来的文件是什么）。
pub(super) fn node_label(node: &PlannedNode) -> String {
    if node.is_root() {
        node.file.filename.clone()
    } else {
        format!("{}（前置）", node.file.filename)
    }
}

/// 本地失败（找实例 / 建目录 / 取消）——不是网络问题，归到 `Other`。
pub(super) fn local(reason: impl Into<String>) -> ContentFailure {
    ContentFailure::new(ContentFailureKind::Other, reason)
}

fn find_instance(env: &HostEnv, id: &str) -> Result<InstanceSummary, ContentFailure> {
    super::super::game::instances(env)
        .into_iter()
        .find(|instance| instance.id == id)
        .ok_or_else(|| local(format!("找不到实例 {id}")))
}

/// 实例的运行目录（D20 覆盖链：实例 `isolation` > 全局 `instances.mode`）。
fn game_dir_of(env: &HostEnv, instance: &InstanceSummary) -> Result<PathBuf, ContentFailure> {
    let global = super::super::game::global_mode(env);
    let config = read_instance_config(&env.layout, &instance.id)
        .map_err(|error| local(format!("读实例设置失败：{error}")))?;
    derive_game_dir(
        &env.game,
        &instance.version_id,
        global,
        config.as_ref(),
        None,
    )
    .map(|derivation| derivation.game_dir)
    .map_err(|error| local(format!("隔离目录派生失败：{error}")))
}

/// 只留文件名本身：站点给的名字可能带路径分隔符，落盘前必须砍掉
/// （与 `core::addon::pack::path` 的穿越双检同一条纪律）。
pub(super) fn safe_file_name(raw: &str) -> String {
    let name = raw
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(raw)
        .trim()
        .to_string();
    if name.is_empty() || name == "." || name == ".." {
        "download.bin".to_string()
    } else {
        name
    }
}
