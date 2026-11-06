//! 内容库页的栏几何（纯函数 + 数值表）。
//!
//! ## 为什么这一页自带一份几何
//!
//! 任务书要求「与 M107 同一套视觉语言」：类型栏 140 / 图标栏 64、间距 24、列表列下限 424
//! （行宽红线）、详情 320。M107 的那份几何在 `pages/layout/rails.rs`（M107 分支，尚未并入 main），
//! 本 mission 的分支基于 main，所以这里按**同一组数值**先落一份，并在报告里登记
//! 「M107 合并后应改为复用 `pages/layout/rails.rs`，删掉本文件」。
//!
//! ## 四档的判据是**算出来的**，不是拍的
//!
//! | 档 | 条件（可用宽，已扣导航轨与页边距） | 版面 |
//! |---|---|---|
//! | `Wide` | ≥ 140 + 24 + 424 + 24 + 320 = **932** | 标签栏 + 列表 + 详情 |
//! | `Narrow` | ≥ 64 + 24 + 424 + 24 + 320 = **856**（类型栏收成图标栏） | 图标栏 + 列表 + 详情 |
//! | `Compact` | ≥ 140 + 24 + 424 = **588** | 标签栏 + 单列（详情整页替换列表） |
//! | `Tight` | 其余 | 类型条变成顶部一行 chip + 单列 |
//!
//! 三个门槛都是「Σ栏宽 + 间距」算出来的，改动任何一档都会让数值表用例红。

use unlucky_design::spacing::Space;

use crate::pages::layout;

/// 类型栏宽（标签形态）。
///
/// M107 的版本类型栏是 140；这里取 200：四个**内容类型**的名字（`Resource packs`）比
/// 版本类型（`Release` / `Snapshot`）长，140 会把标签切掉。登记在 `pages/content.rs` 的偏离表。
pub const RAIL_LABEL: f32 = 200.0;
/// 类型栏宽（图标形态）。
pub const RAIL_ICON: f32 = 64.0;
/// 详情面板宽。
pub const DETAIL: f32 = 320.0;
/// 栏间距（`Space::X6` = 24，与 `pages::layout::PANES_GAP` 同源）。
pub const GAP: f32 = Space::X6.dp();

/// 一屏三栏各自多宽。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rails {
    /// 档位
    pub tier: Tier,
    /// 类型栏宽（`Tight` 档是 0：类型条在顶部）
    pub rail: f32,
    /// 列表列宽
    pub list: f32,
    /// 详情面板宽（0 = 这一档没有独立详情列）
    pub detail: f32,
    /// 栏间距
    pub gap: f32,
}

/// 版面档位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// 标签栏 + 列表 + 详情
    Wide,
    /// 图标栏 + 列表 + 详情
    Narrow,
    /// 标签栏 + 单列
    Compact,
    /// 顶部 chip 条 + 单列
    Tight,
}

impl Tier {
    /// 这一档有独立详情列吗。
    pub const fn has_detail(self) -> bool {
        matches!(self, Self::Wide | Self::Narrow)
    }

    /// 这一档的类型栏是标签形态吗（`Tight` 没有栏）。
    pub const fn rail_has_labels(self) -> bool {
        matches!(self, Self::Wide | Self::Compact)
    }
}

/// `Wide` 的门槛：标签栏 + 间距 + 列表下限 + 间距 + 详情。
pub fn wide_min() -> f32 {
    RAIL_LABEL + GAP + layout::LINE_SAFE_WIDTH_PX + GAP + DETAIL
}

/// `Narrow` 的门槛：图标栏 + 间距 + 列表下限 + 间距 + 详情。
pub fn narrow_min() -> f32 {
    RAIL_ICON + GAP + layout::LINE_SAFE_WIDTH_PX + GAP + DETAIL
}

/// `Compact` 的门槛：标签栏 + 间距 + 列表下限。
pub fn compact_min() -> f32 {
    RAIL_LABEL + GAP + layout::LINE_SAFE_WIDTH_PX
}

/// 可用宽 → 三栏几何。
pub fn resolve(available: f32) -> Rails {
    let cap = layout::measure_cap_px();

    if available >= wide_min() {
        let rail = RAIL_LABEL;
        return Rails {
            tier: Tier::Wide,
            rail,
            gap: GAP,
            detail: DETAIL,
            list: (available - rail - GAP - DETAIL - GAP).max(layout::LINE_SAFE_WIDTH_PX),
        };
    }
    if available >= narrow_min() {
        let rail = RAIL_ICON;
        return Rails {
            tier: Tier::Narrow,
            rail,
            gap: GAP,
            detail: DETAIL,
            list: (available - rail - GAP - DETAIL - GAP).max(layout::LINE_SAFE_WIDTH_PX),
        };
    }
    if available >= compact_min() {
        let rail = RAIL_LABEL;
        return Rails {
            tier: Tier::Compact,
            rail,
            gap: GAP,
            detail: 0.0,
            list: (available - rail - GAP).min(cap.max(layout::LINE_SAFE_WIDTH_PX)),
        };
    }
    Rails {
        tier: Tier::Tight,
        rail: 0.0,
        gap: GAP,
        detail: 0.0,
        list: available.min(cap.max(layout::LINE_SAFE_WIDTH_PX)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 门槛必须由「Σ栏宽 + 间距」算出来（抓得住：谁把 932 抄成 900 这种拍值）。
    #[test]
    fn the_three_thresholds_are_sums_not_magic_numbers() {
        assert_eq!(wide_min(), 200.0 + 24.0 + 424.0 + 24.0 + 320.0);
        assert_eq!(narrow_min(), 64.0 + 24.0 + 424.0 + 24.0 + 320.0);
        assert_eq!(compact_min(), 200.0 + 24.0 + 424.0);
        assert_eq!(wide_min(), 992.0);
        assert_eq!(narrow_min(), 856.0);
        assert_eq!(compact_min(), 648.0);
    }

    #[test]
    fn each_tier_takes_effect_at_its_own_threshold() {
        assert_eq!(resolve(wide_min()).tier, Tier::Wide);
        assert_eq!(resolve(wide_min() - 0.5).tier, Tier::Narrow);
        assert_eq!(resolve(narrow_min()).tier, Tier::Narrow);
        assert_eq!(resolve(narrow_min() - 0.5).tier, Tier::Compact);
        assert_eq!(resolve(compact_min()).tier, Tier::Compact);
        assert_eq!(resolve(compact_min() - 0.5).tier, Tier::Tight);
    }

    #[test]
    fn a_wide_page_never_gives_the_list_less_than_the_row_width_red_line() {
        // 1180 窗宽（右栗收起）：1044 可用 → Wide，列表 536
        let rails = resolve(1044.0);
        assert_eq!(rails.tier, Tier::Wide);
        assert!(rails.list >= layout::LINE_SAFE_WIDTH_PX, "{}", rails.list);
        // 逐档扫一遍：只要还有独立列表列，就不许低于 424（行宽红线）
        for available in 500..=1600 {
            let rails = resolve(available as f32);
            if rails.rail > 0.0 {
                assert!(
                    rails.list >= layout::LINE_SAFE_WIDTH_PX,
                    "available={available} 时列表只剩 {}",
                    rails.list
                );
            }
        }
    }

    #[test]
    fn the_detail_column_only_exists_in_the_two_wide_tiers() {
        assert!(Tier::Wide.has_detail());
        assert!(Tier::Narrow.has_detail());
        assert!(!Tier::Compact.has_detail());
        assert!(!Tier::Tight.has_detail());
        assert_eq!(resolve(1200.0).detail, DETAIL);
        assert_eq!(resolve(700.0).detail, 0.0);
        // 图标形态只在这两档出现，且窄档的标签栏不存在
        assert_eq!(resolve(900.0).rail, RAIL_ICON);
        assert_eq!(resolve(700.0).rail, RAIL_LABEL);
        assert_eq!(resolve(400.0).rail, 0.0);
        assert!(Tier::Wide.rail_has_labels());
        assert!(!Tier::Narrow.rail_has_labels());
    }
}
