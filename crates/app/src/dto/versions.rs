//! 下载页 DTO：版本列表与分类（含远古版子类，D16/Q24-4）。

use serde::{Deserialize, Serialize};

/// 版本类型（远古版按 unlisted-versions 的分类拆开）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VersionKind {
    /// 正式版
    Release,
    /// 预览版（snapshot）
    Snapshot,
    /// 远古版（带子类）
    Ancient(AncientKind),
}

/// 远古版子类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AncientKind {
    /// classic（rd-*）
    Classic,
    /// old_alpha（a1.*）
    OldAlpha,
    /// old_beta（b1.*）
    OldBeta,
    /// 未列入官方清单的远古版汇总
    Unlisted,
}

impl VersionKind {
    /// 是否属于「远古版」——列表行要加警示徽标。
    pub fn is_ancient(self) -> bool {
        matches!(self, Self::Ancient(_))
    }
}

/// 下载页版本行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionSummary {
    /// 版本 id（如 `1.20.4` / `rd-132211`）
    pub id: String,
    /// 版本类型
    pub kind: VersionKind,
    /// 发布日期（ISO-8601 日期部分）
    pub released: String,
    /// 本地是否已安装（行尾显示「已装」）
    pub installed: bool,
}

/// 下载页左侧分类（页内常驻侧栏的选中项）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VersionCategory {
    /// 正式版
    Release,
    /// 预览版
    Snapshot,
    /// 远古版某子类
    Ancient(AncientKind),
    /// 整合包（P1）
    Modpack,
    /// Mod（P1）
    Mod,
}
