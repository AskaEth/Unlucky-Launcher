//! 加载器元数据的网络层：版本列表 + 官方 profile/installer → [`LoaderRelease`]。
//!
//! 分工与 M27 的 [`crate::game::install::RemoteSource`] 一样：**只发请求、只解析**，
//! 不写游戏目录（installer jar 落 `.ULL/cache/meta/` 作离线复用），镜像/退避/health 全在
//! `core::download` 的引擎里（我们一律递**官方地址**，重写由镜像表负责）。
//!
//! 三个数据源的取数路径完全不同（**三路显式派发**，不许再有「不是 fabric 就是 maven」）：
//!
//! ```text
//! Fabric / Quilt : meta API → profile/json（官方直出可用版本 JSON）
//! NeoForge/Forge : maven-metadata.xml → installer jar → 读 install_profile.json + version.json
//! LiteLoader     : dl.liteloader.com/versions.json →（快照再读 maven-metadata.xml）→ 从零合成
//! ```

use std::fs::File;
use std::path::{Path, PathBuf};

use zip::ZipArchive;

use super::fabric;
use super::forge;
use super::installer_zip::{io_error, read_installer};
use super::liteloader;
use super::maven;
use super::plan::LoaderRelease;
use super::{LoaderError, LoaderErrorKind, LoaderKind};
use crate::download::{DownloadEngine, DownloadRequest, ResourceKind};

/// installer jar 的缓存文件名前缀（超长/含 `+` 的版本串会被规整成安全文件名）。
const INSTALLER_TAG: &str = "installer";

/// LiteLoader 官方版本表的缓存文件名（整表 49 KB，一次取全量再本地筛）。
const LITELOADER_LIST_TAG: &str = "liteloader-versions.json";

/// 远端加载器元数据来源。
pub struct LoaderSource<'a> {
    engine: &'a DownloadEngine,
    meta_cache: &'a Path,
}

impl<'a> LoaderSource<'a> {
    /// 绑定下载引擎与元数据缓存目录（`.ULL/cache/meta/`）。
    #[must_use]
    pub fn new(engine: &'a DownloadEngine, meta_cache: &'a Path) -> Self {
        Self { engine, meta_cache }
    }

    /// 取该原版版本可用的加载器版本列表（**最新在前**）。
    ///
    /// # Errors
    ///
    /// 网络失败、元数据解析失败、该原版版本没有加载器版本时返回 [`LoaderError`]。
    pub async fn loader_versions(
        &self,
        kind: LoaderKind,
        mc_version: &str,
    ) -> Result<Vec<String>, LoaderError> {
        if kind.is_fabric_family() {
            let url = fabric::loader_list_url(kind, mc_version);
            let body = self.fetch_text(&url).await?;
            let entries = fabric::parse_loader_list(kind, &body)?;
            return Ok(entries.into_iter().map(|entry| entry.version).collect());
        }
        if kind == LoaderKind::LiteLoader {
            let table = self.liteloader_table().await?;
            let entries = table.get(mc_version.trim()).ok_or_else(|| {
                LoaderError::not_found(format!(
                    "LiteLoader 没有适配原版 {mc_version} 的版本（官方版本表 {} 条原版版本）",
                    table.len()
                ))
            })?;
            return Ok(entries.iter().map(|entry| entry.version.clone()).collect());
        }
        let url = maven::metadata_url(kind, mc_version).ok_or_else(|| non_maven(kind))?;
        let body = self.fetch_text(&url).await?;
        let all = maven::parse_metadata_versions(&body);
        // maven 表是升序的，统一成"最新在前"。
        let mut picked = maven::select_versions(kind, mc_version, &all);
        picked.reverse();
        if picked.is_empty() {
            return Err(LoaderError::not_found(format!(
                "{} 在 {} 上没有可用版本（maven 表 {} 条，全部与该原版版本不匹配）",
                kind.label(),
                mc_version,
                all.len()
            )));
        }
        Ok(picked)
    }

    /// 解析一个加载器版本（`loader_version` 为 `None` 时取官方默认：Fabric 系取首个稳定版，
    /// maven 系取该原版版本下最新的一版，LiteLoader 取首个 release / 否则最新）。
    ///
    /// 等价于 [`Self::resolve_on`]，父版本 = 原版本身。
    ///
    /// # Errors
    ///
    /// 网络失败、版本不存在、installer 缺 profile、协议不受支持时返回 [`LoaderError`]。
    pub async fn resolve(
        &self,
        kind: LoaderKind,
        mc_version: &str,
        loader_version: Option<&str>,
    ) -> Result<LoaderRelease, LoaderError> {
        self.resolve_on(kind, mc_version, loader_version, None)
            .await
    }

    /// 解析一个加载器版本，并**指定父版本**（`parent` 为 `None` 时等于 `mc_version`）。
    ///
    /// 目前只有 LiteLoader 接受「父版本不是原版」：它靠 `--tweakClass` 追加生效，叠在
    /// Forge 1.12.2 上是官方主推组合。其它加载器声明自己的主类/启动目标，叠在别人的 loader
    /// 层上没有装配路径，因此**具名拒绝**，而不是默默装成「叠在原版上」。
    ///
    /// # Errors
    ///
    /// 同上；另外「非 LiteLoader 却指定了非原版父版本」返回 `unsupported`。
    pub async fn resolve_on(
        &self,
        kind: LoaderKind,
        mc_version: &str,
        loader_version: Option<&str>,
        parent: Option<&str>,
    ) -> Result<LoaderRelease, LoaderError> {
        let requested = loader_version
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let parent_version_id = parent
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(mc_version);
        if parent_version_id != mc_version && kind != LoaderKind::LiteLoader {
            return Err(LoaderError::unsupported(format!(
                "{} 没有「叠在 {parent_version_id} 上」的装配路线：它自己声明主类与启动目标，只能叠在原版 {mc_version} 上",
                kind.label()
            ))
            .with_hint("先装原版，再在其上安装这一家加载器"));
        }
        if kind.is_fabric_family() {
            let version = match requested {
                Some(version) => version.to_string(),
                None => {
                    let url = fabric::loader_list_url(kind, mc_version);
                    let body = self.fetch_text(&url).await?;
                    let entries = fabric::parse_loader_list(kind, &body)?;
                    fabric::pick_default(&entries)
                        .map(|entry| entry.version.clone())
                        .ok_or_else(|| {
                            LoaderError::not_found(format!("{} 没有可用的加载器版本", kind.label()))
                        })?
                }
            };
            let url = fabric::profile_url(kind, mc_version, &version);
            let body = self.fetch_text(&url).await?;
            return fabric::build_release(kind, mc_version, &version, &body);
        }
        if kind == LoaderKind::LiteLoader {
            return self
                .resolve_liteloader(mc_version, requested, parent_version_id)
                .await;
        }

        let candidates = match requested {
            Some(raw) => maven::maven_version_candidates(kind, mc_version, raw),
            None => vec![self
                .loader_versions(kind, mc_version)
                .await?
                .into_iter()
                .next()
                .ok_or_else(|| {
                    LoaderError::not_found(format!("{} 没有可用的加载器版本", kind.label()))
                })?],
        };
        let (maven_version, jar) = self.installer_for(kind, mc_version, &candidates).await?;
        let (profile, embedded) = read_installer(&jar)?;
        let mut release = forge::build_release(
            kind,
            mc_version,
            &maven_version,
            &profile,
            embedded.as_ref(),
        )?;
        // M35 执行器要的两样东西：installer 本体（跑 processor 与解包）与它声明的 profile。
        release.installer = Some(jar);
        release.parent_version_id = parent_version_id.to_string();
        Ok(release)
    }

    /// 按候选顺序取 installer jar；**只有 404 才换下一条候选**。
    ///
    /// 为什么只在 404 时换：老版 Forge 的 maven 版本串带**双后缀**
    /// （`1.7.10-10.13.4.1614-1.7.10`），用户只写 `10.13.4.1614` 时归一出的第一条坐标在
    /// maven 上并不存在（[`maven::maven_version_candidates`] 补的第二条才是真的）。
    /// 反过来，网络不通时换候选只是把同一个故障再试一遍，还会把「版本串拼错」和
    /// 「网络失败」搅成同一句 `loader_network`——这两件事必须报不同的码。
    ///
    /// # Errors
    ///
    /// 全部候选都 404 时返回 [`LoaderErrorKind::NotFound`]（消息里列全试过的坐标）；
    /// 任何一次网络失败原样上抛（不换候选）。
    async fn installer_for(
        &self,
        kind: LoaderKind,
        mc_version: &str,
        candidates: &[String],
    ) -> Result<(String, PathBuf), LoaderError> {
        for candidate in candidates {
            let url =
                maven::installer_url(kind, mc_version, candidate).ok_or_else(|| non_maven(kind))?;
            let path = self.installer_path(kind, candidate);
            match self.fetch_installer(&url, &path).await {
                Ok(jar) => return Ok((candidate.clone(), jar)),
                // 404/410 = 这条坐标在官方源上不存在，值得换下一条候选。
                Err(error) if error.kind() == LoaderErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            }
        }
        Err(LoaderError::not_found(format!(
            "{} 的 installer 在官方 maven 上没有这些坐标（全部 404）：{}",
            kind.label(),
            candidates.join(" / ")
        ))
        .with_hint(
            "这是「版本串拼不出真实坐标」，不是网络故障。`--loader-version` 要写 maven-metadata.xml 里的完整版本串（老版本带双后缀：1.7.10 是 1.7.10-10.13.4.1614-1.7.10、1.8.9 是 1.8.9-11.15.1.2318-1.8.9）",
        ))
    }

    /// LiteLoader：整表 → 选条 →（快照再查 maven-metadata）→ 合成清单。
    async fn resolve_liteloader(
        &self,
        mc_version: &str,
        requested: Option<&str>,
        parent_version_id: &str,
    ) -> Result<LoaderRelease, LoaderError> {
        let table = self.liteloader_table().await?;
        let entries = table.get(mc_version.trim()).ok_or_else(|| {
            LoaderError::not_found(format!(
                "LiteLoader 没有适配原版 {mc_version} 的版本（官方版本表 {} 条原版版本）",
                table.len()
            ))
        })?;
        let entry = match requested {
            Some(raw) => liteloader::find_entry(entries, raw).ok_or_else(|| {
                LoaderError::not_found(format!(
                    "LiteLoader {} 没有版本 `{raw}`（该原版下有：{}）",
                    mc_version,
                    entries
                        .iter()
                        .map(|entry| entry.version.as_str())
                        .collect::<Vec<_>>()
                        .join(" / ")
                ))
            })?,
            None => liteloader::default_entry(entries).ok_or_else(|| {
                LoaderError::not_found(format!("LiteLoader 没有适配原版 {mc_version} 的版本"))
            })?,
        };
        // 直链：release 分支记录里就有；快照分支必须再读一次 maven-metadata.xml。
        let artifact_url = match (&entry.url, &entry.snapshot_dir) {
            (Some(url), _) => url.clone(),
            (None, Some(dir)) => {
                let metadata = self.fetch_text(&format!("{dir}maven-metadata.xml")).await?;
                liteloader::snapshot_artifact_url(entry, &metadata)?
            }
            (None, None) => {
                return Err(LoaderError::metadata(format!(
                    "LiteLoader {} 的记录既没有直链也没有快照目录",
                    entry.version
                )))
            }
        };
        liteloader::build_release(mc_version, parent_version_id, entry, &artifact_url)
    }

    /// 取 LiteLoader 整张官方版本表（按原版版本分组）。
    async fn liteloader_table(
        &self,
    ) -> Result<std::collections::BTreeMap<String, Vec<liteloader::LiteLoaderEntry>>, LoaderError>
    {
        let body = self
            .fetch_text_named(liteloader::VERSION_LIST_URL, LITELOADER_LIST_TAG)
            .await?;
        liteloader::parse_versions(&body)
    }

    /// installer jar 的本地缓存路径（`+`/非法字符规整成 `_`）。
    fn installer_path(&self, kind: LoaderKind, maven_version: &str) -> PathBuf {
        let safe: String = maven_version
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                    ch
                } else {
                    '_'
                }
            })
            .collect();
        self.meta_cache
            .join(format!("{}-{safe}-{INSTALLER_TAG}.jar", kind.as_str()))
    }

    /// 取 installer jar：本地缓存里有可读的 zip 就直接用（离线复用），否则下载。
    async fn fetch_installer(&self, url: &str, path: &Path) -> Result<PathBuf, LoaderError> {
        if path.is_file() && ZipArchive::new(File::open(path).map_err(io_error(path))?).is_ok() {
            return Ok(path.to_path_buf());
        }
        let request = DownloadRequest::new(ResourceKind::Library, url, path.to_path_buf());
        let outcome = self.engine.download(request).await?;
        Ok(outcome.path)
    }

    /// 取一小段文本元数据（版本列表 / profile JSON / maven metadata）。
    async fn fetch_text(&self, url: &str) -> Result<String, LoaderError> {
        self.fetch_text_named(url, "loader-meta.bin").await
    }

    /// 同上，缓存文件名按调用方给（LiteLoader 整表单独一档，别把小响应和 49 KB 的表混存）。
    async fn fetch_text_named(&self, url: &str, tag: &str) -> Result<String, LoaderError> {
        let dest = self.meta_cache.join(tag);
        let bytes = self
            .engine
            .download_bytes(DownloadRequest::new(
                ResourceKind::VersionManifest,
                url,
                dest,
            ))
            .await?;
        String::from_utf8(bytes)
            .map_err(|_| LoaderError::metadata(format!("元数据不是 UTF-8：{url}")))
    }
}

/// 非 maven 族被送进 maven 分支时的统一报错（入口已分流，走到这里说明调用链有 bug）。
fn non_maven(kind: LoaderKind) -> LoaderError {
    LoaderError::unsupported(format!(
        "{} 不是 maven 族加载器，没有 maven 元数据/installer 地址",
        kind.label()
    ))
}
