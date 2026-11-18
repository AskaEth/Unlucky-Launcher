//! CurseForge v1 的端点常量、枚举值与检索请求（**按站点原文建模**）。
//!
//! 站点文档里这些参数都是**数字枚举**（`classId=6`、`modLoaderType=4`、`sortField=6`），
//! 直接传数字会让调用点全是裸魔法值，且抄错一个数字不会编译失败、只会静默返回别的东西。
//! 所以每个枚举都带 `code()`，数字只在这一处出现。
//!
//! | 枚举 | 站点参数 | 取值 |
//! |------|----------|------|
//! | [`CfClass`] | `classId` | 6 Mods / 4471 Modpacks / 12 ResourcePacks / 6552 Shaders |
//! | [`CfLoader`] | `modLoaderType` / `latestFilesIndexes[].modLoader` | 1 Forge / 4 Fabric / 5 Quilt / 6 NeoForge（另有 2 Cauldron / 3 LiteLoader） |
//! | [`CfReleaseType`] | `releaseType` | 1 Release / 2 Beta / 3 Alpha |
//! | [`CfSort`] | `sortField` | 1 Featured / 2 Popularity / 3 LastUpdated / 6 TotalDownloads |
//!
//! `gameId` 只有一个值：Minecraft 是 432。

/// CurseForge 官方 API 的根（每一次请求都要 `x-api-key`）。
pub const CF_API: &str = "https://api.curseforge.com/v1";

/// CurseForge 里 "Minecraft" 的 `gameId`（官方固定值）。
pub const MINECRAFT_GAME_ID: u32 = 432;

/// 内容类型（= 站点的 `classId`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CfClass {
    /// Mod（6）
    Mods,
    /// 整合包（4471）
    Modpacks,
    /// 资源包（12）
    ResourcePacks,
    /// 光影（6552）
    Shaders,
}

impl CfClass {
    /// 站点的 `classId`。
    #[must_use]
    pub const fn code(self) -> u32 {
        match self {
            Self::Mods => 6,
            Self::Modpacks => 4471,
            Self::ResourcePacks => 12,
            Self::Shaders => 6552,
        }
    }

    /// 从 `classId` 反解（认不出返回 `None`；调用方决定当「未知」还是报错）。
    #[must_use]
    pub const fn parse(code: u32) -> Option<Self> {
        Some(match code {
            6 => Self::Mods,
            4471 => Self::Modpacks,
            12 => Self::ResourcePacks,
            6552 => Self::Shaders,
            _ => return None,
        })
    }
}

/// 加载器（= 站点的 `modLoaderType` 数字枚举）。
///
/// [`CfLoader::facet`] 给的是**与 Modrinth 同口径**的字符串（`fabric` / `forge` / `quilt` /
/// `neoforge`）：检索结果里的 `loaders` 字段要与另一条源对得上，页面才不用为两个站点写两套判断。
/// Cauldron / LiteLoader 在 Modrinth 那边没有同名 facet，`facet()` 给 `None`（**不**编一个名字）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CfLoader {
    /// Forge（1）
    Forge,
    /// Cauldron（2）
    Cauldron,
    /// LiteLoader（3）
    LiteLoader,
    /// Fabric（4）
    Fabric,
    /// Quilt（5）
    Quilt,
    /// NeoForge（6）
    NeoForge,
}

impl CfLoader {
    /// 站点的 `modLoaderType` 数字。
    #[must_use]
    pub const fn code(self) -> u32 {
        match self {
            Self::Forge => 1,
            Self::Cauldron => 2,
            Self::LiteLoader => 3,
            Self::Fabric => 4,
            Self::Quilt => 5,
            Self::NeoForge => 6,
        }
    }

    /// 从数字反解。
    #[must_use]
    pub const fn parse(code: u32) -> Option<Self> {
        Some(match code {
            1 => Self::Forge,
            2 => Self::Cauldron,
            3 => Self::LiteLoader,
            4 => Self::Fabric,
            5 => Self::Quilt,
            6 => Self::NeoForge,
            _ => return None,
        })
    }

    /// 与 Modrinth 同口径的 facet 名（Cauldron / LiteLoader 没有 ⇒ `None`）。
    #[must_use]
    pub const fn facet(self) -> Option<&'static str> {
        match self {
            Self::Forge => Some("forge"),
            Self::Fabric => Some("fabric"),
            Self::Quilt => Some("quilt"),
            Self::NeoForge => Some("neoforge"),
            Self::Cauldron | Self::LiteLoader => None,
        }
    }

    /// 从 facet 名（`fabric`）或站点在 `gameVersions` 里写的加载器名（`Forge`）反解，**忽略大小写**。
    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        match word.trim().to_ascii_lowercase().as_str() {
            "forge" => Some(Self::Forge),
            "cauldron" => Some(Self::Cauldron),
            "liteloader" => Some(Self::LiteLoader),
            "fabric" => Some(Self::Fabric),
            "quilt" => Some(Self::Quilt),
            "neoforge" => Some(Self::NeoForge),
            _ => None,
        }
    }
}

/// 版本类型（= 站点的 `releaseType`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CfReleaseType {
    /// 正式版（1）
    #[default]
    Release,
    /// 测试版（2）
    Beta,
    /// 早期预览（3）
    Alpha,
}

impl CfReleaseType {
    /// 站点数字。
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Release => 1,
            Self::Beta => 2,
            Self::Alpha => 3,
        }
    }

    /// 稳定字符串 id（与 Modrinth 的 `release` / `beta` / `alpha` 同口径）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Release => "release",
            Self::Beta => "beta",
            Self::Alpha => "alpha",
        }
    }

    /// 从站点数字反解；认不出按正式版（**不**拿它做安装判据）。
    #[must_use]
    pub const fn parse(code: u8) -> Self {
        match code {
            2 => Self::Beta,
            3 => Self::Alpha,
            _ => Self::Release,
        }
    }
}

/// 排序字段（= 站点的 `sortField`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CfSort {
    /// 站点推荐（1）
    #[default]
    Featured,
    /// 热度（2）
    Popularity,
    /// 最近更新（3）
    LastUpdated,
    /// 总下载量（6）
    TotalDownloads,
}

impl CfSort {
    /// 站点数字。
    #[must_use]
    pub const fn code(self) -> u32 {
        match self {
            Self::Featured => 1,
            Self::Popularity => 2,
            Self::LastUpdated => 3,
            Self::TotalDownloads => 6,
        }
    }
}

/// 一次 mod 检索（分页游标也在里面：换页就是换一份请求）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModQuery {
    /// 关键词（空 = 不带 `searchFilter`，即「按排序浏览」）
    pub text: String,
    /// 内容类型（`classId`）
    pub class: CfClass,
    /// 游戏版本过滤（精确值，如 `1.20.1`）
    pub game_version: Option<String>,
    /// 加载器过滤
    pub loader: Option<CfLoader>,
    /// 排序
    pub sort: CfSort,
    /// 偏移（站点从 0 起）
    pub index: u32,
    /// 每页条数（站点上限 50，构造时钳住）
    pub page_size: u32,
}

/// 站点接受的 `pageSize` 上限（超过会被站点拒或截断，钳住更省一次往返）。
pub const MAX_PAGE_SIZE: u32 = 50;

impl Default for ModQuery {
    fn default() -> Self {
        Self {
            text: String::new(),
            class: CfClass::Mods,
            game_version: None,
            loader: None,
            sort: CfSort::Featured,
            index: 0,
            page_size: 20,
        }
    }
}

impl ModQuery {
    /// 钳过界的分页参数 + 去掉首尾空白的关键词。
    #[must_use]
    pub fn normalized(&self) -> Self {
        let mut query = self.clone();
        query.text = query.text.trim().to_string();
        query.page_size = query.page_size.clamp(1, MAX_PAGE_SIZE);
        query
    }
}

/// 一次「某个 mod 的文件列表」请求（版本与加载器过滤都在这里）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModFileQuery {
    /// mod id（站点的数字 id，不是 slug）
    pub mod_id: u32,
    /// 游戏版本过滤（精确值）
    pub game_version: Option<String>,
    /// 加载器过滤
    pub loader: Option<CfLoader>,
    /// 偏移
    pub index: u32,
    /// 每页条数（上限同 [`MAX_PAGE_SIZE`]）
    pub page_size: u32,
}

impl ModFileQuery {
    /// 某个 mod 的全部文件（不带过滤，第一页）。
    #[must_use]
    pub const fn new(mod_id: u32) -> Self {
        Self {
            mod_id,
            game_version: None,
            loader: None,
            index: 0,
            page_size: MAX_PAGE_SIZE,
        }
    }

    /// 加游戏版本过滤。
    #[must_use]
    pub fn with_game_version(mut self, version: impl Into<String>) -> Self {
        self.game_version = Some(version.into());
        self
    }

    /// 加加载器过滤。
    #[must_use]
    pub const fn with_loader(mut self, loader: CfLoader) -> Self {
        self.loader = Some(loader);
        self
    }

    /// 钳过界的分页参数。
    #[must_use]
    pub fn normalized(&self) -> Self {
        let mut query = self.clone();
        query.page_size = query.page_size.clamp(1, MAX_PAGE_SIZE);
        query
    }
}
