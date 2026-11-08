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

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::json;
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

/// 造一个内核 job（源 = GitHub / `ULL_UPDATE_DIR`，暂存 = 数据根缓存，目标 = 本进程 exe）。
fn job(ctx: &Ctx, dest: Option<PathBuf>) -> Job {
    let engine = super::install::build_engine(ctx);
    let job = Job::for_process(default_source(Arc::new(engine)), &ctx.layout().cache());
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
    let hint = if error.is_retryable() {
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
            let mut human = vec![
                format!("已下载并校验 v{version}（{} 字节）", receipt.bytes),
                format!("已替换：{dest}"),
            ];
            human.push(match &backup {
                Some(backup) => format!("旧版本备份：{backup}"),
                None => "原本没有旧文件（首次落盘）".to_string(),
            });
            human.push("下次启动生效（正在运行的进程仍是旧版本）。".to_string());
            Ok(Outcome::new(Report::new(
                json!({
                    "current": current.to_string(),
                    "channel": CHANNEL.as_str(),
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
fn report_of(current: &Version, source: &str, outcome: &CheckOutcome) -> Report {
    let base = |status: &str| {
        json!({
            "current": current.to_string(),
            "channel": CHANNEL.as_str(),
            "source": source,
            "status": status,
        })
    };
    match outcome {
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
    }
}
