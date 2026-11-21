//! `ull java install`：回退链查版本 → 下载 → 解压 → `java -version` 验证（以及 `--dry-run`）。
//!
//! 从 `commands/java.rs` 拆出来只为守 400 行硬线（M280 的 `--version` / `--dry-run` 让那边到
//! 421 行）；`list` / 设置读取 / 渲染助手仍在 `java.rs`。与 `java_launch.rs` / `java_vendors.rs`
//! 同一个拆法。
//!
//! 版本这一维（M280）：`--version` 不给就是该家**最新 GA**（与加旗标之前逐字节相同）；
//! 给了就要求**精确**那一版——能按版本取的只有 Temurin / Oracle，Microsoft 只认它内置的
//! 那份快照，Zulu 只有 latest。钉不了时 core 报错并列出可用版本，**绝不回落到最新**。

use serde_json::json;

use unlucky_core::download::blocking::block_on;
use unlucky_core::download::DownloadEngine;
use unlucky_core::game::java::{self, InstallRequest, JdkVendor};

use super::java::{build_engine, candidate_json, map_java_error, JavaSettings};
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::Report;

/// 可自动安装的最老大版本（再老的 Temurin 没有 GA 产物）。
const MIN_INSTALLABLE_MAJOR: u32 = 8;

/// `ull java install <major> [--vendor X] [--version V] [--dry-run]`：
/// 回退链查版本 → 下载 → 解压 → `java -version` 验证。
///
/// 幂等：同一个归档装过就直接复用（不联网、不解压）。下载走 `core::download` 引擎，
/// 国内环境按 D7 的地区策略优先院校/国内镜像。
///
/// `--dry-run` 只查版本：打印最终会打哪个版本串、哪个 URL，一个字节都不落盘。
///
/// # Errors
///
/// 大版本不合法、平台无产物、该家没有这个版本、所有发行版都失败（网络/校验/解压/跑不起来）时返回用户错误。
pub fn install(
    ctx: &Ctx,
    major: u32,
    vendor: Option<JdkVendor>,
    version: Option<&str>,
    dry_run: bool,
) -> Result<Report, CliError> {
    if !(MIN_INSTALLABLE_MAJOR..=99).contains(&major) {
        return Err(CliError::usage(format!(
            "大版本号看起来不对：{major}（可用 8 / 11 / 17 / 21 / 25 这类 GA 大版本）"
        )));
    }
    let settings = JavaSettings::load(ctx);
    let engine = build_engine(ctx);
    let install_root = settings.location(ctx)?;
    let request = match vendor {
        Some(vendor) => {
            InstallRequest::with_vendor(major, vendor).with_priority(settings.priority.clone())
        }
        None => InstallRequest::new(major).with_priority(settings.priority.clone()),
    }
    .with_version(version.map(str::to_string));
    if dry_run {
        return dry_run_report(&engine, &install_root, &request);
    }
    let installed =
        block_on(java::install(&engine, &install_root, request.clone())).map_err(map_java_error)?;

    let data = json!({
        "major": installed.major,
        "vendor": installed.vendor.id(),
        "release": installed.release.release_name,
        "version": installed.release.version,
        "requested_version": request.version,
        "semver": installed.release.semver,
        "archive": installed.release.archive.name,
        "sha256": installed.release.archive.sha256,
        "size": installed.release.archive.size,
        "root": installed.root.display().to_string(),
        "install_mode": installed.mode.as_str(),
        "binary": installed.binary().display().to_string(),
        "reused": installed.reused(),
        "source": installed.downloaded,
        "bytes": installed.bytes,
        "files": installed.files,
        "extracted": installed.extracted,
        "chain": installed.chain.to_json(),
        "runtime": candidate_json(&installed.runtime),
    });

    let mut human = Vec::new();
    if installed.reused() {
        human.push(format!(
            "Java {}（{}）已就绪（{}），未走网络",
            installed.major,
            installed.vendor.id(),
            installed.root.display()
        ));
    } else {
        human.push(format!(
            "已安装 Java {}（{}，来源 {}，{} 字节 / {} 个文件）",
            installed.major,
            installed.release.summary(),
            installed.downloaded.as_deref().unwrap_or("?"),
            installed.bytes,
            installed.files
        ));
    }
    if let Some(wanted) = &request.version {
        human.push(format!(
            "指定版本：{wanted} → 命中 {}",
            installed.release.version
        ));
    }
    human.push(format!("落点：{}", installed.location_line()));
    human.push(format!("可执行文件：{}", installed.binary().display()));
    if let Some(version) = &installed.runtime.version {
        human.push(format!("探测：{version}"));
    }
    for note in installed.chain.degradations() {
        human.push(format!("降级：{note}"));
    }
    human.push("`%prog% launch` 会自动优先用它".to_string());
    Ok(Report::new(data, human))
}

/// `--dry-run`：查一次版本（与真装走同一条选择路径），打印版本串与 URL，不下载、不解压、不写盘。
///
/// 查版本本身仍要联网，但它把「会装哪一版、打哪个 URL」在下载之前摆出来——
/// 验证版本选择逻辑不必先下 200 MB。
fn dry_run_report(
    engine: &DownloadEngine,
    location: &java::JavaInstallRoot,
    request: &InstallRequest,
) -> Result<Report, CliError> {
    let (release, root, ready) =
        block_on(java::plan(engine, location, request)).map_err(map_java_error)?;
    let data = json!({
        "dry_run": true,
        "major": release.major,
        "vendor": release.vendor.id(),
        "version": release.version,
        "requested_version": request.version,
        "semver": release.semver,
        "release": release.release_name,
        "archive": release.archive.name,
        "url": release.archive.url,
        "sha256": release.archive.sha256,
        "size": release.archive.size,
        "root": root.display().to_string(),
        "install_mode": location.mode.as_str(),
        "already_installed": ready,
    });
    let mut human = vec![format!(
        "将安装 Java {}（{}）",
        release.major,
        release.summary()
    )];
    if let Some(wanted) = &request.version {
        human.push(format!("指定版本：{wanted} → 命中 {}", release.version));
    }
    human.push(format!("版本：{}", release.version));
    human.push(format!("URL：{}", release.archive.url));
    human.push(format!(
        "落点：{}（{}）",
        root.display(),
        location.mode.as_str()
    ));
    human.push(format!(
        "已装好：{}",
        if ready {
            "是（会直接复用，不联网）"
        } else {
            "否（会下载）"
        }
    ));
    human.push("（--dry-run：没有下载、没有解压、没有写盘）".to_string());
    Ok(Report::new(data, human))
}
