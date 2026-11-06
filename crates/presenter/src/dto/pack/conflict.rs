//! 冲突的 DTO：成因、本次处置、以及三方哈希的那一条账。
//!
//! 三条判据与 core 的 `ConflictReason` 一一对应；`choice` 是**事后记录**（本次实际怎么处置的），
//! 不是「逐文件决策通道」——core 今天没有那种通道（`ConflictPolicy` 是整次导入一个全局档）。
//! 「默认保留用户那一侧」这条硬线在 `ImportRequestView::overwrite` 缺省 `false` 上。

use serde::{Deserialize, Serialize};

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
