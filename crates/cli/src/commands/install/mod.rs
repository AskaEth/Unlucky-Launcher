//! `ull install <version> [--loader <kind>] [--loader-version <v>]`：装版本本体 / 装加载器层。
//!
//! ```text
//! --loader 缺省        → vanilla：M27 原版安装（resolve → plan → execute → register）
//! --loader fabric|quilt → addon::loader：原版（已装则跳）→ loader 版本 JSON → 补 loader 库 → register
//! --loader neoforge|forge → 同上；需要 installer processors 的版本再跑 M35 执行器（依赖库 → java → 版本 JSON）
//! ```
//!
//! CLI 只做四件事：建下载引擎、把计划/结果排版成人读与 `--json` 两种形态
//! （[`report`] / [`report_loader`]）、把错误翻译成稳定错误码、往 stderr 打实时进度。
//! 所有版本/继承/库/下载逻辑都在 core（CLI 不复制任何路径或镜像规则）。

mod loader;
mod loader_processors;
mod report;
mod report_loader;
mod vanilla;

use std::sync::Arc;

use unlucky_core::addon::loader::LoaderKind;
use unlucky_core::download::cache::now_ms;
use unlucky_core::download::region::{decide_from, load_cached, store_cached};
use unlucky_core::download::region_system::detect_signals;
use unlucky_core::download::{
    ContentCache, DownloadEngine, DownloadErrorKind, EngineConfig, HealthRegistry, MirrorTable,
};
use unlucky_core::game::install::{InstallError, InstallReport, Registration};

use crate::cli::InstallArgs;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::Report;

/// 文件级并发上限：资源是海量小文件，16 路足够压满带宽又不至于打爆镜像。
pub(super) const CONCURRENCY: usize = 16;

/// 执行 `install`：按 `--loader` 分发。
///
/// # Errors
///
/// 用法错误、加载器未知、版本不存在、网络失败、校验不符、写盘失败、实例清单损坏。
pub fn run(ctx: &Ctx, args: &InstallArgs) -> Result<Report, CliError> {
    match args
        .loader
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        None | Some("vanilla") | Some("none") => vanilla::run(ctx, args),
        Some(raw) => match LoaderKind::parse(raw) {
            Some(kind) => loader::run(ctx, args, kind),
            None => Err(
                CliError::user("loader_unknown", format!("未知加载器 `{raw}`"))
                    .with_hint("--loader 取值：vanilla / fabric / quilt / neoforge / forge"),
            ),
        },
    }
}

/// 登记实例（幂等）：`profiles.json` + `config/instances/<id>.json`。
///
/// 原版安装与加载器安装共用：前者 `version_id` = 原版 id，后者 = loader 版本 id
/// （实例的 `versionId` 必须指向 loader 版本，否则启动会跑成原版）。
pub(super) fn register_instance(
    ctx: &Ctx,
    name: &str,
    version_id: &str,
) -> Result<Registration, CliError> {
    let created_at =
        unlucky_core::launch::time::format_rfc3339_millis(unlucky_core::launch::time::now());
    unlucky_core::game::install::instance::register(
        &ctx.layout(),
        name,
        version_id,
        Some(&created_at),
    )
    .map_err(|error| CliError::user("instance_register", format!("登记实例失败：{error}")))
}

/// 建下载引擎：内容寻址缓存落 `.ULL/cache/`，地区判定走 24h 缓存 + 环境信号。
///
/// 地区判定**不做网络探测**（P2 首版；探测开关属设置层接线），
/// 因此中文环境默认镜像优先、其它环境直连——与 D7 的"不探测时"约定一致。
pub(super) fn build_engine(ctx: &Ctx) -> DownloadEngine {
    let layout = ctx.layout();
    let cache_root = layout.cache();
    let now = now_ms();
    let region_path = cache_root.join("region.json");
    let region = load_cached(&region_path, now).unwrap_or_else(|| {
        let profile = decide_from(&detect_signals(), now);
        let _ = store_cached(&region_path, &profile);
        profile
    });
    DownloadEngine::new(
        EngineConfig::default(),
        Arc::new(MirrorTable::builtin()),
        Arc::new(region),
        Arc::new(HealthRegistry::open_in(&cache_root, now)),
        Arc::new(ContentCache::open(&cache_root)),
    )
}

/// 同名但绑定别的版本 → 冲突；同名同版本视为重装（复用实例，允许）。
pub(super) fn ensure_name_free(ctx: &Ctx, name: &str, version: &str) -> Result<(), CliError> {
    let profiles = super::instances::read_profiles(ctx)?;
    let conflict = profiles
        .profiles
        .iter()
        .find(|profile| profile.name == name && profile.version_id != version);
    match conflict {
        Some(profile) => Err(CliError::user(
            "instance_exists",
            format!("同名实例已存在：{name}（绑定版本 {}）", profile.version_id),
        )
        .with_hint("用 --name 换个名字，或 `%prog% instances list` 看看现有实例")),
        None => Ok(()),
    }
}

/// 安装期错误 → CLI 错误（保留稳定错误码与提示）。
pub(super) fn map_install_error(error: &InstallError, ctx: &Ctx) -> CliError {
    let hint = match error.hint() {
        Some(hint) => format!("{hint}（游戏根：{}）", ctx.game().root().display()),
        None => format!("游戏根：{}", ctx.game().root().display()),
    };
    CliError::user(error.kind().as_str(), error.message().to_string()).with_hint(hint)
}

/// 有文件失败 → 按"最严重的失败类别"给稳定码，并列出前几条明细。
pub(super) fn failure_error(report: &InstallReport, ctx: &Ctx) -> CliError {
    let code = match report.failure_kind() {
        Some(DownloadErrorKind::Integrity) => "install_integrity",
        Some(DownloadErrorKind::Io) => "install_io",
        _ => "install_network",
    };
    let mut message = format!(
        "{} 个文件没下成（共 {} 个）",
        report.failed(),
        report.files.len()
    );
    for outcome in report.failures(3) {
        if let Some(failure) = &outcome.failure {
            message.push_str(&format!("；{}：{}", outcome.name, failure.message));
        }
    }
    CliError::user(code, message).with_hint(format!(
        "重跑 `%prog% install` 会续传并跳过已就绪文件；游戏根 {}",
        ctx.game().root().display()
    ))
}
