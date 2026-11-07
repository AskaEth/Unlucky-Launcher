//! 加载器元数据的网络层：版本列表 + 官方 profile/installer → [`LoaderRelease`]。
//!
//! 分工与 M27 的 [`crate::game::install::RemoteSource`] 一样：**只发请求、只解析**，
//! 不写游戏目录（installer jar 落 `.ULL/cache/meta/` 作离线复用），镜像/退避/health 全在
//! `core::download` 的引擎里（我们一律递**官方地址**，重写由镜像表负责）。
//!
//! 两族的取数路径完全不同：
//!
//! ```text
//! Fabric / Quilt : meta API → profile/json（官方直出可用版本 JSON）
//! NeoForge/Forge : maven-metadata.xml → installer jar → 读 install_profile.json + version.json
//! ```

use std::fs::File;
use std::path::{Path, PathBuf};

use zip::ZipArchive;

use super::fabric;
use super::forge::{self, InstallProfile};
use super::maven;
use super::plan::LoaderRelease;
use super::{LoaderError, LoaderKind};
use crate::download::{DownloadEngine, DownloadRequest, ResourceKind};
use crate::game::manifest::VersionJson;

/// installer jar 的缓存文件名前缀（超长/含 `+` 的版本串会被规整成安全文件名）。
const INSTALLER_TAG: &str = "installer";

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
        let url = maven::metadata_url(kind, mc_version);
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
    /// maven 系取该原版版本下最新的一版）。
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
        let requested = loader_version
            .map(str::trim)
            .filter(|value| !value.is_empty());
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

        let maven_version = match requested {
            Some(raw) => maven::normalize_maven_version(kind, mc_version, raw),
            None => self
                .loader_versions(kind, mc_version)
                .await?
                .into_iter()
                .next()
                .ok_or_else(|| {
                    LoaderError::not_found(format!("{} 没有可用的加载器版本", kind.label()))
                })?,
        };
        let url = maven::installer_url(kind, mc_version, &maven_version);
        let path = self.installer_path(kind, &maven_version);
        let jar = self.fetch_installer(&url, &path).await?;
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
        Ok(release)
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
        let dest = self.meta_cache.join("loader-meta.bin");
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

/// 读 installer jar 里的 `install_profile.json` 与内嵌的版本 JSON。
///
/// 内嵌版本 JSON 的路径来自 profile 的 `json` 字段（通常 `/version.json`）；
/// 该条目不存在时返回 `None`，由 [`forge::build_release`] 判定为 unsupported。
///
/// # Errors
///
/// 不是合法 zip、缺 `install_profile.json`、profile/版本 JSON 无法解析时返回 metadata 错误。
pub fn read_installer(path: &Path) -> Result<(InstallProfile, Option<VersionJson>), LoaderError> {
    let file = File::open(path).map_err(io_error(path))?;
    let mut archive = ZipArchive::new(file).map_err(|error| {
        LoaderError::metadata(format!("{} 不是合法 zip：{error}", path.display()))
    })?;

    let profile_text = read_entry(&mut archive, "install_profile.json")?.ok_or_else(|| {
        LoaderError::metadata(format!(
            "{} 里没有 install_profile.json（不是 Forge/NeoForge installer？）",
            path.display()
        ))
    })?;
    let profile: InstallProfile = serde_json::from_str(&profile_text).map_err(|error| {
        LoaderError::metadata(format!("install_profile.json 解析失败：{error}"))
    })?;

    let embedded = match profile
        .version_json_path
        .as_deref()
        .map(|entry| entry.trim_start_matches('/'))
        .filter(|entry| !entry.is_empty())
    {
        Some(entry) => match read_entry(&mut archive, entry)? {
            Some(text) => Some(VersionJson::from_json_str(&text).map_err(|error| {
                LoaderError::metadata(format!("installer 内 {entry} 解析失败：{error}"))
            })?),
            None => None,
        },
        None => None,
    };
    Ok((profile, embedded))
}

/// 读一个 zip 条目成字符串；条目不存在返回 `Ok(None)`。
fn read_entry(archive: &mut ZipArchive<File>, name: &str) -> Result<Option<String>, LoaderError> {
    use std::io::Read;

    let mut entry = match archive.by_name(name) {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(error) => {
            return Err(LoaderError::metadata(format!(
                "读 installer 条目 {name} 失败：{error}"
            )))
        }
    };
    let mut text = String::new();
    entry
        .read_to_string(&mut text)
        .map_err(|error| LoaderError::io(format!("installer 条目 {name} 不是可读文本：{error}")))?;
    Ok(Some(text))
}

fn io_error(path: &Path) -> impl FnOnce(std::io::Error) -> LoaderError + '_ {
    move |error| LoaderError::io(format!("{} 读写失败：{error}", path.display()))
}
