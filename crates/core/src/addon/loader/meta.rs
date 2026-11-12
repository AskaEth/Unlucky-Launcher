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
use super::forge::{self, InstallProfile};
use super::liteloader;
use super::maven;
use super::plan::LoaderRelease;
use super::{LoaderError, LoaderKind};
use crate::download::{DownloadEngine, DownloadRequest, ResourceKind};
use crate::game::manifest::VersionJson;

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
        let url = maven::installer_url(kind, mc_version, &maven_version)
            .ok_or_else(|| non_maven(kind))?;
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
        release.parent_version_id = parent_version_id.to_string();
        Ok(release)
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
