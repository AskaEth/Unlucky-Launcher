//! 两个落盘阶段的实现：**覆盖内容解压** 与 **远端补全下载**（共用同一张三方合并表）。
//!
//! 拆出来是为了让 `pipeline.rs` 只剩"顺序与收尾"：阶段实现里看得见"每个文件怎么判"，
//! 编排里看得见"什么时候回滚"。

use std::path::Path;

use super::super::pack::{PackArchive, PackError};
use super::baseline::PackBaseline;
use super::error::ImportError;
use super::fetch::{FetchError, FetchRequest};
use super::merge::{self, MergeAction};
use super::pipeline::tell;
use super::plan::{curse_class_dir, ImportPlan, PlanRemote};
use super::report::{
    ConflictChoice, ConflictFile, ConflictReason, FileAction, ImportReport, ImportStage,
    ImportedFile, MissingFile, MissingReason,
};
use super::stage::Staging;
use super::ImportRequest;
use crate::download::verify::{self, digest_bytes};
use crate::download::HashAlgo;

/// 阶段一：覆盖条目落暂存 + 三方合并。
///
/// 被同包另一条声明遮蔽的条目（[`super::plan::PlanOverride::shadowed`]）**不读字节、不落地**：
/// 它的结论已经由 `plan::layer::resolve` 算成一条 `NOTE_OVERRIDE_SHADOWED` 注记
/// （进 `report.warnings`），而这条路径的行由真正的胜者给出——一路径一行，不再出现
/// "两条 Installed、前一条的哈希其实没落地"。
pub(crate) async fn extract_overrides(
    request: &ImportRequest<'_>,
    archive: &PackArchive,
    plan: &ImportPlan,
    baseline: &PackBaseline,
    staging: &Staging,
    report: &mut ImportReport,
    next: &mut PackBaseline,
) -> Result<(), ImportError> {
    let live: Vec<&super::plan::PlanOverride> = plan
        .overrides
        .iter()
        .filter(|entry| entry.shadowed.is_none())
        .collect();
    let total = live.len();
    let target = staging.target().to_path_buf();
    for (index, entry) in live.iter().enumerate() {
        tell(
            &request.progress,
            ImportStage::Extract,
            index,
            total,
            Some(entry.path.clone()),
        );
        let name = format!("{}/{}", entry.dir, entry.path);
        let bytes = archive.read(&name)?.ok_or_else(|| {
            ImportError::from(PackError::metadata(format!(
                "清单列了 {name}，但包里读不到这个条目（声明与实际不符）"
            )))
        })?;
        // 用**实测**摘要而不是模型里记的那个：模型可能来自外部构造。
        let pack_sha1 = digest_bytes(HashAlgo::Sha1, &bytes);
        next.insert(entry.path.clone(), pack_sha1.clone());
        let disk = disk_sha1(&target.join(&entry.path))?;
        let decision = merge::decide(&pack_sha1, baseline.sha1(&entry.path), disk.as_deref());
        // 逐项决策优先，其次整批档（缺省是"保留"）：见 `decision::ConflictDecisions`。
        let picked = request.decisions.choice_for(&entry.path);
        let (action, choice) = settle(decision, disk.is_some(), picked);
        if let Some(reason) = decision.reason {
            report.conflicts.push(conflict(
                entry.path.clone(),
                reason,
                choice,
                &pack_sha1,
                bytes.len() as u64,
                baseline.sha1(&entry.path).map(str::to_string),
                disk,
            ));
        }
        if action.wrote() {
            staging.write(&entry.path, &bytes)?;
            report.bytes_written += bytes.len() as u64;
        }
        report.files.push(ImportedFile {
            path: entry.path.clone(),
            source: entry.source,
            sha1: Some(pack_sha1),
            bytes: bytes.len() as u64,
            disabled: entry.disabled,
            action,
            note: entry.note.clone(),
        });
    }
    Ok(())
}

/// 阶段二：远端条目补全 + 下载 + 同一张合并表。
pub(crate) async fn fetch_remote(
    request: &ImportRequest<'_>,
    plan: &ImportPlan,
    baseline: &PackBaseline,
    staging: &Staging,
    report: &mut ImportReport,
    next: &mut PackBaseline,
) -> Result<(), ImportError> {
    let total = plan.remote.len();
    let target = staging.target().to_path_buf();
    for (index, entry) in plan.remote.iter().enumerate() {
        let display = entry.path.clone().unwrap_or_else(|| entry.file.key());
        tell(
            &request.progress,
            ImportStage::Fetch,
            index,
            total,
            Some(display),
        );
        let Some(resolved) = resolve(request, entry).await? else {
            report
                .missing
                .push(missing_for(entry, request.fetcher.is_some()));
            continue;
        };
        let disk = disk_sha1(&target.join(&resolved.path))?;
        // 有声明摘要就能先判合并：命中"保留用户 / 已一致"时**不必下载**（省一次网络）。
        if let Some(declared) = resolved.sha1.clone() {
            let decision = merge::decide(&declared, baseline.sha1(&resolved.path), disk.as_deref());
            let picked = request.decisions.choice_for(&resolved.path);
            let (action, choice) = settle(decision, disk.is_some(), picked);
            if !action.wrote() {
                next.insert(resolved.path.clone(), declared.clone());
                if let Some(reason) = decision.reason {
                    report.conflicts.push(conflict(
                        resolved.path.clone(),
                        reason,
                        choice,
                        &declared,
                        resolved.size.unwrap_or(0),
                        baseline.sha1(&resolved.path).map(str::to_string),
                        disk.clone(),
                    ));
                }
                report.files.push(ImportedFile {
                    path: resolved.path,
                    source: entry.source,
                    sha1: Some(declared),
                    bytes: resolved.size.unwrap_or(0),
                    disabled: entry.disabled,
                    action,
                    note: None,
                });
                continue;
            }
        }
        let dest = staging.dest_for(&resolved.path)?;
        let fetched = match request.fetcher {
            Some(fetcher) => {
                fetcher
                    .fetch_file(FetchRequest {
                        url: &resolved.url,
                        dest: &dest,
                        sha1: resolved.sha1.as_deref(),
                        sha512: resolved.sha512.as_deref(),
                        size: resolved.size,
                    })
                    .await
            }
            None => Err(FetchError::unsupported("本次导入没有 fetcher（离线）")),
        };
        let fetched = match fetched {
            Ok(fetched) => fetched,
            Err(error) if error.kind().is_tolerable() => {
                report.missing.push(missing_from_error(
                    &resolved.path,
                    entry,
                    MissingReason::FetchFailed,
                    error.message(),
                ));
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        let pack_sha1 = digest_file(&dest)?;
        if let Some(declared) = resolved.sha1.as_deref() {
            if !verify::matches(declared, &pack_sha1) {
                // 摘要不符 = 内容坏了 / 被换过，**不许**当成"暂时搞不到"混过去。
                return Err(ImportError::integrity(format!(
                    "{} 的摘要与清单声明不符（声明 {declared}，实际 {pack_sha1}）",
                    resolved.path
                )));
            }
        }
        let decision = merge::decide(&pack_sha1, baseline.sha1(&resolved.path), disk.as_deref());
        let picked = request.decisions.choice_for(&resolved.path);
        let (action, choice) = settle(decision, disk.is_some(), picked);
        if let Some(reason) = decision.reason {
            report.conflicts.push(conflict(
                resolved.path.clone(),
                reason,
                choice,
                &pack_sha1,
                fetched.bytes,
                baseline.sha1(&resolved.path).map(str::to_string),
                disk,
            ));
        }
        if action.wrote() {
            report.bytes_written += fetched.bytes;
        } else {
            staging.discard(&resolved.path)?;
        }
        next.insert(resolved.path.clone(), pack_sha1.clone());
        report.files.push(ImportedFile {
            path: resolved.path,
            source: entry.source,
            sha1: Some(pack_sha1),
            bytes: fetched.bytes,
            disabled: entry.disabled,
            action,
            note: None,
        });
    }
    Ok(())
}

/// 一个已解析出"路径 + 地址"的远端条目。
struct Resolved {
    path: String,
    url: String,
    sha1: Option<String>, // 两档摘要各自可能缺（CF 只有 sha1、mrpack 可能只有 sha512）
    sha512: Option<String>,
    size: Option<u64>,
}

/// 把条目补全成"有路径、有地址"。
/// `Ok(None)` = 这条补不出来（进 `missing`）；`Err` = 致命（本地 IO / 内容不符）。
async fn resolve(
    request: &ImportRequest<'_>,
    entry: &PlanRemote,
) -> Result<Option<Resolved>, ImportError> {
    if let Some(path) = &entry.path {
        // mrpack：路径与地址都在清单里，取第一个非空地址。
        let Some(url) = entry
            .file
            .downloads
            .iter()
            .find(|url| !url.trim().is_empty())
            .cloned()
        else {
            return Ok(None);
        };
        return Ok(Some(Resolved {
            path: path.clone(),
            url,
            sha1: entry.file.sha1().map(str::to_string),
            sha512: entry.file.sha512().map(str::to_string),
            size: entry.file.size,
        }));
    }
    // CurseForge：只有 projectID/fileID，必须靠 CF API 补全（当前默认不支持 ⇒ 如实记缺）。
    let (Some(project_id), Some(file_id)) = (entry.file.project_id, entry.file.file_id) else {
        return Ok(None);
    };
    let Some(fetcher) = request.fetcher else {
        return Ok(None);
    };
    let meta = match fetcher.curse_file(project_id, file_id).await {
        Ok(meta) => meta,
        Err(error) if error.kind().is_tolerable() => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let (Some(url), Some(dir)) = (
        meta.url.clone().filter(|url| !url.trim().is_empty()),
        meta.class_id.and_then(curse_class_dir),
    ) else {
        // 作者关了 API 分发 / classId 认不出 ⇒ **不猜**目录、不猜 URL。
        return Ok(None);
    };
    Ok(Some(Resolved {
        path: format!("{dir}/{}", meta.file_name),
        url,
        // CF 官方只给 sha1，这一档没有可核的期望值。
        sha1: meta.sha1,
        sha512: None,
        size: meta.size,
    }))
}

/// 合并判定 + 决策 → 实际动作。
///
/// `picked` 是这一次的处置（逐项决策或整批档，见 [`super::decision::ConflictDecisions`]）：
/// 它**只对冲突生效**——`Install` / `SkipIdentical` 不受它影响，所以"用户点错一条保留"不会漏装。
fn settle(
    decision: merge::MergeDecision,
    disk_exists: bool,
    picked: ConflictChoice,
) -> (FileAction, ConflictChoice) {
    let overwrite = picked == ConflictChoice::Overwrite;
    match decision.action {
        MergeAction::Install => (
            if disk_exists {
                FileAction::Replaced
            } else {
                FileAction::Installed
            },
            ConflictChoice::Overwrite,
        ),
        MergeAction::SkipIdentical => (FileAction::SkippedIdentical, ConflictChoice::KeepUser),
        MergeAction::KeepUser if overwrite => (
            if disk_exists {
                FileAction::Replaced
            } else {
                FileAction::Installed
            },
            ConflictChoice::Overwrite,
        ),
        MergeAction::KeepUser => (FileAction::KeptUser, ConflictChoice::KeepUser),
    }
}

/// 组装一条冲突记录。
fn conflict(
    path: String,
    reason: ConflictReason,
    choice: ConflictChoice,
    pack_sha1: &str,
    pack_size: u64,
    base_sha1: Option<String>,
    disk_sha1: Option<String>,
) -> ConflictFile {
    ConflictFile {
        path,
        reason,
        choice,
        message_key: reason.message_key(),
        pack_sha1: pack_sha1.to_string(),
        pack_size,
        base_sha1,
        disk_sha1,
    }
}

/// 组装一条缺失记录（条目本身补不出来）。
fn missing_for(entry: &PlanRemote, fetcher_present: bool) -> MissingFile {
    let reason = if fetcher_present {
        MissingReason::NoSource
    } else {
        MissingReason::Offline
    };
    let detail = if fetcher_present {
        "清单里没有可用下载地址，或 CurseForge 元数据补不出来（不猜 URL）"
    } else {
        "本次导入没有接 fetcher（离线），远端文件一律跳过"
    };
    missing_from_error(
        &entry.path.clone().unwrap_or_else(|| entry.file.key()),
        entry,
        reason,
        detail,
    )
}

/// 组装一条缺失记录（已知原因与细节）。
fn missing_from_error(
    display: &str,
    entry: &PlanRemote,
    reason: MissingReason,
    detail: &str,
) -> MissingFile {
    MissingFile {
        path: entry.path.clone(),
        display: display.to_string(),
        project_id: entry.file.project_id,
        file_id: entry.file.file_id,
        reason,
        message_key: reason.message_key(),
        detail: detail.to_string(),
    }
}

/// 磁盘现状的 SHA-1（不存在 ⇒ `None`）。
fn disk_sha1(path: &Path) -> Result<Option<String>, ImportError> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => verify::digest_file(path, HashAlgo::Sha1)
            .map(Some)
            .map_err(|error| pack_io(format!("算 {} 的摘要失败：{error}", path.display()))),
        Ok(_) => Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(pack_io(format!("看 {} 失败：{error}", path.display()))),
    }
}

/// 文件的 SHA-1。
fn digest_file(path: &Path) -> Result<String, ImportError> {
    verify::digest_file(path, HashAlgo::Sha1)
        .map_err(|error| pack_io(format!("算 {} 的摘要失败：{error}", path.display())))
}

/// `pack_io` 便捷构造（本地 IO 复用 M110 的稳定码，不再造新码）。
fn pack_io(message: impl Into<String>) -> ImportError {
    PackError::io(message).into()
}
