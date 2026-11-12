//! 远端元数据：版本清单（带 TTL 缓存与过期回退）、版本 JSON 链、资源索引。
//!
//! 依据：`DevDocs/analyse/03-version-download.md` §2.6（清单缓存）、§4.3（继承解析）、
//! §6.2（版本 JSON）、§6.5（资源索引）；`DevDocs/PlanDoc/05-download-and-mirrors.md` §2
//! （清单类资源"短 TTL + 失败立切源"）。
//!
//! 三条与 HMCL 有意的差异（都是 analyse/03 §8 点名的上游坑）：
//! 1. **清单与版本 JSON 一律校验 sha1**（v2 端点自带），镜像篡改/截断在下载阶段就暴露；
//! 2. **清单缓存过期自动刷新**，且刷新失败时**回退到过期缓存**并标注
//!    [`ManifestFreshness::StaleFallback`]——离线机器上也能装已缓存的清单；
//! 3. 版本 JSON 链**远端优先、本地兜底**：装已存在的 loader 实例时不会因为"官方清单里没有
//!    这个魔改 id"就直接失败。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::super::error::GameError;
use super::super::inherit;
use super::super::layout::GameLayout;
use super::super::manifest::{AssetIndexRef, VersionJson};
use super::super::unlisted;
use super::super::version::{RemoteVersion, VersionList, VERSION_MANIFEST_V2_URL};
use super::assets::AssetIndex;
use super::InstallError;
use super::InstallErrorKind;
use crate::download::{DownloadEngine, DownloadRequest, ResourceKind};
use crate::platform::atomic::write_atomic;

/// 版本清单缓存有效期（12 小时）。过期后自动重新拉取；拉取失败退回旧缓存。
pub const MANIFEST_TTL_MS: u64 = 12 * 60 * 60 * 1000;

/// 清单缓存文件名（落在 `.ULL/cache/meta/`）。
pub const MANIFEST_CACHE_FILE: &str = "version_manifest_v2.json";

/// 继承链最大层数（防御恶意/损坏清单造成的无限展开；`resolve` 另有成环检测）。
const MAX_CHAIN_DEPTH: usize = 16;

/// 清单内容的来源（可观测性：诊断页/事件流要能说明"这次用的是缓存还是新拉的"）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestFreshness {
    /// 本次真实拉取。
    Fetched,
    /// 命中未过期的本地缓存。
    Cached,
    /// 拉取失败，退回过期缓存（离线可用）。
    StaleFallback,
}

impl ManifestFreshness {
    /// 稳定字符串 id。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fetched => "fetched",
            Self::Cached => "cached",
            Self::StaleFallback => "stale_fallback",
        }
    }
}

/// 落盘的缓存形态（带拉取时刻，TTL 判定用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManifestCache {
    fetched_at_ms: u64,
    list: VersionList,
}

/// 远端元数据来源。
pub struct RemoteSource<'a> {
    engine: &'a DownloadEngine,
    game: &'a GameLayout,
    meta_cache: &'a Path,
    manifest_url: String,
    now_ms: u64,
}

impl<'a> RemoteSource<'a> {
    /// 绑定引擎、游戏布局与缓存目录。
    #[must_use]
    pub fn new(
        engine: &'a DownloadEngine,
        game: &'a GameLayout,
        meta_cache: &'a Path,
        now_ms: u64,
    ) -> Self {
        Self {
            engine,
            game,
            meta_cache,
            manifest_url: VERSION_MANIFEST_V2_URL.to_string(),
            now_ms,
        }
    }

    /// 覆盖清单地址（内网专有清单/测试用）。
    #[must_use]
    pub fn with_manifest_url(mut self, url: impl Into<String>) -> Self {
        self.manifest_url = url.into();
        self
    }

    /// 取版本清单（带 TTL 缓存；`refresh = true` 强制重新拉取）。
    ///
    /// # Errors
    ///
    /// 无可用缓存且网络失败、或清单 JSON/校验失败时返回 [`InstallError`]。
    pub async fn manifest(
        &self,
        refresh: bool,
    ) -> Result<(VersionList, ManifestFreshness), InstallError> {
        let path = self.meta_cache.join(MANIFEST_CACHE_FILE);
        let cached = read_cache(&path);
        if !refresh {
            if let Some(cache) = &cached {
                if self.now_ms.saturating_sub(cache.fetched_at_ms) < MANIFEST_TTL_MS {
                    return Ok((cache.list.clone(), ManifestFreshness::Cached));
                }
            }
        }
        match self.fetch_manifest(&path).await {
            Ok(list) => Ok((list, ManifestFreshness::Fetched)),
            Err(error) => match cached {
                Some(cache) => Ok((cache.list, ManifestFreshness::StaleFallback)),
                None => Err(error),
            },
        }
    }

    /// 解析目标版本的扁平清单（远端取版本 JSON，递归拉父层，再走
    /// [`inherit::resolve`]）。
    ///
    /// # Errors
    ///
    /// 版本不存在（[`InstallError::version_not_found`]）、网络失败、继承链断裂/成环、
    /// JSON 非法。
    pub async fn resolve(&self, version_id: &str) -> Result<VersionJson, InstallError> {
        let (manifest, _) = self.manifest(false).await?;
        let root = self.load_one(version_id, &manifest).await?;

        let mut table: BTreeMap<String, VersionJson> = BTreeMap::new();
        table.insert(version_id.to_string(), root.clone());
        let mut visited: BTreeSet<String> = BTreeSet::new();
        visited.insert(version_id.to_string());
        let mut pending = vec![root.clone()];
        let mut depth = 0_usize;

        while let Some(layer) = pending.pop() {
            let Some(parent_id) = layer.inherits_from.clone() else {
                continue;
            };
            depth += 1;
            if depth > MAX_CHAIN_DEPTH {
                return Err(InstallError::new(
                    InstallErrorKind::Metadata,
                    format!("版本 {version_id} 的继承链超过 {MAX_CHAIN_DEPTH} 层，疑似清单损坏"),
                ));
            }
            // 环不在这里报：交给 resolve 给出完整 id 链（信息更全）。
            if !visited.insert(parent_id.clone()) {
                continue;
            }
            let parent = self.load_one(&parent_id, &manifest).await?;
            pending.push(parent.clone());
            table.insert(parent_id, parent);
        }

        inherit::resolve(&table, &root)
            .map_err(|error| InstallError::new(InstallErrorKind::Metadata, error.to_string()))
    }

    /// 取资源索引，返回（索引模型，原始字节）。
    ///
    /// 原始字节一并返回，是因为**规划必须先有索引内容**才能枚举对象；这份字节随后由
    /// [`super::execute`] 原子落到 `assets/indexes/<id>.json`，不重复下载。
    ///
    /// # Errors
    ///
    /// 索引没有下载地址、网络失败、sha1 不符或 JSON 非法。
    pub async fn asset_index(
        &self,
        reference: &AssetIndexRef,
    ) -> Result<(AssetIndex, Vec<u8>), InstallError> {
        let url = reference.url.clone().ok_or_else(|| {
            InstallError::new(
                InstallErrorKind::Metadata,
                format!("资源索引 {} 没有下载地址", reference.id),
            )
        })?;
        let bytes = self
            .fetch_bytes(ResourceKind::AssetIndex, &url, reference.sha1.as_deref())
            .await?;
        let index = AssetIndex::from_json_slice(&bytes)?;
        Ok((index, bytes))
    }

    /// 拉清单并落缓存（成功才写，失败不动旧缓存）。
    async fn fetch_manifest(&self, cache_path: &Path) -> Result<VersionList, InstallError> {
        let request = DownloadRequest::new(
            ResourceKind::VersionManifest,
            self.manifest_url.clone(),
            cache_path,
        );
        let bytes = self.engine.download_bytes(request).await?;
        let mut list =
            VersionList::from_json_str(std::str::from_utf8(&bytes).map_err(|_| {
                InstallError::new(InstallErrorKind::Metadata, "版本清单不是 UTF-8")
            })?)
            .map_err(|error| InstallError::new(InstallErrorKind::Metadata, error.to_string()))?;
        // 官方清单没有的远古版本/魔改条目由内嵌补充清单兜底（analyse/03 §2.3）。
        unlisted::merge_into(&mut list);
        let cache = ManifestCache {
            fetched_at_ms: self.now_ms,
            list: list.clone(),
        };
        let json = serde_json::to_vec(&cache).map_err(|error| {
            InstallError::new(
                InstallErrorKind::Metadata,
                format!("序列化清单缓存失败：{error}"),
            )
        })?;
        write_atomic(cache_path, &json)?;
        Ok(list)
    }

    /// 取一份版本 JSON：远端优先（带 v2 的 sha1 校验），本地 `versions/<id>/<id>.json` 兜底。
    async fn load_one(
        &self,
        id: &str,
        manifest: &VersionList,
    ) -> Result<VersionJson, InstallError> {
        let remote = match manifest.find(id) {
            Some(entry) => Some(self.fetch_version_json(entry).await),
            None => None,
        };
        match remote {
            Some(Ok(version)) => Ok(version),
            Some(Err(remote_error)) => {
                let local = self.read_local(id)?;
                match local {
                    Some(version) => Ok(version),
                    None => Err(remote_error),
                }
            }
            None => match self.read_local(id)? {
                Some(version) => Ok(version),
                None => Err(InstallError::version_not_found(id)),
            },
        }
    }

    async fn fetch_version_json(&self, entry: &RemoteVersion) -> Result<VersionJson, InstallError> {
        let bytes = self
            .fetch_bytes(
                ResourceKind::VersionManifest,
                &entry.url,
                entry.sha1.as_deref(),
            )
            .await?;
        VersionJson::from_json_slice(&bytes).map_err(InstallError::from)
    }

    async fn fetch_bytes(
        &self,
        kind: ResourceKind,
        url: &str,
        sha1: Option<&str>,
    ) -> Result<Vec<u8>, InstallError> {
        let mut request = DownloadRequest::new(kind, url, self.meta_cache.join("download.bin"));
        if let Some(hex) = sha1.filter(|hex| !hex.is_empty()) {
            request = request.with_sha1(hex);
        }
        Ok(self.engine.download_bytes(request).await?)
    }

    /// 读本地版本 JSON（`versions/<id>/<id>.json`）；不存在返回 `Ok(None)`。
    fn read_local(&self, id: &str) -> Result<Option<VersionJson>, InstallError> {
        let path: PathBuf = self.game.version_json(id);
        match std::fs::read(&path) {
            Ok(bytes) => VersionJson::from_json_slice(&bytes)
                .map(Some)
                .map_err(|error| InstallError::new(InstallErrorKind::Metadata, error.to_string())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(InstallError::new(
                InstallErrorKind::Io,
                format!("读 {} 失败：{error}", path.display()),
            )),
        }
    }
}

/// 读清单缓存（文件缺失/损坏都当没有，不报错——缓存坏了就重新拉）。
fn read_cache(path: &Path) -> Option<ManifestCache> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// 当前毫秒时间戳（缓存 TTL 判定用；与下载引擎同一个时钟来源）。
pub use crate::download::cache::now_ms;

/// 把 [`GameError`] 转成"缺元数据"错误（供上层统一处理）。
#[must_use]
pub fn metadata_error(error: GameError) -> InstallError {
    InstallError::new(InstallErrorKind::Metadata, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{metadata_error, now_ms, read_cache, ManifestCache, MANIFEST_TTL_MS};
    use crate::game::error::GameError;
    use crate::game::version::VersionList;

    #[test]
    fn ttl_constant_is_half_a_day() {
        assert_eq!(MANIFEST_TTL_MS, 43_200_000);
    }

    #[test]
    fn now_ms_is_a_plausible_wall_clock() {
        // 2020-01-01 之后、2100 年之前：只用来挡住"单位错成秒"这类退化。
        let now = now_ms();
        assert!(now > 1_577_836_800_000, "now_ms 太小：{now}");
        assert!(now < 4_102_444_800_000, "now_ms 太大：{now}");
    }

    #[test]
    fn cache_roundtrip_and_corruption_tolerance() {
        let dir = std::env::temp_dir().join(format!("ull-manifest-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("建临时目录");
        let path = dir.join("version_manifest_v2.json");

        let cache = ManifestCache {
            fetched_at_ms: 123,
            list: VersionList::default(),
        };
        std::fs::write(&path, serde_json::to_vec(&cache).expect("序列化")).expect("写");
        let read = read_cache(&path).expect("应读回");
        assert_eq!(read.fetched_at_ms, 123);

        std::fs::write(&path, b"not json").expect("写坏内容");
        assert!(read_cache(&path).is_none());

        assert!(read_cache(&dir.join("missing.json")).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn game_error_becomes_metadata_error() {
        let error = metadata_error(GameError::MissingVersionId);
        assert_eq!(error.kind().as_str(), "install_metadata");
    }
}
