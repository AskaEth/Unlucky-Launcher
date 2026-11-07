//! 「一键装入指定实例」（M109）。
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

use unlucky_core::download::blocking::block_on;
use unlucky_core::download::{DownloadRequest, ResourceKind};
use unlucky_core::game::install::instance::read_instance_config;
use unlucky_core::game::isolation::derive_game_dir;

use crate::dto::{
    loader_facet, ContentInstallRequest, ContentSource, InstallProgressView, InstallStage,
    InstanceSummary,
};

use super::super::env::HostEnv;
use super::super::job::{HostTask, JobSender};
use super::http::{encode, get};
use super::sources::MODRINTH;

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
            "整合包安装尚未开放；内容库对整合包只做检索",
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
        Err(reason) => {
            progress.stage = if cancel.load(Ordering::SeqCst) {
                InstallStage::Cancelled
            } else {
                InstallStage::Failed
            };
            let _ = tx.send(HostTask::InstallDone {
                progress: Box::new(progress),
                result: Err(reason),
                epoch,
            });
        }
    }
}

fn fail(tx: &JobSender, mut progress: InstallProgressView, reason: &str, epoch: u64) {
    progress.stage = InstallStage::Failed;
    progress.message = reason.to_string();
    let _ = tx.send(HostTask::InstallDone {
        progress: Box::new(progress),
        result: Err(reason.to_string()),
        epoch,
    });
}

/// 真正干活的那一段，返回一句人读的落盘描述。
fn run(
    env: &HostEnv,
    request: &ContentInstallRequest,
    cancel: &AtomicBool,
    progress: &mut InstallProgressView,
    tx: &JobSender,
    epoch: u64,
) -> Result<String, String> {
    let instance = find_instance(env, &request.instance)?;
    let game_dir = game_dir_of(env, &instance)?;
    let dir = request
        .kind
        .dir()
        .ok_or_else(|| "该内容类型不支持单文件装入".to_string())?;
    let target_dir = game_dir.join(dir);
    std::fs::create_dir_all(&target_dir)
        .map_err(|error| format!("建目录 {} 失败：{error}", target_dir.display()))?;

    let file = resolve_file(env, request)?;
    if cancel.load(Ordering::SeqCst) {
        return Err("已取消（下载开始前）".to_string());
    }
    progress.stage = InstallStage::Downloading;
    progress.message = file.name.clone();
    let _ = tx.send(HostTask::Install {
        progress: Box::new(progress.clone()),
        epoch,
    });

    let dest = target_dir.join(&file.name);
    let mut download = DownloadRequest::new(ResourceKind::ModFile, file.url, dest.clone());
    if let Some(sha1) = file.sha1.as_deref().filter(|hex| !hex.is_empty()) {
        download = download.with_sha1(sha1);
    }
    block_on(super::super::engine::engine(env).download(download))
        .map_err(|error| format!("下载失败：{error}"))?;
    Ok(format!("{}（{}）", file.name, dest.display()))
}

fn find_instance(env: &HostEnv, id: &str) -> Result<InstanceSummary, String> {
    super::super::game::instances(env)
        .into_iter()
        .find(|instance| instance.id == id)
        .ok_or_else(|| format!("找不到实例 {id}"))
}

/// 实例的运行目录（D20 覆盖链：实例 `isolation` > 全局 `instances.mode`）。
fn game_dir_of(env: &HostEnv, instance: &InstanceSummary) -> Result<PathBuf, String> {
    let global = super::super::game::global_mode(env);
    let config = read_instance_config(&env.layout, &instance.id)
        .map_err(|error| format!("读实例设置失败：{error}"))?;
    derive_game_dir(
        &env.game,
        &instance.version_id,
        global,
        config.as_ref(),
        None,
    )
    .map(|derivation| derivation.game_dir)
    .map_err(|error| format!("隔离目录派生失败：{error}"))
}

/// 要下载的那个文件。
struct RemoteFile {
    url: String,
    name: String,
    sha1: Option<String>,
}

/// 解析「这个项目下最适合目标实例的那个文件」。
fn resolve_file(env: &HostEnv, request: &ContentInstallRequest) -> Result<RemoteFile, String> {
    match request.source {
        ContentSource::Modrinth => modrinth_file(env, request),
        ContentSource::Curseforge => Err(
            "CurseForge 的文件解析未接通（要带 key 的文件端点；见报告「没验的部分」）".to_string(),
        ),
    }
}

fn modrinth_file(env: &HostEnv, request: &ContentInstallRequest) -> Result<RemoteFile, String> {
    let mut url = format!(
        "{MODRINTH}/project/{}/version?game_versions={}",
        request.project,
        encode(&format!("[\"{}\"]", request.game_version))
    );
    if let Some(facet) = loader_facet(request.loader) {
        url.push_str(&format!("&loaders={}", encode(&format!("[\"{facet}\"]"))));
    }
    let body = get(env, &url, &[])?;
    let json: serde_json::Value = serde_json::from_str(&body)
        .map_err(|error| format!("Modrinth 版本列表不是合法 JSON：{error}"))?;
    let versions = json
        .as_array()
        .ok_or_else(|| "Modrinth 版本列表不是数组".to_string())?;
    let first = versions
        .first()
        .ok_or_else(|| "这个项目没有匹配目标实例（版本 / 加载器）的文件".to_string())?;
    let files = first
        .get("files")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "版本里没有 files".to_string())?;
    // 显式选 `primary: true`（Modrinth 官方口径）：取第一个会拿到 sources jar 这类坑
    let file = files
        .iter()
        .find(|file| {
            file.get("primary")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        })
        .or_else(|| files.first())
        .ok_or_else(|| "版本里没有可用文件".to_string())?;

    let url = file
        .get("url")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "文件没有 url".to_string())?;
    let name = file
        .get("filename")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "文件没有 filename".to_string())?;
    let sha1 = file
        .get("hashes")
        .and_then(|hashes| hashes.get("sha1"))
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);
    Ok(RemoteFile {
        url: url.to_string(),
        name: safe_file_name(name),
        sha1,
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
