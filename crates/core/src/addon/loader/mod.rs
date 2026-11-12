//! 加载器安装（Fabric / Quilt / NeoForge / Forge / LiteLoader）：官方元数据 → loader 版本 JSON → 补库清单。
//!
//! 设计输入：`DevDocs/PlanDoc/08-mod-and-recommend.md` §1（D10 加载器安装）、
//! `DevDocs/analyse/05-modloader-modpack.md` §1（五加载器安装协议、`patch + priority` 主线）。
//!
//! # 与 M27 安装层的关系（不复制粘贴，只叠一层）
//!
//! 加载器不是"另一个版本"，而是**叠在已装原版之上的一个 `inheritsFrom` 层**：
//!
//! ```text
//! versions/1.20.1/1.20.1.json              原版（M27 负责装）
//! versions/1.20.1-fabric-0.15.11/…json    loader 层：inheritsFrom=1.20.1 + 主类 + loader 库
//! ```
//!
//! 因此本模块**只做两件事**：①把官方元数据翻译成一份 loader 版本 JSON；
//! ②把它的库列表交给 [`crate::game::install::libraries`] 展开成下载计划。
//! 落盘、跳过判定、实例登记一律复用 M27（[`crate::game::install::execute`] / `register`），
//! 不另起一套下载/写盘逻辑。
//!
//! # 三条路线（按"能不能不跑 java 就装出来"分类，而不是按加载器名字）
//!
//! | 路线 | 加载器 | 做法 |
//! |------|--------|------|
//! | [`LoaderSupport::JsonRoute`] | Fabric / Quilt | 官方 meta 直接给 launcher 用的 profile JSON |
//! | [`LoaderSupport::JsonRoute`] | LiteLoader | 官方版本表 → **从零合成**清单（见 [`liteloader`]） |
//! | [`LoaderSupport::JsonRoute`] | Forge spec-0（如 1.12.2 重建版） | installer 里的 `version.json` + 库清单，universal jar 走 maven 直取 |
//! | [`LoaderSupport::RequiresInstaller`] | Forge spec≥1 / NeoForge | installer 声明了 client 侧 processors（拆包/映射），**必须外调 java** |
//!
//! 第四类本期只出**明确 unsupported**：版本 JSON 与库清单仍照常生成（`--dry-run` 可看），
//! 但真装会被拒——半成品实例比明确报错更糟（沿 M27 §7「如实告知」的纪律）。
//! 老协议 installer（内联 `versionInfo` + universal jar 解包，≈1.12.2 之前的重建版）同样归 unsupported。
//!
//! OptiFine 仍**认得出但装不了**（[`LoaderKind::from_core`] 返回 `None`）：它要跑外部
//! `optifine.Patcher` 并原地改 zip，不在本期（见 `DevDocs/artifacts/m218/SURVEY-p1.md` §3）。

pub mod fabric;
pub mod forge;
pub mod liteloader;
pub mod maven;
pub mod meta;
pub mod parent;
pub mod plan;
pub mod processors;

#[cfg(test)]
mod tests;

pub use meta::LoaderSource;
pub use plan::{LoaderInstallPlan, LoaderPlanInput, LoaderRelease, LoaderSupport};

use std::fmt;

use crate::download::{DownloadError, DownloadErrorKind};

/// 支持的加载器种类。
///
/// `vanilla` / `none` **不在这里**：那是"不装加载器"，由 CLI 走 M27 的原版路径。
/// OptiFine 也**不在**这里：它要跑外部 `optifine.Patcher` + 原地改 zip，本启动器仍只识别不安装。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LoaderKind {
    /// Fabric（`meta.fabricmc.net`，纯 meta，无 installer）。
    Fabric,
    /// Quilt（`meta.quiltmc.org`，与 Fabric 同构）。
    Quilt,
    /// NeoForge（`maven.neoforged.net`，installer 内嵌 install profile）。
    NeoForge,
    /// Forge（`maven.minecraftforge.net`，两代 installer 协议）。
    Forge,
    /// LiteLoader（`dl.liteloader.com`，官方版本表 + 从零合成 launchwrapper 清单，无 installer）。
    LiteLoader,
}

impl LoaderKind {
    /// 解析命令行 / 整合包清单里的加载器名（大小写不敏感）。
    ///
    /// 七个 id 的匹配**委派**给 [`crate::game::LoaderKind::parse`]（词表的源头，M126），
    /// 本方法只多收一个**输入别名** `neo-forge`：那是 Modrinth 的拼法（mrpack
    /// `dependencies` 的键、CF 包里也有这种写法），**只进不出**——[`Self::as_str`] 绝不产出
    /// 它。识别得出但本启动器装不了的（原版 / OptiFine）返回 `None`。
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        let wanted = raw.trim();
        if wanted.eq_ignore_ascii_case("neo-forge") {
            return Some(Self::NeoForge);
        }
        Self::from_core(crate::game::LoaderKind::parse(wanted)?)
    }

    /// core 的宽种类 → 本模块能装的五家（`None` = 识别得出但本启动器装不了）。
    ///
    /// 穷尽 match：core 加一支就编不过，逼作者决定「它能不能装」。
    const fn from_core(kind: crate::game::LoaderKind) -> Option<Self> {
        match kind {
            crate::game::LoaderKind::Fabric => Some(Self::Fabric),
            crate::game::LoaderKind::Quilt => Some(Self::Quilt),
            crate::game::LoaderKind::NeoForge => Some(Self::NeoForge),
            crate::game::LoaderKind::Forge => Some(Self::Forge),
            crate::game::LoaderKind::LiteLoader => Some(Self::LiteLoader),
            crate::game::LoaderKind::Vanilla | crate::game::LoaderKind::OptiFine => None,
        }
    }

    /// 稳定字符串 id（进版本 id、JSON 输出与日志）。
    ///
    /// 词表**不在本模块**：[`crate::game::LoaderKind::as_str`] 是「种类 → 对外稳定 id」的源头
    /// （M126）。这里只做**结构映射**（一个 id 字面量都没有）——安装侧拼出来的
    /// `<mc>-<kind>-<loader>` 必须能被 [`crate::game::InstanceLoader::parse`] 认回来，委派让
    /// 「两边各写一份、改一边忘了另一边」这件事物理上做不到。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fabric => crate::game::LoaderKind::Fabric.as_str(),
            Self::Quilt => crate::game::LoaderKind::Quilt.as_str(),
            Self::NeoForge => crate::game::LoaderKind::NeoForge.as_str(),
            Self::Forge => crate::game::LoaderKind::Forge.as_str(),
            Self::LiteLoader => crate::game::LoaderKind::LiteLoader.as_str(),
        }
    }

    /// 人读名称。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Fabric => "Fabric",
            Self::Quilt => "Quilt",
            Self::NeoForge => "NeoForge",
            Self::Forge => "Forge",
            Self::LiteLoader => "LiteLoader",
        }
    }

    /// 是否属于 Fabric 系（纯 meta、profile JSON 直接可用）。
    #[must_use]
    pub const fn is_fabric_family(self) -> bool {
        matches!(self, Self::Fabric | Self::Quilt)
    }

    /// 是否走 maven 族（installer jar + install profile）。
    ///
    /// 第三个数据源（LiteLoader）既不是 fabric 系也不是 maven 族，所以这条路**必须显式问**，
    /// 不能再用「不是 fabric 就是 maven」的二分——那会把 LiteLoader 静默送进 Forge 的 URL 拼装。
    #[must_use]
    pub const fn is_maven_family(self) -> bool {
        matches!(self, Self::NeoForge | Self::Forge)
    }

    /// 全部加载器（CLI 的 `--loader` 枚举提示用）。
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::Fabric,
            Self::Quilt,
            Self::NeoForge,
            Self::Forge,
            Self::LiteLoader,
        ]
    }

    /// loader 版本 id 的词根：`<mc>-<kind>-<loader>`（`versions/<id>/` 与实例 `versionId` 都用它）。
    #[must_use]
    pub fn version_id(self, mc_version: &str, loader_version: &str) -> String {
        format!("{mc_version}-{}-{loader_version}", self.as_str())
    }
}

impl fmt::Display for LoaderKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

/// 加载器安装失败的大类（CLI 据此给稳定错误码 `loader_*`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoaderErrorKind {
    /// 该加载器/该版本走不通我们的路线（需要跑 installer 或属未支持的老协议）。
    Unsupported,
    /// 官方元数据里没有这个加载器版本（或没有匹配该原版版本的版本）。
    NotFound,
    /// 元数据解析失败（profile JSON / install profile / maven metadata 坏了）。
    Metadata,
    /// 网络侧失败（所有候选源都失败 / 超时）。
    Network,
    /// 本地 IO（读 installer 缓存、写盘失败）。
    Io,
}

impl LoaderErrorKind {
    /// 稳定字符串 id（进 `--json` 的 `error.code`）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unsupported => "loader_unsupported",
            Self::NotFound => "loader_not_found",
            Self::Metadata => "loader_metadata",
            Self::Network => "loader_network",
            Self::Io => "loader_io",
        }
    }
}

/// 加载器安装错误：大类 + 人读说明 + 可选提示。
#[derive(Debug, Clone)]
pub struct LoaderError {
    kind: LoaderErrorKind,
    message: String,
    hint: Option<String>,
}

impl LoaderError {
    /// 构造。
    #[must_use]
    pub fn new(kind: LoaderErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            hint: None,
        }
    }

    /// 走不通我们的路线（需要外调 java installer / 老协议）。
    #[must_use]
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(LoaderErrorKind::Unsupported, message)
    }

    /// 元数据里查不到。
    #[must_use]
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(LoaderErrorKind::NotFound, message)
    }

    /// 元数据坏了/形状不符。
    #[must_use]
    pub fn metadata(message: impl Into<String>) -> Self {
        Self::new(LoaderErrorKind::Metadata, message)
    }

    /// 本地 IO。
    #[must_use]
    pub fn io(message: impl Into<String>) -> Self {
        Self::new(LoaderErrorKind::Io, message)
    }

    /// 补一句提示。
    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// 大类。
    #[must_use]
    pub const fn kind(&self) -> LoaderErrorKind {
        self.kind
    }

    /// 人读说明。
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// 下一步提示。
    #[must_use]
    pub fn hint(&self) -> Option<&str> {
        self.hint.as_deref()
    }
}

impl fmt::Display for LoaderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for LoaderError {}

impl From<DownloadError> for LoaderError {
    fn from(error: DownloadError) -> Self {
        let kind = match error.kind {
            DownloadErrorKind::Io => LoaderErrorKind::Io,
            DownloadErrorKind::InvalidRequest => LoaderErrorKind::Metadata,
            DownloadErrorKind::Integrity
            | DownloadErrorKind::NotFound
            | DownloadErrorKind::ServerError
            | DownloadErrorKind::Timeout
            | DownloadErrorKind::Network
            | DownloadErrorKind::RangeUnsupported
            | DownloadErrorKind::AllSourcesFailed => LoaderErrorKind::Network,
        };
        let mut message = error.message.clone();
        if let Some(source) = &error.source {
            message.push_str(&format!("（源 {source}）"));
        }
        Self::new(kind, message)
    }
}
