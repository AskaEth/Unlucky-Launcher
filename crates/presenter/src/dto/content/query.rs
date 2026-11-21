//! 排序方式（M109 起在 `dto/content.rs`，M162 拆出来给内容源抽象让位）。
//!
//! 四种排序是**两个站点都有的公共面**，各自映射到自己的参数（Modrinth 的 `index`、
//! CurseForge 的 `sortField`）。映射函数放在各自的来源实现里，这里只有声明与稳定 slug。

use serde::{Deserialize, Serialize};

/// 排序方式（两个站点的官方排序项，口径对齐后再映射到各自参数）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContentSort {
    /// 相关度（Modrinth `relevance`；CF 没有对应项，退回热度）
    Relevance,
    /// 下载量
    Downloads,
    /// 最新发布
    Newest,
    /// 最近更新
    Updated,
}

impl ContentSort {
    /// 四种排序的声明顺序。
    pub const ALL: [ContentSort; 4] = [
        ContentSort::Relevance,
        ContentSort::Downloads,
        ContentSort::Newest,
        ContentSort::Updated,
    ];

    /// Modrinth `/v2/search` 的 `index` 参数值。
    #[must_use]
    pub const fn modrinth_index(self) -> &'static str {
        match self {
            Self::Relevance => "relevance",
            Self::Downloads => "downloads",
            Self::Newest => "newest",
            Self::Updated => "updated",
        }
    }

    /// CurseForge 的 `sortField`（1 = 热度，2 = 更新，3 = 发布，4 = 名称）。
    #[must_use]
    pub const fn curseforge_field(self) -> u32 {
        match self {
            Self::Relevance | Self::Downloads => 1,
            Self::Newest => 3,
            Self::Updated => 2,
        }
    }
}
