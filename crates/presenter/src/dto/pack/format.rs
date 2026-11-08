//! 整合包格式：探测结果与导出目标共用一个枚举。
//!
//! `slug()` 与 core 的格式 id **逐字一致**（`curseforge` / `modrinth` / `multimc`），
//! `from_slug` 是反向唯一入口——翻译发生在 `host::addon`，页面侧只认这两个函数。

use serde::{Deserialize, Serialize};

/// 整合包格式（探测结果与导出目标共用一个枚举）。
///
/// MultiMC / Prism 只**导入**不导出（core 的有意省略），所以导出侧两个入口是
/// [`Self::Modrinth`] / [`Self::Curseforge`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PackFormatView {
    /// CurseForge：`manifest.json` + `overrides/`。
    Curseforge,
    /// Modrinth：`.mrpack`（`modrinth.index.json` + `overrides/`）。
    Modrinth,
    /// MultiMC / Prism：`mmc-pack.json` + `.minecraft/`（**只读**）。
    Multimc,
}

impl PackFormatView {
    /// 导出菜单里可选的两个（顺序 = 画出来的顺序）。
    pub const EXPORTABLE: [PackFormatView; 2] =
        [PackFormatView::Modrinth, PackFormatView::Curseforge];

    /// 稳定 slug（与 core 的格式 id 逐字一致，**不要改名**）。
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Curseforge => "curseforge",
            Self::Modrinth => "modrinth",
            Self::Multimc => "multimc",
        }
    }

    /// 从 core 的格式 id 反解（`None` = 不认识的 id，调用方按「未知」处理，别猜）。
    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "curseforge" => Some(Self::Curseforge),
            "modrinth" => Some(Self::Modrinth),
            "multimc" => Some(Self::Multimc),
            _ => None,
        }
    }
}
