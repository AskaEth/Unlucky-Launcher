//! `ull update check` / `ull update apply`（M159）：**复用 core 的同一内核**。
//!
//! 两条命令都只做「解析参数 → 造一个 [`Job`] → 调 `check` / `apply` → 排版输出」，
//! 版本比较、渠道准入、清单校验、长度/SHA256 双检、原子替换全在 `core::update`
//! （GUI 走的是同一个 `Job`，见 `crates/presenter/src/host/update.rs`）。
//!
//! 三条不许折叠的边界：
//!
//! | 情形 | 输出 |
//! |---|---|
//! | 已是最新 | `status=up_to_date`（退出码 0） |
//! | 有可装的更新 | `status=available`（退出码 0；`apply` 才动文件） |
//! | 检查失败 | **错误**（退出码 1，`code=update_check_failed`）——绝不说成「已是最新」 |
//!
//! 还有一条单独报的：`missing_artifact`（有更高版本但里面没有本平台的产物）。那是发布侧的漏项，
//! 同样不许说成「已是最新」。
//!
//! `apply` 默认替换**本进程的 exe**（`Job::for_process`）；`--dest` 可以钉到别的路径（取证用）。
//!
//! # 测试通道（M169）
//!
//! 想在没有稳定 Release 的情况下整环验一遍，得用**专门编出来的验收二进制**：
//!
//! ```text
//! cargo build --release --features update-test-source     # 官方发布构建不带这个 feature
//! ULL_UPDATE_DIR=<root> ULL_UPDATE_TEST_SOURCE=1 ./ull update check
//! ```
//!
//! 三档全给才生效（feature 默认关 = 官方二进制里没有这条路径；带 feature 时还要两档环境变量，
//! 判据见 `core::update::job`）。这时 `--json` 的 `source` 是 `file:<root>`、`test_source` 是
//! `true`，人读输出另有一行点名。它不是第二个更新源：没有设置键、也没有命令行参数。
//!
//! ⚠ **验收时必须给 `--dest`**：不给就是替换本进程的 exe。自动化用例里的「本进程」是测试二进制，
//! 去掉 `--dest` 会把它真改名换掉（`crates/cli/tests/update_e2e_test.rs` 的注释写了这件事）。

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::json;
use unlucky_core::download::DownloadEngine;
use unlucky_core::update::{default_source, Applied, CheckOutcome, Job, UpdateError, Version};

use crate::cli::{UpdateApplyArgs, UpdateCheckArgs};
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::{Outcome, Report};

/// D50：先只发 stable。
const CHANNEL: unlucky_core::update::Channel = unlucky_core::update::Channel::Stable;

/// 本进程版本（与 `ull --version` 同源）。
fn current_version() -> Result<Version, CliError> {
    let text = env!("CARGO_PKG_VERSION");
    Version::parse(text)
        .map_err(|error| CliError::internal("bad_version", format!("编译期版本号不合法：{error}")))
}

/// 自更新的下载引擎：与 `install` / `java` / `pack` 同一条装配（参数来自
/// [`crate::download`]，设置读数与落点在那边）。
///
/// 自更新只下**一个** artifact（`update::github` 的 `fetch_artifact` → 单次 `engine.download`），
/// 所以并发的差别在这条路径上不可观测；真正随设置变的是**每源重试次数**与**gh-proxy 前缀**——
/// 那正是「设置说真话」的两维。单独留这个接缝是为了让用例能逐命令打点（变异时只有它红）。
pub(crate) fn engine(ctx: &Ctx) -> Arc<DownloadEngine> {
    super::install::build_engine(ctx)
}

/// 造一个内核 job（源 = GitHub / `ULL_UPDATE_DIR`，暂存 = 数据根缓存，目标 = 本进程 exe）。
fn job(ctx: &Ctx, dest: Option<PathBuf>) -> Job {
    let engine = engine(ctx);
    let job = Job::for_process(default_source(engine), &ctx.layout().cache());
    match dest {
        Some(dest) => job.with_dest(dest),
        None => job,
    }
}

/// 失败 → CLI 错误（`code` 说「哪条命令的哪一类失败」，`detail.kind` 说**到底为什么**）。
///
/// `--json` 信封示例（复审 P3-3 之后的样子）：
///
/// ```json
/// {"ok":false,"error":{"code":"update_apply_failed","kind":"user","message":"应用更新失败（digest_mismatch）：…",
///  "hint":"…","detail":{"kind":"digest_mismatch","retryable":true}}}
/// ```
///
/// 为什么要两层：`code` 是命令级的（脚本据此决定「要不要提示用户重跑」），`detail.kind` 是
/// **底层原因**（`digest_mismatch` / `network` / `invalid_manifest` / `io` …，据此决定「重试有没有意义」）。
/// 以前原因只写在人读 message 里，脚本不解析散文就分不出来。
fn failure(code: &'static str, action: &str, error: &UpdateError) -> CliError {
    // 签名类错误单独给一句：它们**不可重试**，而且绝不能靠「换个源/跳过校验」绕过——绕过一次
    // 就等于把信任根拆了（见 `core::update::signature` 的文件头）。
    let hint = if error.kind.is_signature() {
        concat!(
            "清单签名没通过，重试没有意义：signature_missing = 发布侧漏了签名；",
            "signature_unknown_key / keyring_empty = 本构建的信任表里没有这把钥（升级启动器或等发布侧换钥）；",
            "signature_invalid = 签名之后清单被改过。别绕过校验，等发布侧修"
        )
    } else if error.is_retryable() {
        "网络/占用类失败，过一会儿重试一次通常就好"
    } else {
        "这不是瞬时失败：清单或版本号本身有问题，等发布侧修"
    };
    CliError::user(
        code,
        format!("{action}失败（{}）：{error}", error.kind.as_str()),
    )
    .with_detail(json!({
        "kind": error.kind.as_str(),
        "retryable": error.is_retryable(),
    }))
    .with_hint(hint)
}

/// 这个源是不是测试通道（判据派生自**实际用了哪个源**，不是回读环境变量：
/// `DirSource` 的 id 是 `file:<root>`，GitHub 源是 `github:<owner>/<repo>`）。
fn is_test_source(source_id: &str) -> bool {
    source_id.starts_with("file:")
}

/// 测试通道的点名行（只在真走了本地目录源时出现）。文案里点明**这是验收构建**：官方发布二进制
/// 没有这条路径（`update-test-source` 默认关），所以出现这一行就说明手上这份不是官方产物。
fn test_channel_note(source: &str) -> String {
    format!(
        "测试通道生效：源 = {source}（本地目录，不是 GitHub Releases；本二进制带 \
         `update-test-source` feature 编出来，官方发布构建不含这条路径）"
    )
}

/// `ull update check`：只查、不动任何文件。
///
/// # Errors
///
/// 清单取不到 / 清单不合法 / 渠道不符 → 用户错误（退出码 1）。
pub fn check(ctx: &Ctx, _args: &UpdateCheckArgs) -> Result<Outcome, CliError> {
    let current = current_version()?;
    let job = job(ctx, None);
    let outcome = job
        .check(&current, CHANNEL)
        .map_err(|error| failure("update_check_failed", "检查更新", &error))?;
    Ok(Outcome::new(report_of(&current, job.source_id(), &outcome)))
}

/// `ull update apply`：复查 → 下载 → 双检 → 替换（失败自动清理/回滚）。
///
/// # Errors
///
/// 复查、取包、校验、替换任一步失败 → 用户错误（退出码 1）。
pub fn apply(ctx: &Ctx, args: &UpdateApplyArgs) -> Result<Outcome, CliError> {
    let current = current_version()?;
    let job = job(ctx, args.dest.clone());
    let dest = job.dest().display().to_string();
    match job
        .apply(&current, CHANNEL)
        .map_err(|error| failure("update_apply_failed", "应用更新", &error))?
    {
        Applied::Installed { receipt, version } => {
            let backup = receipt
                .backup
                .as_ref()
                .map(|path| path.display().to_string());
            let source = job.source_id();
            let test_source = is_test_source(source);
            let mut human = vec![
                format!("已下载并校验 v{version}（{} 字节）", receipt.bytes),
                format!("已替换：{dest}"),
            ];
            human.push(match &backup {
                Some(backup) => format!("旧版本备份：{backup}"),
                None => "原本没有旧文件（首次落盘）".to_string(),
            });
            human.push("下次启动生效（正在运行的进程仍是旧版本）。".to_string());
            if test_source {
                human.push(test_channel_note(source));
            }
            Ok(Outcome::new(Report::new(
                json!({
                    "current": current.to_string(),
                    "channel": CHANNEL.as_str(),
                    "source": source,
                    "test_source": test_source,
                    "status": "installed",
                    "version": version,
                    "path": dest,
                    "bytes": receipt.bytes,
                    "backup": backup,
                }),
                human,
            )))
        }
        Applied::Nothing(outcome) => {
            let report = report_of(&current, job.source_id(), &outcome);
            Ok(Outcome::new(Report::new(
                json!({ "status": "nothing_to_apply", "check": report.data }),
                {
                    let mut human =
                        vec!["复查之后没有可应用的更新（已经不是最新版本冲突）".to_string()];
                    human.extend(report.human);
                    human
                },
            )))
        }
    }
}

/// 把检查结论排成「人读几行 + `--json` 一份」。
///
/// `test_source` 逐条写进 JSON（不只在高亮时写）：脚本据此判断「这一份结论是不是本地夹具给的」，
/// 而判据来自 `source`（实际用了哪个源），所以不会出现「环境变量说关了、输出说开着」。
fn report_of(current: &Version, source: &str, outcome: &CheckOutcome) -> Report {
    let test_source = is_test_source(source);
    let base = |status: &str| {
        json!({
            "current": current.to_string(),
            "channel": CHANNEL.as_str(),
            "source": source,
            "test_source": test_source,
            "status": status,
        })
    };
    let mut report = match outcome {
        CheckOutcome::UpToDate { latest, .. } => {
            let latest = latest.as_ref().map(ToString::to_string);
            let mut data = base("up_to_date");
            data["latest"] = json!(latest);
            Report::new(
                data,
                vec![match &latest {
                    Some(latest) => format!("已是最新：v{current}（渠道 {latest} 内最高）"),
                    None => format!("已是最新：v{current}（该渠道还没有任何发布）"),
                }],
            )
        }
        CheckOutcome::Available(candidate) => {
            let version = candidate.release.version.to_string();
            let forced = candidate.urgency == unlucky_core::update::Urgency::Forced;
            let notes = candidate.release.notes.clone().unwrap_or_default();
            let mut human = vec![format!(
                "发现新版本 v{version}（当前 v{current}，{} 字节{}）",
                candidate.artifact.size,
                if forced { "，强制更新" } else { "" }
            )];
            if !notes.trim().is_empty() {
                human.push(format!("说明：{}", notes.trim()));
            }
            human.push("用 `%prog% update apply` 下载并应用（下次启动生效）".to_string());
            Report::new(
                json!({
                    "current": current.to_string(),
                    "channel": CHANNEL.as_str(),
                    "source": source,
                    "test_source": test_source,
                    "status": "available",
                    "version": version,
                    "size": candidate.artifact.size,
                    "forced": forced,
                    "notes": notes,
                    "target": candidate.artifact.target.as_str(),
                }),
                human,
            )
        }
        CheckOutcome::MissingArtifact { release } => {
            let version = release.version.to_string();
            Report::new(
                json!({
                    "current": current.to_string(),
                    "channel": CHANNEL.as_str(),
                    "source": source,
                    "test_source": test_source,
                    "status": "missing_artifact",
                    "version": version,
                    "target": unlucky_core::update::Target::current().as_str(),
                }),
                vec![format!(
                    "v{version} 已发布，但没有为本平台（{}）提供产物——这是发布侧漏项，不是「已是最新」",
                    unlucky_core::update::Target::current().as_str()
                )],
            )
        }
    };
    if test_source {
        report.human.push(test_channel_note(source));
    }
    report
}
