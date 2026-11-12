//! 冲突的 DTO：成因、本次处置、**预检的待决清单**、以及三方哈希的那一条账。
//!
//! 三种东西分得清清楚楚（混起来就会把"要不要覆盖"变成"已经覆盖了"）：
//!
//! | 类型 | 什么时候出现 | 语义 |
//! |---|---|---|
//! | [`ImportConflictView`] | **导入前**（`PackEvent::Scan`） | 待用户点头的一条：路径、来源、目标现有体积与哈希 |
//! | [`ConflictFileView`] | **导入后**（`PackEvent::Report`） | 事后账：这条实际怎么处置的（`choice`） |
//! | [`ConflictDecisionsView`] | 从页面发回去（`ImportRequestView::decisions`） | 用户点的「全部保留 / 全部覆盖 / 逐项」 |
//!
//! 「默认保留用户那一侧」这条硬线在 [`ConflictDecisionsView::default`] 上：缺省 = 全部保留，
//! 而 core 侧 [`ConflictChoiceView::KeepUser`] 也是缺省档——两侧都不许"没收到决策就覆盖"。

use serde::{Deserialize, Serialize};

/// 一条冲突**来源**（预检清单里那一列）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictSourceView {
    /// 包内 `overrides/`（CF 清单点名的那个目录 / MultiMC 的 `.minecraft/` 也走这条）。
    Overrides,
    /// 包内 `client-overrides/`（叠层里胜出的那一侧）。
    ClientOverrides,
    /// CurseForge `files[]` 条目。
    CurseForge,
    /// Modrinth `files[]` 条目。
    Modrinth,
}

impl ConflictSourceView {
    /// 稳定 slug。
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Overrides => "overrides",
            Self::ClientOverrides => "client_overrides",
            Self::CurseForge => "curseforge",
            Self::Modrinth => "modrinth",
        }
    }

    /// 从 core 的 id 反解。
    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "overrides" => Some(Self::Overrides),
            "client_overrides" => Some(Self::ClientOverrides),
            "curseforge" => Some(Self::CurseForge),
            "modrinth" => Some(Self::Modrinth),
            _ => None,
        }
    }

    /// 是不是"包内的覆盖目录"（相对于清单里的远端条目）。
    #[must_use]
    pub const fn is_override_dir(self) -> bool {
        matches!(self, Self::Overrides | Self::ClientOverrides)
    }
}

/// 一条**待用户点头**的冲突（导入前那张清单里的一行）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportConflictView {
    /// 实例内相对路径。
    pub path: String,
    /// 来源（包内覆盖目录 / 清单远端条目）。
    pub source: ConflictSourceView,
    /// 成因。
    pub reason: ConflictReasonView,
    /// 包内那条是不是禁用态（`.disabled`）。
    pub disabled: bool,
    /// 包内声明的 SHA-1。
    pub pack_sha1: String,
    /// 包内声明的字节数。
    pub pack_size: u64,
    /// 目标现有文件的 SHA-1。
    pub disk_sha1: String,
    /// 目标现有文件的字节数（覆盖会丢掉的量）。
    pub disk_size: u64,
    /// 上次导入写下的 SHA-1（没有基线时 `None`）。
    pub base_sha1: Option<String>,
}

/// 用户的冲突决策（三条入口共用一个类型）。
///
/// 三条入口与 core 的 `ConflictDecisions`（`default` + `entries`）是同一件事的两种说法：
///
/// | 变体 | core 形状 |
/// |---|---|
/// | [`ConflictDecisionsView::KeepAll`] | `default = KeepUser`, 空表 |
/// | [`ConflictDecisionsView::OverwriteAll`] | `default = Overwrite`, 空表 |
/// | `PerItem { default, items }` | `default` + 逐项表 |
///
/// `PerItem` 带一个 `default` 是为了让"全部覆盖之后我又单独放回两条"这种真实操作可表达：
/// 若只有"逐项覆盖 + 缺省保留"，用户就得把剩下的几百条全点一遍。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictDecisionsView {
    /// **全部保留**（缺省）：用户已有文件一个都不动。
    KeepAll,
    /// **全部覆盖**：整合包说了算。
    OverwriteAll,
    /// **逐项**：点过的路径按 `items`，其余按 `default`。
    PerItem {
        /// 没点过的路径按它处置。
        default: ConflictChoiceView,
        /// 点过的路径。
        items: Vec<ConflictDecisionView>,
    },
}

impl Default for ConflictDecisionsView {
    /// **缺省 = 全部保留**（绝不默认覆盖用户已有文件）。
    fn default() -> Self {
        Self::KeepAll
    }
}

impl ConflictDecisionsView {
    /// 逐项构造（缺省档 + 点过的那些）。
    #[must_use]
    pub fn per_item(default: ConflictChoiceView, items: Vec<ConflictDecisionView>) -> Self {
        if items.is_empty() {
            return match default {
                ConflictChoiceView::KeepUser => Self::KeepAll,
                ConflictChoiceView::Overwrite => Self::OverwriteAll,
            };
        }
        Self::PerItem { default, items }
    }

    /// 逐项表里的缺省档（`KeepAll` / `OverwriteAll` 也答得出）。
    #[must_use]
    pub const fn default_choice(&self) -> ConflictChoiceView {
        match self {
            Self::KeepAll => ConflictChoiceView::KeepUser,
            Self::OverwriteAll => ConflictChoiceView::Overwrite,
            Self::PerItem { default, .. } => *default,
        }
    }

    /// 用户显式要求"以包为准"吗（整批覆盖）。
    #[must_use]
    pub fn overwrites_everything(&self) -> bool {
        matches!(self, Self::OverwriteAll)
    }

    /// 逐项表里"覆盖"的条数（不含缺省档）。
    #[must_use]
    pub fn overwrite_count(&self) -> usize {
        self.items()
            .iter()
            .filter(|item| item.choice == ConflictChoiceView::Overwrite)
            .count()
    }

    /// 逐项表里"保留"的条数。
    #[must_use]
    pub fn keep_count(&self) -> usize {
        self.items()
            .iter()
            .filter(|item| item.choice == ConflictChoiceView::KeepUser)
            .count()
    }

    /// 逐项表（`KeepAll` / `OverwriteAll` 是空表）。
    #[must_use]
    pub fn items(&self) -> &[ConflictDecisionView] {
        match self {
            Self::PerItem { items, .. } => items,
            Self::KeepAll | Self::OverwriteAll => &[],
        }
    }

    /// 逐项表的条数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.items().len()
    }

    /// 有没有逐项决策。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items().is_empty()
    }
}

/// 逐项决策的一条（路径 → 处置）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictDecisionView {
    /// 实例内相对路径。
    pub path: String,
    /// 处置。
    pub choice: ConflictChoiceView,
}

/// 一次导入预检的结果（`PackEvent::Scan` 的载荷）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportScanView {
    /// 目标实例目录（人要看到"装到哪"）。
    pub target: std::path::PathBuf,
    /// 命中的格式。
    pub format: Option<crate::dto::PackFormatView>,
    /// 包名。
    pub pack_name: Option<String>,
    /// 包版本。
    pub pack_version: Option<String>,
    /// 游戏版本。
    pub game_version: Option<String>,
    /// 待用户点头的冲突（按路径排序）。
    pub conflicts: Vec<ImportConflictView>,
    /// 补全前没有路径、因而判不了的远端条目数。
    pub unknown_remote: usize,
    /// 覆盖会丢掉的字节总量。
    pub lost_bytes: u64,
    /// 包内那些条目声明了多大。
    pub incoming_bytes: u64,
    /// 注记（与导入报告同源的人读串）。
    pub notes: Vec<String>,
    /// 预检本身失败（探测不过 / 开包失败）时的原因。
    pub error: Option<crate::dto::ImportErrorView>,
}

impl ImportScanView {
    /// 需要用户点头吗（`false` = 可以直接导入，别打扰）。
    #[must_use]
    pub fn needs_decision(&self) -> bool {
        !self.conflicts.is_empty()
    }

    /// 一条冲突都没有、预检也成功（可以直接导）。
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.error.is_none() && self.conflicts.is_empty()
    }
}

/// 冲突成因（core 的三条判据）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictReasonView {
    /// 基线里记过，磁盘内容与基线不同 ⇒ 用户改过。
    UserModified,
    /// 基线里记过，磁盘上却没了 ⇒ 用户删过。
    UserDeleted,
    /// 没有基线可判，磁盘上本来就有同名文件。
    PreExisting,
}

impl ConflictReasonView {
    /// 从 core 的 id 反解。
    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "user_modified" => Some(Self::UserModified),
            "user_deleted" => Some(Self::UserDeleted),
            "pre_existing" => Some(Self::PreExisting),
            _ => None,
        }
    }
}

/// 冲突本次实际采用的处置。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictChoiceView {
    /// 保留用户侧（**默认**）。
    KeepUser,
    /// 用包内内容覆盖（用户显式要求「整合包说了算」）。
    Overwrite,
}

impl ConflictChoiceView {
    /// 从 core 的 id 反解。
    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "keep_user" => Some(Self::KeepUser),
            "overwrite" => Some(Self::Overwrite),
            _ => None,
        }
    }
}

/// 一条冲突（**事后账**：本次实际怎么处置的）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictFileView {
    /// 实例内相对路径。
    pub path: String,
    /// 成因。
    pub reason: ConflictReasonView,
    /// 本次实际采用的处置。
    pub choice: ConflictChoiceView,
    /// 包内声明的 SHA-1。
    pub pack_sha1: String,
    /// 上次导入写下的 SHA-1（没有基线时 `None`）。
    pub base_sha1: Option<String>,
    /// 磁盘现状的 SHA-1（文件不存在时 `None`）。
    pub disk_sha1: Option<String>,
    /// 包内声明的字节数。
    pub pack_size: u64,
}
