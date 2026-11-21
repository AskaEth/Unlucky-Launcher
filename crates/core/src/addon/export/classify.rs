//! 逐文件判定（导出管线的第 ③ 步）：这份本地文件在目标格式里能不能被远程替代。
//!
//! 判定要**两个条件同时成立**才算远程引用：
//!
//! 1. 某个目录（[`RemoteCatalog`]）能**证明**它的身份——Modrinth 按 SHA-1、CurseForge 按指纹；
//! 2. 目标格式**表达得了**这个证明——mrpack 要 `path` + `sha512` + `downloads`，
//!    CF 清单要 `projectID` + `fileID`。
//!
//! 只满足第一条时就是"证明了但用不上"，它有自己的原因（`CurseOnlyMatch` /
//! `ModrinthOnlyMatch`），不能和"两个目录都不认识"混成一句话。
//!
//! 分成单独一个文件是为了让 [`super::pipeline`] 只讲流程；这里全是判据，可以逐条对着测
//! （`classify_with` 甚至允许测试直接喂指纹——哨兵值 `811513880` 用真实文件测不出来）。

use super::catalog::{RemoteCatalog, RemoteHit};
use super::report::{ExportFile, FileReason};
use super::scan::LocalFile;
use crate::addon::pack::curseforge::fingerprint;
use crate::addon::pack::manifest::{EnvSupport, PackFile};

/// 判定一条本地文件属于哪一类（外加命中，供组 `files[]` 用）。
pub(super) fn classify(
    local: &LocalFile,
    catalog: &dyn RemoteCatalog,
    target_is_curse: bool,
) -> (FileReason, Option<RemoteHit>) {
    // 指纹要读两遍盘；读不出来就按"没有 CF 侧证据"处理（文件本身仍会内联进包）。
    let value = fingerprint::fingerprint_file(&local.disk).ok();
    classify_with(local, catalog, target_is_curse, value)
}

/// 同 [`classify`]，但指纹由调用方给（`None` = 读不出来）。
///
/// `pub(crate)` 只有一个理由：哨兵值 `811513880` 那条分支**没法用真实文件测**——它是误命中，
/// 找不到一个能算出它的样本（要 2^32 次穷举）。测试直接喂这个数，验证它被当成"查不到"。
pub(crate) fn classify_with(
    local: &LocalFile,
    catalog: &dyn RemoteCatalog,
    target_is_curse: bool,
    fingerprint: Option<u32>,
) -> (FileReason, Option<RemoteHit>) {
    let modrinth = catalog.modrinth_by_sha1(&local.sha1);
    let curseforge = match fingerprint {
        Some(value) if !fingerprint::is_sentinel(value) => catalog.curseforge_by_fingerprint(value),
        Some(_) | None => None,
    };
    if target_is_curse {
        if local.disabled {
            // CF 没有禁用表达：`required:false` 是"可选"不是"禁用"，硬写就等于把它变成启用。
            return (FileReason::DisabledNotExpressible, None);
        }
        return match curseforge {
            Some(hit) if hit.curse_ids().is_some() => {
                (FileReason::CurseForgeFingerprint, Some(hit))
            }
            Some(_) => (FileReason::MissingCurseIds, None),
            None if modrinth.is_some() => (FileReason::ModrinthOnlyMatch, None),
            None => (FileReason::NoRemoteMatch, None),
        };
    }
    match modrinth {
        Some(hit) if hit.sha512().is_some() && hit.has_download() => {
            let mut merged = hit;
            // 两条都命中：两条直链都写进 downloads[]（HMCL 同款，兼容面更大）。
            if let Some(curse) = curseforge {
                for url in curse.downloads {
                    if !url.trim().is_empty() && !merged.downloads.iter().any(|seen| seen == &url) {
                        merged.downloads.push(url);
                    }
                }
                merged.project_id = merged.project_id.or(curse.project_id);
                merged.file_id = merged.file_id.or(curse.file_id);
            }
            (FileReason::ModrinthSha1, Some(merged))
        }
        Some(_) => (FileReason::MissingSha512, None),
        None if curseforge.is_some() => (FileReason::CurseOnlyMatch, None),
        None => (FileReason::NoRemoteMatch, None),
    }
}

/// 远程条目：`path` 用实例内路径（禁用文件已带 `.disabled` 后缀），`size` 用本地实测字节数。
pub(super) fn remote_entry(local: &LocalFile, hit: &RemoteHit) -> PackFile {
    let mut hashes = hit.hashes.clone();
    hashes.insert("sha1".to_string(), local.sha1.clone());
    PackFile {
        path: Some(local.path.clone()),
        project_id: hit.project_id,
        file_id: hit.file_id,
        hashes,
        downloads: hit.downloads.clone(),
        size: Some(local.bytes),
        disabled: local.disabled,
        client: EnvSupport::Required,
        server: EnvSupport::Required,
        ..PackFile::default()
    }
}

/// 报告里的一条。
pub(super) fn entry(local: &LocalFile, reason: FileReason, hit: Option<&RemoteHit>) -> ExportFile {
    ExportFile {
        path: local.path.clone(),
        class: reason.class(),
        reason,
        bytes: local.bytes,
        downloads: hit.map(|hit| hit.downloads.clone()).unwrap_or_default(),
        project_id: hit.and_then(|hit| hit.project_id),
        file_id: hit.and_then(|hit| hit.file_id),
        disabled: local.disabled,
    }
}
