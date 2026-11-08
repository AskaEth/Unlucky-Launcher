//! 按钮家族的**内部几何**：一个数值都不写，全部由 `unlucky_design` 的令牌推导。
//!
//! | 量 | 令牌 | 值（DESKTOP / 100% DPI） |
//! |---|---|---|
//! | 可见高 | `LayoutMetrics::control_height` | 40 `[源]` 12b 1.1 |
//! | 圆角 | `ShapeScale::Full.radius_px(高)` | 20 `[源]` 12b 1.2 |
//! | 左右内边距（无图标） | `Space::X6` | 24 / 24 `[源]` 12b 1.3 |
//! | 左内边距（带前置图标） | `Space::X4` | 16 `[源]` 12b 1.4 |
//! | 图标边长 | `IconSize::Button` | 18 `[源]` AAR `ButtonDefaults.IconSize = 18.dp`（12b 1.6 同值） |
//! | 图标↔文字 | `Space::X2` | 8 `[源]` 12b 1.6 |
//! | 描边 | `unlucky_design::stroke::HAIRLINE`（`crate::theme::STROKE_PX` 同一真源） | 1 `[源]` rubric G7 |
//! | IconButton 边长 | `LayoutMetrics::control_height` | 40 `[源]` 12b 2.1 |
//! | IconButton 图标 / 边距 | `IconSize::Toolbar` / `Space::X2` | 24 / 8 `[源]` 12b 2.2 |
//!
//! **没有落点的一个参考值**（登记在报告里，不在此处写数）：
//!
//! - 12b 1.7 最小宽 **58** → `G15`：iced 的 `Length` 没有 min-width 语义；按「24 + 24 内边距 +
//!   LabelLarge 二字」自然宽度 ≈ 76，实际不触发，故本 mission 不施加该约束。
//!
//! **G14 已收口（M86）**：这里曾登记「12b 1.6 图标 18 → 令牌只有 16，先用 16 并偏离 2px」。
//! 令牌层现在有 18 档（[`IconSize::Button`]，出处在 `crates/design/src/icons/mod.rs`），
//! 所以本文件取 18 就是**回到规范值**，不再是偏离。**别按 `ButtonSmallTokens.IconSize = 20` 改**：
//! 那个 token 在 AAR 里零消费（没有任何类读它的 `getIconSize`），是死令牌。

use iced::Padding;

use unlucky_design::icons::IconSize;
use unlucky_design::spacing::Space;

use crate::theme::{Ui, STROKE_PX};

use super::Variant;

/// 文字按钮的几何。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    /// 可见高（药丸圆角按它折算）。
    pub height: f32,
    /// 内边距（上下 0；左右按有无前置图标取 16 或 24）。
    pub padding: Padding,
    /// 图标盒边长。
    pub icon: f32,
    /// 图标 ↔ 文字间距。
    pub gap: f32,
    /// 描边宽度；`0` = 该式无描边。
    pub stroke: f32,
}

impl Geometry {
    /// 按变体 + 有没有前置图标求值（带图标时左内边距收到 16，右仍 24）。
    pub fn of(ui: &Ui, variant: Variant, leading_icon: bool) -> Self {
        let (_, _, border) = variant.roles();
        let start = if leading_icon {
            ui.space(Space::X4)
        } else {
            ui.space(Space::X6)
        };

        Self {
            height: ui.metrics().control_height,
            padding: Padding {
                top: 0.0,
                right: ui.space(Space::X6),
                bottom: 0.0,
                left: start,
            },
            icon: IconSize::Button.px(),
            gap: ui.space(Space::X2),
            stroke: if border.is_some() { STROKE_PX } else { 0.0 },
        }
    }
}

/// IconButton 的几何（正方形，图标 24、边距 8）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IconGeometry {
    /// 容器边长（40，命中区 48 由父级留白补足）。
    pub side: f32,
    /// 图标盒边长（24）——**必须**经 `layout::centered_fixed` 锁住，否则 Q9 会把字形撑满。
    pub icon: f32,
    /// 图标到容器边的默认边距（8；narrow 4 / wide 14 属变体参数，本 mission 不用）。
    pub padding: f32,
}

impl IconGeometry {
    /// 从令牌求值。图标边长**由 `side − 2×padding` 推导**（40 − 16 = 24），
    /// 与 `IconSize::Toolbar`（24）互为校验：两者不等说明令牌层动了，单测会先红。
    pub fn of(ui: &Ui) -> Self {
        let side = ui.metrics().control_height;
        let padding = ui.space(Space::X2);
        Self {
            side,
            icon: side - 2.0 * padding,
            padding,
        }
    }
}
