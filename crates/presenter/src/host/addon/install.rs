//! 「一键装入指定实例」（M109）。
//!
//! ## 网络只走 code 的在线内容源（M162 r1）
//!
//! 解析「哪个文件」用 `core::addon::online::Modrinth::versions`（同一个引擎传输层 ⇒ 信号量、
//! 源健康度、退避都在引擎里），下载用 `online::download_verified`（sha1 + sha512 双检）。
//! 本文件**不再**自己拼 `api.modrinth.com` 的 URL，也不再自己 `serde_json::from_str`。
//!
//! ## 它到底做了什么（诚实版）
//!
//! | 内容类型 | 做什么 | 状态 |
//! |---|---|---|
//! | Mod / 资源包 / 光影 | 解析「该项目下匹配目标实例（版本 + 加载器）的那个文件」→ core 下载引擎下载到实例运行目录的 `mods/` `resourcepacks/` `shaderpacks/` → 进度经 `InstallProgressView` 回流（**复用下载页那块面板**） | 已接 |
//! | 整合包 | 一律 `Failed` + 一句原因 | **未做**：core 没有「远程整合包装进实例」的管线（M111） |
//! | CurseForge 文件解析 | 一律 `Failed` + 一句原因 | **未做**：需要带 key 的文件端点，本轮只做了 CF 的**检索** |
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
    download_verified, EngineArtifactFetcher, EngineTransport, Modrinth, VersionQuery,
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
use super::sources::failure_of;

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
        files_total: 1,
        ..InstallProgressView::new()
    };

    // 整合包：core 没有这条管线（M111），**不**假装能装。UI 侧按钮本来就是禁用的，
    // 这里再挡一次是为了「绕过 UI 直接发指令」也不会静默做半件事。
    if request.kind.dir().is_none() {
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
            progress.stage = InstallStage::Done;
            progress.files_done = 1;
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

/// 真正干活的那一段，返回一句人读的落盘描述（失败带**大类**，离线不等同于「查无此项」）。
fn run(
    env: &HostEnv,
    request: &ContentInstallRequest,
    cancel: &AtomicBool,
    progress: &mut InstallProgressView,
    tx: &JobSender,
    epoch: u64,
) -> Result<String, ContentFailure> {
    let instance = find_instance(env, &request.instance)?;
    let game_dir = game_dir_of(env, &instance)?;
    let dir = request
        .kind
        .dir()
        .ok_or_else(|| local("该内容类型不支持单文件装入"))?;
    let target_dir = game_dir.join(dir);
    std::fs::create_dir_all(&target_dir)
        .map_err(|error| local(format!("建目录 {} 失败：{error}", target_dir.display())))?;

    let file = resolve_file(env, request)?;
    if cancel.load(Ordering::SeqCst) {
        return Err(local("已取消（下载开始前）"));
    }
    progress.stage = InstallStage::Downloading;
    progress.message = file.name.clone();
    let _ = tx.send(HostTask::Install {
        progress: Box::new(progress.clone()),
        epoch,
    });

    let dest = target_dir.join(&file.name);
    // M162：走 `online::download_verified`——sha1 交给引擎流式校验、sha512 落盘后复核，
    // 失败重试且不留半截文件（内容库安装与整合包导入用**同一段**校验，不各写一份）。
    let core_engine = super::super::engine::engine(env);
    let fetcher = EngineArtifactFetcher::new(&core_engine);
    block_on(download_verified(
        &fetcher,
        &file.url,
        file.sha1.as_deref(),
        file.sha512.as_deref(),
        file.size,
        &dest,
    ))
    .map_err(failure_of)?;
    Ok(format!("{}（{}）", file.name, dest.display()))
}

/// 本地失败（找实例 / 建目录 / 取消）——不是网络问题，归到 `Other`。
fn local(reason: impl Into<String>) -> ContentFailure {
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

/// 要下载的那个文件（摘要两档都带着：mrpack 的必填摘要是 sha512）。
struct RemoteFile {
    url: String,
    name: String,
    sha1: Option<String>,
    sha512: Option<String>,
    size: Option<u64>,
}

/// 解析「这个项目下最适合目标实例的那个文件」。
fn resolve_file(
    env: &HostEnv,
    request: &ContentInstallRequest,
) -> Result<RemoteFile, ContentFailure> {
    match request.source {
        ContentSource::Modrinth => modrinth_file(env, request),
        ContentSource::Curseforge => Err(ContentFailure::unsupported(
            "CurseForge 的文件解析未接通（要带 key 的文件端点；见报告「没验的部分」）",
        )),
    }
}

/// 版本列表 → 目标文件：走 core 的在线内容源（**本文件不再拼 URL、不再自己解析 JSON**）。
///
/// 选 `primary: true` 的那个（Modrinth 官方口径）：直接取第一个会拿到 `sources.jar` 这类坑，
/// `Version::primary_file()` 就是为这件事存在的。
fn modrinth_file(
    env: &HostEnv,
    request: &ContentInstallRequest,
) -> Result<RemoteFile, ContentFailure> {
    let core_engine = super::super::engine::engine(env);
    let transport = EngineTransport::new(&core_engine);
    let client = Modrinth::new(&transport);
    let versions = block_on(client.versions(&VersionQuery {
        project: request.project.clone(),
        game_version: Some(request.game_version.clone()),
        loader: loader_facet(request.loader).map(str::to_string),
    }))
    .map_err(failure_of)?;
    let version = versions.first().ok_or_else(|| {
        ContentFailure::new(
            ContentFailureKind::NotFound,
            "这个项目没有匹配目标实例（版本 / 加载器）的文件",
        )
    })?;
    let file = version
        .primary_file()
        .ok_or_else(|| ContentFailure::new(ContentFailureKind::NotFound, "版本里没有可用文件"))?;
    if file.url.trim().is_empty() {
        return Err(ContentFailure::new(
            ContentFailureKind::Unsupported,
            "版本里的主文件没有下载地址",
        ));
    }
    Ok(RemoteFile {
        url: file.url.clone(),
        name: safe_file_name(&file.filename),
        sha1: file.sha1().map(ToString::to_string),
        sha512: file.sha512().map(ToString::to_string),
        size: (file.size > 0).then_some(file.size),
    })
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
