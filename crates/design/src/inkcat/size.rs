//! 控件**尺寸档**：按钮 XS~XL 五档 + FAB 四态（规格卡 `BUTTONS / XS - XL` 与
//! `FAB / SMALL - DEFAULT - MEDIUM - EXTENDED`）。
//!
//! 为什么要有这一层：`LayoutMetrics::control_height`（40）只够表达「标准控件多高」，
//! 而规格卡把按钮做成了**五档梯子**（32/40/56/96/136），FAB 又自成一套四态。这些是「档位」，
//! 不是骨架尺寸（不随窗口变），也不是单个控件的内部微几何（`ComponentMetrics` 管的是画进
//! 方框里的那几个像素）。所以单独一层，一个枚举一个档，消费方按语义取。
//!
//! ## 取值与实测（规格卡 `rx` 全部等于半高 ⇒ 真药丸）
//!
//! | 档 | 高 | 圆角 | 字号 | 规格卡依据 |
//! |---|---|---|---|---|
//! | XS | 32 | 16 | 14 | `40.24×32 rx16`，标签 `font-size=14` |
//! | S | 40 | 20 | 14 | `40.12×40 rx20` |
//! | M | 56 | 28 | 16 | `57.28×56 rx28`（五档行同一档） |
//! | L | 96 | 48 | 24 | `109.92×96 rx48` |
//! | XL | 136 | 68 | 32 | `165.12×136 rx68` |
//!
//! 圆角一律走 [`ShapeScale::Full`]（`radius_px(高)` = 半高），**不写裸数字**：规格卡的 rx
//! 恰好都是半高，`Full` 与之一致，还顺带保证「换档不会漏改圆角」。
//!
//! 字号是规格卡 measured 值；字重 500（Expressive 的按钮文字就是 Medium）。
//! **规格卡没给水平内距**（按钮是自适应宽），[`ControlSize::pad_x`] 是按「半高向下取 4 栅格、
//! 封顶 48」推的 `[估]` 值——药丸两端要留出半圆安全区，这条规则比逐档拍数字更经得起改档。

use crate::shape::ShapeScale;
use crate::typography::{TypeRole, TypeStyle, Weight};

/// 按钮尺寸档（规格卡 `XS - XL`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControlSize {
    /// 32 高：行内小按钮、徽标式操作。
    Xs,
    /// 40 高：默认密集按钮（= `LayoutMetrics::control_height`）。
    S,
    /// 56 高：主操作按钮（规格卡 `BUTTONS / 5 LEVELS` 那一行就是这一档）。
    M,
    /// 96 高：页面级主行动（空态、引导页）。
    L,
    /// 136 高：展示型入口（规格卡给的最大档，慎用）。
    Xl,
}

impl ControlSize {
    /// 由小到大全部档位。
    pub const ALL: [ControlSize; 5] = [
        ControlSize::Xs,
        ControlSize::S,
        ControlSize::M,
        ControlSize::L,
        ControlSize::Xl,
    ];

    /// 稳定字符串名（`xs` … `xl`）。
    pub const fn name(self) -> &'static str {
        match self {
            ControlSize::Xs => "xs",
            ControlSize::S => "s",
            ControlSize::M => "m",
            ControlSize::L => "l",
            ControlSize::Xl => "xl",
        }
    }

    /// 可见高度（px）。
    pub const fn height_px(self) -> f32 {
        match self {
            ControlSize::Xs => 32.0,
            ControlSize::S => 40.0,
            ControlSize::M => 56.0,
            ControlSize::L => 96.0,
            ControlSize::Xl => 136.0,
        }
    }

    /// 圆角档：全部药丸（规格卡每档的 rx 都等于半高）。
    pub const fn shape(self) -> ShapeScale {
        ShapeScale::Full
    }

    /// 实际圆角（= 半高，规格卡 rx 实测值 16/20/28/48/68）。
    pub fn radius_px(self) -> f32 {
        self.shape().radius_px(self.height_px())
    }

    /// 标签字号（规格卡 measured：14/14/16/24/32）。
    pub const fn label_px(self) -> f32 {
        match self {
            ControlSize::Xs | ControlSize::S => 14.0,
            ControlSize::M => 16.0,
            ControlSize::L => 24.0,
            ControlSize::Xl => 32.0,
        }
    }

    /// 标签该用的官方字级（`[spec]` 字号对应的最近档；同一字号多档时取 `Role::ALL` 序里靠前的那个）。
    pub const fn type_role(self) -> TypeRole {
        match self {
            ControlSize::Xs | ControlSize::S => TypeRole::LabelLarge,
            ControlSize::M => TypeRole::BodyLarge,
            ControlSize::L => TypeRole::HeadlineSmall,
            ControlSize::Xl => TypeRole::HeadlineLarge,
        }
    }

    /// 标签样式：字号/行高跟字级、**字重固定 500**（规格卡按钮文字都是 `font-weight=500`，
    /// 而官方 body/headline 是 400 —— 这是按钮语义，不是排版漂移）。
    pub fn label(self) -> TypeStyle {
        TypeStyle {
            weight: Weight::Medium,
            ..self.type_role().style()
        }
    }

    /// 水平内距（px）。`[估]`：规格卡是自适应宽、没写内距。
    ///
    /// 规则 = 半高向下取 4 栅格，封顶 48（再宽只会白吃宽度，观感不再变）。
    pub const fn pad_x(self) -> f32 {
        match self {
            ControlSize::Xs => 16.0,
            ControlSize::S => 20.0,
            ControlSize::M => 28.0,
            ControlSize::L => 48.0,
            ControlSize::Xl => 48.0,
        }
    }

    /// 该档是否达到无障碍命中下限（M3：可点目标 ≥48dp）。XS/S 需要外扩命中区。
    pub const fn meets_touch_target(self) -> bool {
        self.height_px() >= 48.0
    }
}

/// FAB 四态（规格卡 `FAB / SMALL - DEFAULT - MEDIUM - EXTENDED`）。
///
/// 规格卡的圆角是 12/16/20/16 —— **不是**半高（40 的半高是 20，规格卡是 12），所以这里
/// 逐个写档位（[`ShapeScale::Medium`] / [`ShapeScale::Large`] / [`ShapeScale::LargeIncreased`]），
/// 不用 `Full`。扩展态（Extended）是「药丸 + 图标 + 标签」，几何见 [`FabSize::Extended`] 的各访问器。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FabSize {
    /// 40×40，圆角 12，图标 20。
    Small,
    /// 56×56，圆角 16，图标 26（= M3 默认 FAB 尺寸）。
    Default,
    /// 80×80，圆角 20，图标 34。
    Medium,
    /// 160×56（最小宽，随标签撑开），圆角 16，图标 24，标签 15。
    Extended,
}

impl FabSize {
    /// 全部四态。
    pub const ALL: [FabSize; 4] = [
        FabSize::Small,
        FabSize::Default,
        FabSize::Medium,
        FabSize::Extended,
    ];

    /// 稳定字符串名（`small` / `default` / `medium` / `extended`）。
    pub const fn name(self) -> &'static str {
        match self {
            FabSize::Small => "small",
            FabSize::Default => "default",
            FabSize::Medium => "medium",
            FabSize::Extended => "extended",
        }
    }

    /// 宽度（px；Extended 是**最小**宽，实际由标签撑开）。
    pub const fn width_px(self) -> f32 {
        match self {
            FabSize::Small => 40.0,
            FabSize::Default => 56.0,
            FabSize::Medium => 80.0,
            FabSize::Extended => 160.0,
        }
    }

    /// 高度（px）。
    pub const fn height_px(self) -> f32 {
        match self {
            FabSize::Small => 40.0,
            FabSize::Default => 56.0,
            FabSize::Medium => 80.0,
            FabSize::Extended => 56.0,
        }
    }

    /// 圆角档（规格卡 rx：12 / 16 / 20 / 16）。
    pub const fn shape(self) -> ShapeScale {
        match self {
            FabSize::Small => ShapeScale::Medium,
            FabSize::Default | FabSize::Extended => ShapeScale::Large,
            FabSize::Medium => ShapeScale::LargeIncreased,
        }
    }

    /// 实际圆角（px）。
    pub fn radius_px(self) -> f32 {
        self.shape().radius_px(self.height_px())
    }

    /// 图标（`+` 字形）字号（规格卡 measured：20/26/34/24）。
    pub const fn icon_px(self) -> f32 {
        match self {
            FabSize::Small => 20.0,
            FabSize::Default => 26.0,
            FabSize::Medium => 34.0,
            FabSize::Extended => 24.0,
        }
    }

    /// 标签字号（px；只有 Extended 有标签）。
    pub const fn label_px(self) -> f32 {
        match self {
            FabSize::Extended => 15.0,
            _ => 0.0,
        }
    }

    /// 起始内距（px；只有 Extended 有）：**28**，规格卡画的就这个数。
    ///
    /// 推导（可逐位核）：框 `x=292` 起、宽 160；`+` 是 `font-size=24`、`text-anchor=middle`、
    /// 中心 `x=332` ⇒ 图标盒 = 320..344 ⇒ 内距 = 320 − 292 = **28**；标签 `x=352` 起，
    /// 于是图标盒右沿 344 到标签 352 = [`FabSize::gap_px`] 的 8。
    ///
    /// ⚠️ 与官方口径的偏离已登记：M3 ExtendedFAB 的起始内距是 **16**（图标盒 292+16=308）。
    /// 规格卡画得更宽松，本表按**规格卡**取值（A 级依据 = 图中坐标，不是猜的）；M229 若要用
    /// M3 的 16，得同时把图标中心从 332 挪回 316 —— 那是一处视觉改动，要出截图，别悄悄改数。
    pub const fn pad_start_px(self) -> f32 {
        match self {
            FabSize::Extended => 28.0,
            _ => 0.0,
        }
    }

    /// 图标与标签的间距（px；Extended 实测 8）。
    pub const fn gap_px(self) -> f32 {
        match self {
            FabSize::Extended => 8.0,
            _ => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spacing::GRID;

    #[test]
    fn button_ladder_is_pill_and_monotone() {
        for pair in ControlSize::ALL.windows(2) {
            assert!(
                pair[0].height_px() < pair[1].height_px(),
                "{:?} 应矮于 {:?}",
                pair[0],
                pair[1]
            );
            assert!(pair[0].label_px() <= pair[1].label_px());
            assert!(pair[0].pad_x() <= pair[1].pad_x());
        }
        for size in ControlSize::ALL {
            // 规格卡实测：rx = 半高（16/20/28/48/68）。
            assert_eq!(size.radius_px(), size.height_px() / 2.0, "{size:?}");
            assert_eq!(size.shape(), ShapeScale::Full);
            assert_eq!(size.height_px() % GRID, 0.0, "{size:?} 不在 4px 栅格上");
            assert_eq!(size.pad_x() % GRID, 0.0, "{size:?} 的内距不在 4px 栅格上");
            // 标签字号与所取字级一致（字重才是被覆盖的那一项）。
            assert_eq!(size.label().size_px, size.type_role().style().size_px);
            assert_eq!(size.label().weight, Weight::Medium);
        }
        // 命中下限：只有 L / XL 自身就够 48。
        assert!(!ControlSize::Xs.meets_touch_target());
        assert!(!ControlSize::S.meets_touch_target());
        assert!(ControlSize::M.meets_touch_target());
    }

    #[test]
    fn fab_corners_are_the_spec_values() {
        // 规格卡 rx：40→12、56→16、80→20、扩展 56→16（都不是半高，所以不许用 Full）。
        for (size, radius) in [
            (FabSize::Small, 12.0),
            (FabSize::Default, 16.0),
            (FabSize::Medium, 20.0),
            (FabSize::Extended, 16.0),
        ] {
            assert_eq!(size.radius_px(), radius, "{size:?}");
            assert_ne!(size.shape(), ShapeScale::Full, "{size:?} 不是药丸");
        }
        // 方态三档是正方形；扩展态是药丸。
        for size in [FabSize::Small, FabSize::Default, FabSize::Medium] {
            assert_eq!(size.width_px(), size.height_px(), "{size:?} 该是正方形");
            assert_eq!(size.label_px(), 0.0);
            assert_eq!(size.pad_start_px(), 0.0);
        }
        assert_eq!(FabSize::Extended.width_px(), 160.0);
        assert_eq!(FabSize::Extended.height_px(), 56.0);
        assert_eq!(FabSize::Extended.label_px(), 15.0);
        // 内距 28 = 规格卡图中「框 292 → 图标盒 320」的实测距离（不是 M3 的 16）。
        assert_eq!(FabSize::Extended.pad_start_px(), 28.0);
        // 图标中心落点必须自洽：292 + 28 + 24/2 = 332，与规格卡 `x=332` 对得上。
        assert_eq!(FabSize::Extended.pad_start_px() + 24.0 / 2.0 + 292.0, 332.0);
        assert_eq!(FabSize::Extended.gap_px(), 8.0);
        // 图标字号随档递增（扩展态回到 24，因为它跟标签同一行）。
        assert!(FabSize::Small.icon_px() < FabSize::Default.icon_px());
        assert!(FabSize::Default.icon_px() < FabSize::Medium.icon_px());
        assert_eq!(FabSize::Extended.icon_px(), 24.0);
    }

    #[test]
    fn names_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for size in ControlSize::ALL {
            assert!(seen.insert(size.name()));
        }
        let mut seen = std::collections::BTreeSet::new();
        for size in FabSize::ALL {
            assert!(seen.insert(size.name()));
        }
    }
}
