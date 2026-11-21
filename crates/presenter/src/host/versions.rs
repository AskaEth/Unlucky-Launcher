//! 版本清单（下载页）：远端 manifest（12h 缓存）+ 内嵌 unlisted 清单 → 分类好的行。
//!
//! 分类口径抄 analyse/03 §2.4：官方 `type` 决定 release/snapshot/远古；远古再按 id
//! 前缀拆 classic（`rd-*`）/ old_alpha（`a*`）/ old_beta（`b*`），其余归 unlisted 汇总。
//! 拉取失败但有旧缓存时用旧缓存（离线可用），全无才报错。

use std::collections::BTreeSet;

use unlucky_core::download::blocking::block_on;
use unlucky_core::download::cache::now_ms;
use unlucky_core::game::install::RemoteSource;
use unlucky_core::game::unlisted;
use unlucky_core::game::version::{ReleaseType, RemoteVersion};

use crate::dto::{AncientKind, VersionKind, VersionSummary};

use super::env::HostEnv;
use super::job::{spawn, HostTask, JobSender};

/// 版本 id / 官方类型 → 下载页的四种归类。
pub fn kind_of(id: &str, release_type: ReleaseType) -> VersionKind {
    match release_type {
        ReleaseType::Release => VersionKind::Release,
        ReleaseType::Snapshot | ReleaseType::Pending | ReleaseType::Unobfuscated => {
            VersionKind::Snapshot
        }
        ReleaseType::OldAlpha | ReleaseType::OldBeta | ReleaseType::Modified => {
            VersionKind::Ancient(ancient_kind(id))
        }
        ReleaseType::Unknown => {
            if id.starts_with("rd-") || id.starts_with("a") || id.starts_with("b") {
                VersionKind::Ancient(ancient_kind(id))
            } else {
                VersionKind::Release
            }
        }
    }
}

/// 远古版子类（`rd-*` → classic；`a1.*` → old_alpha；`b1.*` → old_beta；其余 unlisted）。
pub fn ancient_kind(id: &str) -> AncientKind {
    if id.starts_with("rd-") || id.starts_with("inf-") {
        AncientKind::Classic
    } else if id.starts_with("a1.") || id.starts_with("a0.") || id.starts_with('a') {
        AncientKind::OldAlpha
    } else if id.starts_with("b1.") || id.starts_with('b') {
        AncientKind::OldBeta
    } else {
        AncientKind::Unlisted
    }
}

/// 发布日期取 `releaseTime` 的日期部分（缺失时退回 `time`，再退回空串）。
fn released_of(version: &RemoteVersion) -> String {
    version
        .release_time
        .as_deref()
        .or(version.time.as_deref())
        .map(|text| text.get(..10).unwrap_or(text).to_string())
        .unwrap_or_default()
}

/// 拉清单并翻译成 DTO（安装状态按盘上版本判定）。
fn fetch(env: &HostEnv) -> Result<Vec<VersionSummary>, String> {
    let engine = super::engine::engine(env);
    let meta_cache = env.layout.meta_cache();
    let now = now_ms();
    let (mut list, _freshness) = block_on(async {
        RemoteSource::new(&engine, &env.game, &meta_cache, now)
            .manifest(false)
            .await
    })
    .map_err(|error| format!("拉取版本清单失败：{error}"))?;
    // 内嵌 unlisted 清单先合（同 id 先插入者胜），保证远古版显式可达（09 §G2）。
    unlisted::merge_into(&mut list);
    list.sort_newest_first();

    let installed: BTreeSet<String> = env
        .game
        .installed_versions()
        .unwrap_or_default()
        .into_iter()
        .collect();
    Ok(list
        .versions
        .iter()
        .map(|version| VersionSummary {
            id: version.id.clone(),
            kind: kind_of(&version.id, version.version_type),
            released: released_of(version),
            installed: installed.contains(&version.id),
        })
        .collect())
}

/// 后台拉清单（网络在后台线程里，UI 不卡）。
pub fn spawn_list(env: HostEnv, tx: JobSender) {
    spawn("versions", move || {
        let result = fetch(&env);
        let _ = tx.send(HostTask::Versions(result));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_follow_official_types() {
        assert_eq!(
            kind_of("1.21.9", ReleaseType::Release),
            VersionKind::Release
        );
        assert_eq!(
            kind_of("1.21.9-pre3", ReleaseType::Snapshot),
            VersionKind::Snapshot
        );
        assert_eq!(
            kind_of("rd-132211", ReleaseType::OldAlpha),
            VersionKind::Ancient(AncientKind::Classic)
        );
        assert_eq!(
            kind_of("a1.1.1", ReleaseType::OldAlpha),
            VersionKind::Ancient(AncientKind::OldAlpha)
        );
        assert_eq!(
            kind_of("b1.7.3", ReleaseType::OldBeta),
            VersionKind::Ancient(AncientKind::OldBeta)
        );
        assert_eq!(
            kind_of("x-modified", ReleaseType::Modified),
            VersionKind::Ancient(AncientKind::Unlisted)
        );
    }

    #[test]
    fn released_uses_date_part() {
        let version = RemoteVersion {
            id: "1.20.4".to_string(),
            version_type: ReleaseType::Release,
            url: String::new(),
            time: None,
            release_time: Some("2023-12-07T12:00:00+00:00".to_string()),
            sha1: None,
            size: None,
        };
        assert_eq!(released_of(&version), "2023-12-07");
        let empty = RemoteVersion {
            id: "x".to_string(),
            version_type: ReleaseType::Unknown,
            url: String::new(),
            time: None,
            release_time: None,
            sha1: None,
            size: None,
        };
        assert_eq!(released_of(&empty), "");
    }

    #[test]
    fn unlisted_merge_keeps_ancient_versions_reachable() {
        // core 的内嵌清单里一定有远古版（`rd-*` / `a1.*` / `b1.*`）
        let mut list = unlucky_core::game::version::VersionList::default();
        let added = unlisted::merge_into(&mut list);
        assert!(added > 0, "内嵌 unlisted 清单不该为空");
        assert!(
            list.versions.iter().any(|v| v.id.starts_with("a1.")
                || v.id.starts_with("b1.")
                || v.id.starts_with("rd-")),
            "远古版必须显式可达"
        );
    }
}
