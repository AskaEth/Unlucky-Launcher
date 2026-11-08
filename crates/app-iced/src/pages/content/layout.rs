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

/// 主列在**矮窗口**下切成「整列一个滚动条」的门槛（M161）。
///
/// ## 为什么需要这条门槛
///
/// 主列自上而下是：搜索条（48）+ 筛选四排（4×32 + 3×8 = 152）+ 计数头（20）+ 结果列表 + 「加载更多」（40）
/// 再加两条 `Space::X2` 间距。列表自己没有最小高度，于是窗口一矮，**列表视口只剩几十像素**：
/// 一行 `ListItem` 是 60/76 高，被切成半行——两行文字（overline + 标题）看起来像「折行挤压」。
///
/// ## 明确的两档行为（不是到处加滚动条）
///
/// | 正文可用高 | 版面 |
/// |---|---|
/// | ≥ [`MIN_BODY_HEIGHT`] | 现状：**列表自己滚**（筛选条常驻，滚的时候不跑掉） |
/// | < [`MIN_BODY_HEIGHT`] | 主列整列变成**一个**纵向滚动区：列表按自然高度铺开，一行都不压 |
///
/// 阈值 = 固定件（48 + 152 + 20 + 40 + 间距 ≈ 276）＋ 至少三整行（3×60 = 180）= 456，
/// 取 **460**（往上抹到 4 的倍数，给 divider / 取整留余量）。数值改了就改这里的算式与
/// `layout/tests` 的等式用例。
pub const MIN_BODY_HEIGHT: f32 = 460.0;

/// 正文高度的两档行为。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyMode {
    /// 高够：列表自己滚（筛选条常驻）
    Split,
    /// 矮窗口：主列整列一个滚动区，列表按自然高度铺开
    ScrollWhole,
}

impl BodyMode {
    /// 判据（纯函数，测得到）。
    #[must_use]
    pub fn of(body_height: f32) -> Self {
        if body_height >= MIN_BODY_HEIGHT {
            Self::Split
        } else {
            Self::ScrollWhole
        }
    }

    /// 列表要不要自己滚。
    #[must_use]
    pub const fn list_scrolls(self) -> bool {
        matches!(self, Self::Split)
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

#[cfg(test)]
mod body_mode_tests {
    use super::*;

    /// 门槛是**算出来的**：固定件（搜索 48 + 筛选 152 + 计数 20 + 加载更多 40 + 两条 8 间距）
    /// 加三整行列表（3 × 60）。谁把 460 改成拍脑袋的数，这条会红。
    #[test]
    fn the_minimum_body_height_is_fixed_parts_plus_three_rows() {
        let fixed = 48.0 + 152.0 + 20.0 + 40.0 + 2.0 * Space::X2.dp();
        let three_rows = 3.0 * 60.0;
        assert!(
            MIN_BODY_HEIGHT >= fixed + three_rows,
            "门槛 {MIN_BODY_HEIGHT} 小于「固定件 {fixed} + 三行 {three_rows}」——矮窗口还是会切半行"
        );
        assert!(
            MIN_BODY_HEIGHT <= fixed + three_rows + 16.0,
            "门槛留了太多余量：{} 会把还放得下的窗口也推进「整列滚动」档",
            MIN_BODY_HEIGHT - (fixed + three_rows)
        );
    }

    /// 两档在门槛处切开，且只有 `Split` 让列表自己滚。
    #[test]
    fn the_body_mode_flips_at_the_threshold() {
        assert_eq!(BodyMode::of(MIN_BODY_HEIGHT), BodyMode::Split);
        assert_eq!(BodyMode::of(MIN_BODY_HEIGHT - 0.5), BodyMode::ScrollWhole);
        assert!(BodyMode::of(900.0).list_scrolls());
        assert!(!BodyMode::of(300.0).list_scrolls());
    }

    /// 最小窗口（880×560）落在**矮**档，验收到 760 不换版面——**正文可用高按真实外壳算**。
    ///
    /// 复审 P2-2：早先这里是 `560 − 56 − 88 − 32` 反推的（三个数全错：真实是 titlebar 48 /
    /// 两行页头 84 / Expanded 页边距 2×24 = chrome 180），结论碰巧对、但抓不住 metrics 漂移。
    /// 现在直接调 [`crate::pages::frame::body_height`]，chrome 从 token 现取。
    #[test]
    fn the_minimum_window_size_uses_the_scrolling_body() {
        use unlucky_design::color::SchemeFlavor;
        use unlucky_design::typography::Platform;

        use crate::pages::frame;
        use crate::pages::layout::{frame_metrics, PaneKit};
        use crate::theme::{Families, Ui};

        let ui = Ui::with_platform(
            SchemeFlavor::Light,
            Platform::Windows,
            Families {
                text: "Segoe UI",
                display: "Segoe UI",
                mono: "Consolas",
            },
        );
        // 880 宽 = 窗口最小尺寸（`app::MIN_SIZE.width`），属 Expanded。
        let metrics = frame_metrics(PaneKit::Single, 880.0);
        let chrome =
            ui.metrics().titlebar_height + ui.metrics().header_height(true) + 2.0 * metrics.gutter;

        // 真值：48 + 84 + 48 = 180。这条会随 token 改而红——那时请重看一眼两档版面。
        assert_eq!(chrome, 180.0, "外壳 chrome 与 design token 不一致");

        // 最小窗口 560 高：正文只剩 380 < 460 ⇒ 整列滚动（本条债的真实场景）。
        let short = frame::body_height(&ui, metrics, 560.0, true);
        assert_eq!(short, 560.0 - chrome);
        assert_eq!(
            BodyMode::of(short),
            BodyMode::ScrollWhole,
            "正文只剩 {short}"
        );

        // 验收尺寸 760 高：580 ≥ 460 ⇒ 列表自己滚（不换版面）。
        let tall = frame::body_height(&ui, metrics, 760.0, true);
        assert_eq!(BodyMode::of(tall), BodyMode::Split);

        // 把门槛和**真实 chrome** 绑死：窗口高 = chrome + MIN 时必须还是 Split，
        // 少 1px 就翻成整列滚动。chrome 涨到 100/页边距 40 这类漂移会让这条立刻红
        // （而不是像旧用例那样拿反推数字自说自话）。
        assert_eq!(
            BodyMode::of(frame::body_height(
                &ui,
                metrics,
                chrome + MIN_BODY_HEIGHT,
                true
            )),
            BodyMode::Split
        );
        assert_eq!(
            BodyMode::of(frame::body_height(
                &ui,
                metrics,
                chrome + MIN_BODY_HEIGHT - 1.0,
                true
            )),
            BodyMode::ScrollWhole
        );
    }
}
