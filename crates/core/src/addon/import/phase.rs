//! 两个落盘阶段的实现：**覆盖内容解压** 与 **远端补全下载**（共用同一张三方合并表）。
//!
//! 拆出来是为了让 `pipeline.rs` 只剩"顺序与收尾"：阶段实现里看得见"每个文件怎么判"，
//! 编排里看得见"什么时候回滚"。

use std::path::Path;

use super::super::pack::{PackArchive, PackError};
use super::baseline::PackBaseline;
use super::cancel::Stopped;
use super::error::ImportError;
use super::fetch::{FetchError, FetchErrorKind, FetchRequest};
use super::merge;
use super::plan::{curse_class_dir, ImportPlan, PlanRemote};
use super::report::tell;
use super::report::MISSING_OFFLINE_DETAIL;
use super::report::{ImportReport, ImportStage, ImportedFile, MissingFile, MissingReason};
use super::stage::Staging;
use super::ImportRequest;
use crate::download::verify::{self, digest_bytes};
use crate::download::HashAlgo;

mod settle;

use settle::{conflict, settle};

/// 阶段一：覆盖条目落暂存 + 三方合并。
///
/// 被同包另一条声明遮蔽的条目（[`super::plan::PlanOverride::shadowed`]）**不读字节、不落地**：
/// 它的结论已经由 `plan::layer::resolve` 算成一条 `NOTE_OVERRIDE_SHADOWED` 注记
/// （进 `report.warnings`），而这条路径的行由真正的胜者给出——一路径一行，不再出现
/// "两条 Installed、前一条的哈希其实没落地"。
///
/// # 取消检查点
///
/// 解压阶段**每条一次**（循环开头）：只写临时目录，取消 = 临时目录整个删掉，目标目录一个
/// 字节都没动。被 `?` 冒泡出去的 [`Stopped::Cancelled`] 与失败**不是同一条路**。
pub(crate) async fn extract_overrides(
    request: &ImportRequest<'_>,
    archive: &PackArchive,
    plan: &ImportPlan,
    baseline: &PackBaseline,
    staging: &Staging,
    report: &mut ImportReport,
    next: &mut PackBaseline,
) -> Result<(), Stopped> {
    let live: Vec<&super::plan::PlanOverride> = plan
        .overrides
        .iter()
        .filter(|entry| entry.shadowed.is_none())
        .collect();
    let total = live.len();
    let target = staging.target().to_path_buf();
    for (index, entry) in live.iter().enumerate() {
        // 取消检查点（每条一次）：只写临时目录，停下就是"什么都没发生"。
        request.cancel.stopped()?;
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
///
/// # 取消检查点
///
/// 每条远端条目一次（循环开头）。**单条下载在飞不可中断**（[`super::fetch::PackFetcher`] 没有
/// 取消参数），所以取消在最坏情况下要等"当前这一条下载返回"；返回后下一次检查立刻生效，
/// 已下到临时目录的字节随临时目录一起删掉。
pub(crate) async fn fetch_remote(
    request: &ImportRequest<'_>,
    plan: &ImportPlan,
    baseline: &PackBaseline,
    staging: &Staging,
    report: &mut ImportReport,
    next: &mut PackBaseline,
) -> Result<(), Stopped> {
    let total = plan.remote.len();
    let target = staging.target().to_path_buf();
    for (index, entry) in plan.remote.iter().enumerate() {
        // 取消检查点（每条一次）：上一条下载返回之后立刻生效。
        request.cancel.stopped()?;
        let display = entry.path.clone().unwrap_or_else(|| entry.file.key());
        tell(
            &request.progress,
            ImportStage::Fetch,
            index,
            total,
            Some(display.clone()),
        );
        let resolved = match resolve(request, entry).await? {
            Resolution::Ready(resolved) => resolved,
            Resolution::Missing(reason, detail) => {
                report
                    .missing
                    .push(missing_from_error(&display, entry, reason, &detail));
                continue;
            }
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
            // 摘要在**引擎那一层**就没对上（收尾错误的叶子成因是 `Integrity`）：坏字节已被引擎
            // 丢掉、根本没交到这里，所以下面那道「落盘后复核」的闸看不到这种情形。判据与文案都
            // 照它抄（同一句「摘要与清单声明不符」），否则同一个事实会有两种说法。
            Err(error) if error.kind() == FetchErrorKind::Integrity => {
                return Err(declared_mismatch(&resolved.path, error.message()).into());
            }
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
                return Err(declared_mismatch(
                    &resolved.path,
                    format!("声明 {declared}，实际 {pack_sha1}"),
                )
                .into());
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

/// 补全一条的结果：要么「有路径、有地址」，要么「这条补不出来 + 具名原因」。
///
/// 为什么缺失也走返回值、不走 `Err`：一条补不出来**不是**导入失败（[`ImportError`] 那条路会
/// 回滚整包），它只是这一行进 `report.missing`。把原因一起带出来，报告里就能写清「缺哪一条、
/// 为什么缺」（比如「缺 CurseForge Key」与「作者关了 API 分发」是两种修法）。
enum Resolution {
    Ready(Resolved),
    Missing(MissingReason, String),
}

/// 把条目补全成"有路径、有地址"。
/// `Ok(Resolution::Missing(..))` = 这条补不出来（进 `missing`）；`Err` = 致命（本地 IO / 内容不符）。
async fn resolve(
    request: &ImportRequest<'_>,
    entry: &PlanRemote,
) -> Result<Resolution, ImportError> {
    let fetcher_present = request.fetcher.is_some();
    if let Some(path) = &entry.path {
        // mrpack：路径与地址都在清单里，取第一个非空地址。
        let Some(url) = entry
            .file
            .downloads
            .iter()
            .find(|url| !url.trim().is_empty())
            .cloned()
        else {
            return Ok(missing_for(fetcher_present));
        };
        return Ok(Resolution::Ready(Resolved {
            path: path.clone(),
            url,
            sha1: entry.file.sha1().map(str::to_string),
            sha512: entry.file.sha512().map(str::to_string),
            size: entry.file.size,
        }));
    }
    // CurseForge：只有 projectID/fileID，靠 CF 客户端补全（缺 Key 时是一条具名的「缺文件」）。
    let (Some(project_id), Some(file_id)) = (entry.file.project_id, entry.file.file_id) else {
        return Ok(missing_for(fetcher_present));
    };
    let Some(fetcher) = request.fetcher else {
        return Ok(missing_for(false));
    };
    let meta = match fetcher.curse_file(project_id, file_id).await {
        Ok(meta) => meta,
        // 缺 Key 与「这条真补不出来」都是**可容忍**的：只让这一条记缺，整包照装。
        // 原因取 core 那句原话（它会说清是「缺 Key」还是 401/429），缺 Key 归 NoSource。
        Err(error) if error.kind().is_tolerable() => {
            let reason = if error.kind().is_not_configured() {
                MissingReason::NoSource
            } else {
                MissingReason::FetchFailed
            };
            return Ok(Resolution::Missing(reason, error.message().to_string()));
        }
        Err(error) => return Err(error.into()),
    };
    let (Some(url), Some(dir)) = (
        meta.url.clone().filter(|url| !url.trim().is_empty()),
        meta.class_id.and_then(curse_class_dir),
    ) else {
        // 作者关了 API 分发 / classId 认不出 ⇒ **不猜**目录、不猜 URL。
        return Ok(missing_for(true));
    };
    Ok(Resolution::Ready(Resolved {
        path: format!("{dir}/{}", meta.file_name),
        url,
        // CF 官方只给 sha1，这一档没有可核的期望值。
        sha1: meta.sha1,
        sha512: None,
        size: meta.size,
    }))
}

/// 组装一条缺失结果（条目本身补不出来）。
fn missing_for(fetcher_present: bool) -> Resolution {
    let reason = if fetcher_present {
        MissingReason::NoSource
    } else {
        MissingReason::Offline
    };
    let detail = if fetcher_present {
        "清单里没有可用下载地址，或 CurseForge 元数据补不出来（不猜 URL）"
    } else {
        // 这句不自称「离线」：壳层的标签已经是「离线导入」（M325/P30）。
        MISSING_OFFLINE_DETAIL
    };
    Resolution::Missing(reason, detail.to_string())
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

/// 「内容与清单声明不符」的**唯一构造点**（M319 复审观察项）。
///
/// 导入链上有两道这样的闸：一道在引擎那一层就判掉（坏字节没交回来，只有引擎原话），一道是
/// 落盘后的复核（有声明与实际两个摘要）。两道的判据与文案必须一致，否则同一个事实会有两种
/// 说法——detail 由调用方给，句式在这里只写一份。
fn declared_mismatch(path: &str, detail: impl std::fmt::Display) -> ImportError {
    ImportError::integrity(format!("{path} 的摘要与清单声明不符（{detail}）"))
}

/// `pack_io` 便捷构造（本地 IO 复用 M110 的稳定码，不再造新码）。
fn pack_io(message: impl Into<String>) -> ImportError {
    PackError::io(message).into()
}
