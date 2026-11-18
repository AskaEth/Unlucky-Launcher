//! 安装计划：把「已解析的版本清单 + 资源索引」翻译成一份可下载文件清单。
//!
//! 依据：`DevDocs/analyse/03-version-download.md` §6（安装期资源清单逐类拆解）、
//! `DevDocs/PlanDoc/05-download-and-mirrors.md`（镜像由下载引擎展开）。
//!
//! 本模块是**纯计算**：输入已解析清单（[`VersionJson`]）与已取到的资源索引，输出
//! [`InstallPlan`]；不发请求、不写文件。网络在 [`super::source`]、落盘在 [`super::execute`]。
//! 这样 `--dry-run`、测试、以及 GUI 的"安装前确认页"共用同一份计划真相。
//!
//! 恒共享层（D20）：`versions/`、`assets/`、`libraries/` 一律进游戏根，不按实例复制；
//! 只有 `hint=local` 的库落实例私有目录（见 [`super::libraries`]）。

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::super::arguments::Environment;
use super::super::error::GameError;
use super::super::layout::GameLayout;
use super::super::manifest::{Logging, VersionJson};
use super::assets::{self, AssetIndex};
use super::libraries;
use crate::download::{DownloadRequest, ResourceKind};

/// 一个待落盘文件的角色（进度分组、`--dry-run` 汇总、错误定位都用它）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileRole {
    /// 客户端 jar `versions/<jarId>/<jarId>.jar`。
    ClientJar,
    /// 共享库 `libraries/...`。
    Library,
    /// natives 分类器构件（也进 `libraries/`，启动期解压）。
    Native,
    /// 资源索引 `assets/indexes/<id>.json`。
    AssetIndex,
    /// 资源对象 `assets/objects/<hash[0..2]>/<hash>`。
    Asset,
    /// 日志配置 `assets/log_configs/<id>`。
    LogConfig,
}

impl FileRole {
    /// 稳定字符串 id（JSON 输出与日志用，**不要改名**）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ClientJar => "client_jar",
            Self::Library => "library",
            Self::Native => "native",
            Self::AssetIndex => "asset_index",
            Self::Asset => "asset",
            Self::LogConfig => "log_config",
        }
    }

    /// 人读名称（汇总行用）。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ClientJar => "客户端 jar",
            Self::Library => "依赖库",
            Self::Native => "natives",
            Self::AssetIndex => "资源索引",
            Self::Asset => "资源对象",
            Self::LogConfig => "日志配置",
        }
    }
}

/// 清单里的一个待下载文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanFile {
    /// 角色。
    pub role: FileRole,
    /// 人读标识（库坐标 / 资源名 / 索引 id / jar id）。
    pub name: String,
    /// 官方逻辑 URL（镜像展开由下载引擎负责）。
    pub url: String,
    /// 期望 SHA-1（`None` = 清单没给，只能靠大小/存在性兜底）。
    pub sha1: Option<String>,
    /// 期望字节数。
    pub size: Option<u64>,
    /// 最终落盘路径。
    pub dest: PathBuf,
    /// 资源类型（健康度统计与分块准入的第二维度）。
    pub resource: ResourceKind,
}

impl PlanFile {
    /// 转成下载引擎的请求（校验值/大小原样透传）。
    #[must_use]
    pub fn request(&self) -> DownloadRequest {
        let mut request = DownloadRequest::new(self.resource, self.url.clone(), self.dest.clone());
        request.integrity = self
            .sha1
            .clone()
            .filter(|hex| !hex.is_empty())
            .map(crate::download::Integrity::sha1);
        request.expected_size = self.size;
        request
    }

    /// `--dry-run` 的单行描述。
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "role": self.role.as_str(),
            "name": self.name,
            "url": self.url,
            "sha1": self.sha1,
            "size": self.size,
            "dest": self.dest.display().to_string(),
            "resource": self.resource.as_str(),
        })
    }
}

/// 规划阶段已经取到内容、但因 `--dry-run` / 时序原因还没落盘的元数据文件。
///
/// 典型场景：资源索引——不先取到它就无法枚举对象，所以规划期已经下载在内存里；
/// 执行期直接原子写盘，不再重复走一次网络。
#[derive(Debug, Clone)]
pub struct Prefetched {
    /// 目标路径（必须与计划里的某个 [`PlanFile::dest`] 一致）。
    pub dest: PathBuf,
    /// 内容字节。
    pub bytes: Vec<u8>,
}

/// 一次安装的完整计划。
#[derive(Debug, Clone)]
pub struct InstallPlan {
    /// 目标版本 id（`versions/<id>/`）。
    pub version_id: String,
    /// 实际提供 jar 的版本 id（继承链上可能是父版本）。
    pub jar_id: String,
    /// 游戏根目录（恒共享层的根）。
    pub game_root: PathBuf,
    /// 版本清单落盘路径 `versions/<id>/<id>.json`。
    pub version_json_path: PathBuf,
    /// 待下载文件（按 dest 去重、按角色排序）。
    pub files: Vec<PlanFile>,
    /// 已解析的扁平清单（落盘时写进 `versions/<id>/<id>.json`）。
    pub version_json: VersionJson,
    /// 规划期已取到内容的文件（见 [`Prefetched`]）。
    pub prefetched: Vec<Prefetched>,
    /// 人读告警（缺哪类资源一律如实告知，不静默降级）。
    pub warnings: Vec<String>,
}

/// [`InstallPlan::build`] 的输入。
pub struct PlanInput<'a> {
    /// 游戏目录布局。
    pub layout: &'a GameLayout,
    /// 目标版本 id。
    pub version_id: &'a str,
    /// 已 resolve 的扁平清单。
    pub resolved: &'a VersionJson,
    /// 已取到并解析的资源索引（缺 = 老版本没有 `assetIndex`）。
    pub asset_index: Option<&'a AssetIndex>,
    /// 规则求值环境（OS / 架构 / 特性开关）。
    pub environment: &'a Environment,
}

impl InstallPlan {
    /// 组计划。
    ///
    /// # Errors
    ///
    /// 资源对象的哈希不合法（索引坏了）时返回 [`GameError::InvalidAssetHash`]。
    pub fn build(input: PlanInput<'_>) -> Result<Self, GameError> {
        let layout = input.layout;
        let version_id = input.version_id;
        let resolved = input.resolved;
        let mut files: Vec<PlanFile> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();

        let jar_id = resolved
            .jar
            .clone()
            .unwrap_or_else(|| version_id.to_string());

        // ① 客户端 jar：老版本（pre-1.6）没有 `downloads.client`，如实告知而不是编 URL。
        match resolved.client_jar() {
            Some(info) if info.has_url() => files.push(PlanFile {
                role: FileRole::ClientJar,
                name: jar_id.clone(),
                url: info.url.clone().unwrap_or_default(),
                sha1: info.sha1.clone(),
                size: info.size,
                dest: layout.version_jar(&jar_id),
                resource: ResourceKind::ClientJar,
            }),
            _ => warnings.push(format!(
                "版本 {version_id} 的清单没有 downloads.client：客户端 jar 无法自动获取（pre-1.6 旧清单待补兜底）"
            )),
        }

        // ② 依赖库 + natives。
        let (library_files, library_warnings) = libraries::plan(
            layout,
            version_id,
            resolved.libraries.as_deref().unwrap_or_default(),
            input.environment,
        );
        files.extend(library_files);
        warnings.extend(library_warnings);

        // ③ 资源索引 + 逐对象。
        match resolved.asset_index.as_ref() {
            Some(reference) if reference.url.is_some() => files.push(PlanFile {
                role: FileRole::AssetIndex,
                name: reference.id.clone(),
                url: reference.url.clone().unwrap_or_default(),
                sha1: reference.sha1.clone(),
                size: reference.size,
                dest: layout.asset_index(&reference.id),
                resource: ResourceKind::AssetIndex,
            }),
            Some(reference) => warnings.push(format!(
                "资源索引 {} 没有下载地址，跳过（对象无法枚举）",
                reference.id
            )),
            None => warnings.push(format!(
                "版本 {version_id} 的清单没有 assetIndex：资源对象无法枚举（旧版 sha1 反查表待补）"
            )),
        }
        if let Some(index) = input.asset_index {
            files.extend(assets::plan(layout, index)?);
        }

        // ④ 日志配置（官方 manifest 带的 appender 配置；有就下，没有就用启动器的内置模板）。
        if let Some(logging) = resolved.logging.as_ref().and_then(Logging::client) {
            if logging.file.has_url() {
                let file_id = logging
                    .file
                    .id
                    .clone()
                    .unwrap_or_else(|| format!("{version_id}-client"));
                files.push(PlanFile {
                    role: FileRole::LogConfig,
                    name: file_id.clone(),
                    url: logging.file.url.clone().unwrap_or_default(),
                    sha1: logging.file.sha1.clone(),
                    size: logging.file.size,
                    dest: layout.log_config(&file_id),
                    resource: ResourceKind::Other,
                });
            }
        }

        let files = dedupe(files);

        Ok(Self {
            version_id: version_id.to_string(),
            jar_id,
            game_root: layout.root().to_path_buf(),
            version_json_path: layout.version_json(version_id),
            files,
            version_json: resolved.clone(),
            prefetched: Vec::new(),
            warnings,
        })
    }

    /// 挂一份"规划期已取到内容"的文件（内容必须与计划里的对应文件同一落点）。
    pub fn prefetch(&mut self, dest: PathBuf, bytes: Vec<u8>) {
        self.prefetched.push(Prefetched { dest, bytes });
    }

    /// 客户端 jar 的待下载项（`None` = 旧版清单没给下载信息）。
    #[must_use]
    pub fn client_jar(&self) -> Option<&PlanFile> {
        self.files
            .iter()
            .find(|file| file.role == FileRole::ClientJar)
    }

    /// 这份计划能否真的装出一个可启动的版本（判据：有客户端 jar）。
    ///
    /// 旧版（pre-1.6）清单没有 `downloads.client`，计划里就没有 jar——此时"装成功"
    /// 等于骗人，调用方应当拒绝执行而不是登记一个起不来的实例。
    #[must_use]
    pub fn is_installable(&self) -> bool {
        self.client_jar().is_some()
    }

    /// 文件数。
    #[must_use]
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// 总字节数（已知大小的合计；`unknown_size` 给出"有几个文件大小未知"）。
    #[must_use]
    pub fn total_bytes(&self) -> (u64, usize) {
        let mut total = 0_u64;
        let mut unknown = 0_usize;
        for file in &self.files {
            match file.size {
                Some(size) => total += size,
                None => unknown += 1,
            }
        }
        (total, unknown)
    }

    /// 按角色统计（角色 → （文件数, 字节数））。
    #[must_use]
    pub fn by_role(&self) -> BTreeMap<FileRole, (usize, u64)> {
        let mut out: BTreeMap<FileRole, (usize, u64)> = BTreeMap::new();
        for file in &self.files {
            let entry = out.entry(file.role).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += file.size.unwrap_or(0);
        }
        out
    }
}

/// 按落盘路径去重：同一目标只留一条，优先保留带 sha1 的那条（后出现的更可信时覆盖）。
fn dedupe(files: Vec<PlanFile>) -> Vec<PlanFile> {
    let mut by_dest: BTreeMap<PathBuf, PlanFile> = BTreeMap::new();
    for file in files {
        match by_dest.get(&file.dest) {
            Some(existing) if existing.sha1.is_some() || file.sha1.is_none() => {}
            _ => {
                by_dest.insert(file.dest.clone(), file);
            }
        }
    }
    let mut out: Vec<PlanFile> = by_dest.into_values().collect();
    out.sort_by(|left, right| {
        left.role
            .cmp(&right.role)
            .then_with(|| left.dest.cmp(&right.dest))
    });
    out
}
