//! 版本 JSON（`versions/<id>/<id>.json`）的数据模型。
//!
//! 依据：`DevDocs/analyse/03-version-download.md` §4.1（Manifest + Patch 两层模型）、
//! §4.2（宽松解析 + `rawJson` 保真）、§4.6（旧版 JSON 的四个特殊分支）。
//!
//! 一个类型吃两种形态：磁盘上的实例清单（`root`/`hidden`/`patches`）与可合并的补丁层
//! （多一个 `priority`）。合并语义不在这里，而在 [`super::inherit`]——模型只管形状。
//!
//! **保真**：未知字段全部落进 `extra`（`#[serde(flatten)]`），回写时原样带出。
//! 第三方启动器的私有字段（`MMC-*`、`tlauncherVersion`…）因此不会丢。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::arguments::Arguments;
use super::error::GameError;
use super::library::Library;
use super::version::ReleaseType;

/// 单个文件的下载信息（`downloads.client`、库 artifact、资源索引、日志配置都用它）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadInfo {
    /// 内容 id（资源索引/日志配置用；例如日志配置的 `client-1.12.xml`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// SHA-1 校验值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    /// 字节数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// 下载地址（老版本的版本 JSON 可能完全没有）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// 库文件相对 `libraries/` 的路径（Mojang 在 artifact 里给出）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl DownloadInfo {
    /// URL 是否可用。
    pub fn has_url(&self) -> bool {
        self.url.as_deref().is_some_and(|u| !u.is_empty())
    }
}

/// 版本 JSON 的 `downloads` 对象：`client` / `server` / `client_mappings` / `server_mappings`。
///
/// 用"字符串键 + 映射"而不是枚举，是因为魔改 JSON 里会出现未知键
/// （analyse/03 §4.2：HMCL 对未知键 `catch` 后忽略），枚举键会让整份 JSON 解析失败。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Downloads(pub BTreeMap<String, DownloadInfo>);

impl Downloads {
    /// 客户端 jar 的下载信息。
    pub fn client(&self) -> Option<&DownloadInfo> {
        self.0.get("client")
    }

    /// 服务端 jar 的下载信息。
    pub fn server(&self) -> Option<&DownloadInfo> {
        self.0.get("server")
    }

    /// 按原始键取。
    pub fn get(&self, key: &str) -> Option<&DownloadInfo> {
        self.0.get(key)
    }

    /// 条目数。
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// 日志配置（`logging.client`）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggingInfo {
    /// JVM 参数模板，含 `${path}` 占位符。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub argument: Option<String>,
    /// 配置文件本体。
    #[serde(default)]
    pub file: DownloadInfo,
    /// `log4j2-xml` 之类。
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

/// 版本 JSON 的 `logging` 对象（键通常是 `client`）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Logging(pub BTreeMap<String, LoggingInfo>);

impl Logging {
    /// 客户端日志配置。
    pub fn client(&self) -> Option<&LoggingInfo> {
        self.0.get("client")
    }
}

/// `javaVersion`：组件名 + 主版本号（D13 的 MC↔Java 映射用）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    /// 官方 Java 运行时组件名，例如 `java-runtime-gamma`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    /// 主版本号，例如 `17`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub major_version: Option<u32>,
}

/// `assetIndex`：资源索引的引用。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndexRef {
    /// 索引 id（`1.20`、`legacy`…），对应 `assets/indexes/<id>.json`。
    #[serde(default)]
    pub id: String,
    /// 索引文件 SHA-1。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    /// 索引文件字节数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// 全部对象的总字节数（`totalSize`，进度估算用）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_size: Option<u64>,
    /// 索引下载地址。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// 一份版本 JSON（磁盘上的 `<id>.json`，或一个可合并的补丁层）。
///
/// 字段几乎全部可空——这是"容忍残缺清单"的前提（analyse/03 §4.1：HMCL 22 个字段全可空）。
/// 解析后的不变量（由 [`super::inherit::resolve`] 保证）：`inherits_from == None`、
/// `jar` 已回退成实例 id、`patches` 已清空、`root == Some(true)`。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VersionJson {
    /// 版本 id。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 展示名（部分整合包写入）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 继承的父版本 id。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inherits_from: Option<String>,
    /// 启动用的 jar 所在版本 id；`None` 表示用本实例的 jar。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jar: Option<String>,
    /// 主类。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main_class: Option<String>,
    /// 旧版单串游戏参数（pre-1.13），与 `arguments` 并存。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minecraft_arguments: Option<String>,
    /// 新版结构化参数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Arguments>,
    /// 资源版本名（`1.20`、`legacy`…）；缺失按 `legacy` 处理（analyse/03 §4.6）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assets: Option<String>,
    /// 资源索引引用。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_index: Option<AssetIndexRef>,
    /// 兼容级别。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compliance_level: Option<u32>,
    /// 需要的 Java 版本。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub java_version: Option<JavaVersion>,
    /// 依赖库列表。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub libraries: Option<Vec<Library>>,
    /// 下载信息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloads: Option<Downloads>,
    /// 日志配置。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<Logging>,
    /// 版本分类。
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub version_type: Option<ReleaseType>,
    /// 最后修改时间。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    /// 发布时间。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_time: Option<String>,
    /// 启动器最低版本要求。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum_launcher_version: Option<u32>,
    /// 是否为根清单（HMCL 私有字段）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<bool>,
    /// 是否在 UI 中隐藏（HMCL 私有字段）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    /// 补丁层，按 `priority` 升序合并（HMCL 私有字段）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patches: Option<Vec<VersionJson>>,
    /// 补丁优先级；`None` 视作最小值（最先合并，analyse/03 §4.1）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,
    /// 未识别的字段，原样保留（`rawJson` 保真）。
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl VersionJson {
    /// 解析一份版本 JSON。
    pub fn from_json_str(json: &str) -> Result<Self, GameError> {
        serde_json::from_str(json).map_err(|e| GameError::json("version_json", e))
    }

    /// 从字节解析（直接吃 HTTP 响应体）。
    pub fn from_json_slice(bytes: &[u8]) -> Result<Self, GameError> {
        serde_json::from_slice(bytes).map_err(|e| GameError::json("version_json", e))
    }

    /// 序列化回 JSON。
    pub fn to_json_string(&self) -> Result<String, GameError> {
        serde_json::to_string(self).map_err(|e| GameError::json("version_json", e))
    }

    /// 取 `id`，缺失则报错。
    pub fn require_id(&self) -> Result<&str, GameError> {
        self.id.as_deref().ok_or(GameError::MissingVersionId)
    }

    /// 客户端 jar 的下载信息；老版本 JSON 没有 `downloads.client`（analyse/03 §4.6），
    /// 兜底 URL 由 download/launch 侧按镜像表拼，不在模型里硬编码 BMCLAPI。
    pub fn client_jar(&self) -> Option<&DownloadInfo> {
        self.downloads.as_ref().and_then(Downloads::client)
    }

    /// 资源版本名，缺失时按 `legacy`（analyse/03 §4.6 第 3 条）。
    pub fn assets_id(&self) -> &str {
        self.assets.as_deref().unwrap_or("legacy")
    }

    /// 是否已是被解析过的扁平清单。
    pub fn is_resolved(&self) -> bool {
        self.inherits_from.is_none() && self.patches.is_none() && self.jar.is_some()
    }

    /// 版本是否带 Vulkan 后端（`libraries` 含 `org.lwjgl:lwjgl-vulkan`）。
    ///
    /// 只是**必要条件**：`26.2-snapshot-1` 有库但还没有 `--graphicsBackend`
    /// （差两天，实测），下发门限见 [`crate::game::capability`] 的模块文档。
    pub fn has_vulkan_backend(&self) -> bool {
        super::capability::VersionCapabilities::from_json(self).has_vulkan_backend()
    }
}
