//! 报告里的**条目类型**：一条文件、一条冲突、一条缺失、一个 component、一个加载器目标。

use serde::Serialize;

use super::keys;
use crate::addon::loader::LoaderKind;
use crate::addon::pack::PackSide;

/// 一条文件来自哪里。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileSource {
    /// 包内 `overrides/`（或 CF 清单点名的那个目录 / MultiMC 的 `.minecraft/`）。
    Overrides,
    /// 包内 `client-overrides/`。
    ClientOverrides,
    /// CurseForge `files[]` 条目（靠 projectID/fileID 补全）。
    CurseForge,
    /// Modrinth `files[]` 条目（清单自带 path + downloads）。
    Modrinth,
}

impl FileSource {
    /// 从覆盖目录的侧别取来源。
    #[must_use]
    pub const fn from_side(side: PackSide) -> Self {
        match side {
            PackSide::Both => Self::Overrides,
            PackSide::Client => Self::ClientOverrides,
            // 服务端覆盖在客户端导入里不会落地（见 `plan` 的侧别优先级）。
            PackSide::Server => Self::Overrides,
        }
    }
}

/// 一个文件本次被怎么处理。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileAction {
    /// 新写入（磁盘上原本没有）。
    Installed,
    /// 覆盖写入（用户没改过，整合包升级）。
    Replaced,
    /// 磁盘内容与包内一致，没动。
    SkippedIdentical,
    /// 用户改过 / 删过，按策略保留了用户那一侧。
    KeptUser,
}

impl FileAction {
    /// 是否真的写了盘。
    #[must_use]
    pub const fn wrote(self) -> bool {
        matches!(self, Self::Installed | Self::Replaced)
    }
}

/// 冲突的成因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictReason {
    /// 基线里记过这个文件，磁盘内容与基线不同 ⇒ 用户改过。
    UserModified,
    /// 基线里记过这个文件，磁盘上却没了 ⇒ 用户删过。
    UserDeleted,
    /// 没有基线可判，磁盘上本来就有同名文件。
    PreExisting,
}

impl ConflictReason {
    /// 文案 key。
    #[must_use]
    pub const fn message_key(self) -> &'static str {
        match self {
            Self::UserModified => keys::CONFLICT_USER_MODIFIED,
            Self::UserDeleted => keys::CONFLICT_USER_DELETED,
            Self::PreExisting => keys::CONFLICT_PRE_EXISTING,
        }
    }
}

/// 冲突实际采用的处置（由 [`super::super::ConflictPolicy`] / [`super::super::ConflictDecisions`] 决定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictChoice {
    /// 保留用户侧（不写盘）。
    KeepUser,
    /// 用包内内容覆盖（用户选了"覆盖"）。
    Overwrite,
}

impl Default for ConflictChoice {
    /// **缺省是保留**：没有决策时绝不覆盖用户已有文件（M160 的硬线，见 `import::decision`）。
    fn default() -> Self {
        Self::KeepUser
    }
}

/// 一条冲突（"无法判定时给可选择的冲突清单"就是这张表）。
#[derive(Debug, Clone, Serialize)]
pub struct ConflictFile {
    /// 实例内相对路径。
    pub path: String,
    /// 成因。
    pub reason: ConflictReason,
    /// 本次实际采用的一侧。
    pub choice: ConflictChoice,
    /// 文案 key（= `reason.message_key()`）。
    pub message_key: &'static str,
    /// 包内声明的 SHA-1。
    pub pack_sha1: String,
    /// 包内声明的字节数。
    pub pack_size: u64,
    /// 上次导入写下的 SHA-1（没有基线时 `None`）。
    pub base_sha1: Option<String>,
    /// 磁盘现状的 SHA-1（文件不存在时 `None`）。
    pub disk_sha1: Option<String>,
}

/// 一个"搞不到"的文件（**容忍**，但必须逐条列出）。
#[derive(Debug, Clone, Serialize)]
pub struct MissingFile {
    /// 实例内相对路径（mrpack 有；CF 补全失败时没有）。
    pub path: Option<String>,
    /// 人读标识（CF 条目是 `projectID:fileID`，其余是路径）。
    pub display: String,
    /// CF `projectID`。
    pub project_id: Option<i64>,
    /// CF `fileID`。
    pub file_id: Option<i64>,
    /// 为什么搞不到。
    pub reason: MissingReason,
    /// 文案 key。
    pub message_key: &'static str,
    /// 细节（错误原文）。
    pub detail: String,
}

/// "搞不到"的成因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingReason {
    /// 调用方没给 fetcher（离线）。
    Offline,
    /// 条目补不出下载地址（CF 缺 API 客户端 / `downloads` 为空）。
    NoSource,
    /// 尝试过但失败（404 / 网络）。
    FetchFailed,
}

impl MissingReason {
    /// 文案 key。
    #[must_use]
    pub const fn message_key(self) -> &'static str {
        match self {
            Self::Offline => keys::MISSING_OFFLINE,
            Self::NoSource => keys::MISSING_NO_SOURCE,
            Self::FetchFailed => keys::MISSING_FETCH_FAILED,
        }
    }
}

/// 一个 MultiMC / Prism component 被识别为"本启动器不支持"（**逐项**列出，不静默丢）。
#[derive(Debug, Clone, Serialize)]
pub struct RejectedComponent {
    /// 原始 uid（`org.lwjgl3` / `net.fabricmc.intermediary` / 社区自定义）。
    pub uid: String,
    /// 该组件声明的版本。
    pub version: Option<String>,
    /// 文案 key（恒为 [`keys::REJECTED_COMPONENT`]）。
    pub message_key: &'static str,
}

/// 加载器安装计划的输入：**只需要这三个参数**就能接上 `addon::loader` 的既有能力。
#[derive(Debug, Clone)]
pub struct LoaderTarget {
    /// 加载器种类。
    pub kind: LoaderKind,
    /// 目标原版版本。
    pub mc_version: String,
    /// 加载器版本（已剥掉 CF 的 MC 版本前缀）。
    pub loader_version: String,
    /// 清单里的原始标识（诊断用）。
    pub raw: String,
    /// 实例绑定的版本 id（`1.20.1-fabric-0.15.11` 或纯原版 `1.20.1`）。
    pub version_id: String,
}

impl LoaderTarget {
    /// 直接喂给 `LoaderSource::resolve(kind, mc_version, Some(loader_version))`。
    #[must_use]
    pub fn resolve_args(&self) -> (LoaderKind, &str, &str) {
        (self.kind, &self.mc_version, &self.loader_version)
    }
}

impl Serialize for LoaderTarget {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;

        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("kind", self.kind.as_str())?;
        map.serialize_entry("label", self.kind.label())?;
        map.serialize_entry("mc_version", &self.mc_version)?;
        map.serialize_entry("loader_version", &self.loader_version)?;
        map.serialize_entry("version_id", &self.version_id)?;
        map.serialize_entry("raw", &self.raw)?;
        map.end()
    }
}

/// 一条落地的文件。
#[derive(Debug, Clone, Serialize)]
pub struct ImportedFile {
    /// 实例内相对路径（`.disabled` 后缀原样保留）。
    pub path: String,
    /// 来源。
    pub source: FileSource,
    /// 实际写入/比对用的 SHA-1。
    pub sha1: Option<String>,
    /// 字节数。
    pub bytes: u64,
    /// 是否禁用态（路径后缀表达，不附加语义）。
    pub disabled: bool,
    /// 处理方式。
    pub action: FileAction,
    /// 可选注记（`.disabled` / `optional` 之类）。
    pub note: Option<ImportIssue>,
}

/// 一条人读注记（带稳定 key）。
#[derive(Debug, Clone, Serialize)]
pub struct ImportIssue {
    /// 文案 key。
    pub message_key: &'static str,
    /// 人读细节。
    pub message: String,
    /// 相关实例内路径。
    pub path: Option<String>,
}

impl ImportIssue {
    /// 构造。
    #[must_use]
    pub fn new(message_key: &'static str, message: impl Into<String>) -> Self {
        Self {
            message_key,
            message: message.into(),
            path: None,
        }
    }

    /// 挂一条路径。
    #[must_use]
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }
}

/// 登记出来的实例（给 UI 显示"装到哪了"）。
#[derive(Debug, Clone, Serialize)]
pub struct RegisteredInstance {
    /// 实例 id（稳定 UUID）。
    pub id: String,
    /// 展示名。
    pub name: String,
    /// 绑定版本 id。
    pub version_id: String,
    /// 是否本次新建（`false` = 复用已有档案）。
    pub created: bool,
}
