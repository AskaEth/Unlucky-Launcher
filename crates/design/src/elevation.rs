//! 高程令牌：elevation level、阴影参数、surface container 映射。
//!
//! M3 在 2024 之后改了「怎么表达高程」：**不再**往表面上叠半透明的 surface tint，
//! 而是用 `surface-container-*` 这一族中性容器色表达层级（GPU 路径再叠阴影做空间感）。
//! 本模块两种口径都给出，旧口径显式标为 deprecated，避免页面线混用。

use crate::color::Role;

/// M3 高程级别 0..5（对应 0 / 1 / 3 / 6 / 8 / 12 dp）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Elevation {
    /// 0dp：贴底内容（窗口底、内联区）。
    Level0,
    /// 1dp：卡片、可抬起的列表项。
    Level1,
    /// 3dp：可交互卡片、导航栏。
    Level2,
    /// 6dp：FAB、侧栏、浮动工具条。
    Level3,
    /// 8dp：菜单、popover。
    Level4,
    /// 12dp：对话框、模态。
    Level5,
}

impl Elevation {
    /// 全部级别。
    pub const ALL: [Elevation; 6] = [
        Elevation::Level0,
        Elevation::Level1,
        Elevation::Level2,
        Elevation::Level3,
        Elevation::Level4,
        Elevation::Level5,
    ];

    /// 标称高度（dp）。
    pub const fn dp(self) -> f32 {
        match self {
            Elevation::Level0 => 0.0,
            Elevation::Level1 => 1.0,
            Elevation::Level2 => 3.0,
            Elevation::Level3 => 6.0,
            Elevation::Level4 => 8.0,
            Elevation::Level5 => 12.0,
        }
    }

    /// 该级别应使用的表面容器色角色（M3 现行口径）。
    ///
    /// 层级靠**换底色**表达，不靠 tint；浅深模式都成立。
    pub const fn surface_role(self) -> Role {
        match self {
            Elevation::Level0 => Role::SurfaceContainerLowest,
            Elevation::Level1 => Role::SurfaceContainerLow,
            Elevation::Level2 => Role::SurfaceContainer,
            Elevation::Level3 => Role::SurfaceContainerHigh,
            Elevation::Level4 | Elevation::Level5 => Role::SurfaceContainerHighest,
        }
    }

    /// 阴影参数：一层高斯阴影，数值=玻璃视觉规格表的六档。
    ///
    /// **数值来源**：`DevDocs/PlanDoc/research/11-m3-glass-visual-spec.md` §5 那张表
    /// （「给 `Elevation::shadow()` 填值」，档位 → `offset_y` / `blur` / `alpha`）。
    /// 本函数逐档照抄，**不在这里另调**——页面里也不要绕过令牌自己写这几个数。
    ///
    /// **三条使用约束**（同文档 §5 与 `11b-m3-glass-params.md` §2 的 R4/R5）：
    ///
    /// - **硬顶** `blur ≤ 24` / `alpha ≤ 0.28` / `offset_y ≤ 8`。表里最大的一档就是
    ///   [`Elevation::Level5`] 的 `8 / 24 / 0.28`，没有额外余量可透支。
    /// - **每个元素只画一层**。阴影是单层的 [`ShadowSpec`]，不是可叠加的图层栈；
    ///   想要更「深」就升档，别把两层阴影摞起来。
    /// - **阴影只给浮层**（菜单 / 对话框 / popover 这类真浮起来的东西）。
    ///   卡片与内容区靠 [`Elevation::surface_role`] 的底色差表达层级，不靠阴影；
    ///   一屏带阴影的元素越多，越接近「全屏都在飘」。
    ///
    /// [`Elevation::Level0`] 三项全零 = 这一档不画阴影（规格表里对应「—」）。
    pub const fn shadow(self) -> ShadowSpec {
        match self {
            Elevation::Level0 => ShadowSpec {
                blur_px: 0.0,
                offset_y_px: 0.0,
                alpha: 0.0,
            },
            Elevation::Level1 => ShadowSpec {
                blur_px: 3.0,
                offset_y_px: 1.0,
                alpha: 0.14,
            },
            Elevation::Level2 => ShadowSpec {
                blur_px: 6.0,
                offset_y_px: 2.0,
                alpha: 0.16,
            },
            Elevation::Level3 => ShadowSpec {
                blur_px: 12.0,
                offset_y_px: 4.0,
                alpha: 0.20,
            },
            Elevation::Level4 => ShadowSpec {
                blur_px: 18.0,
                offset_y_px: 6.0,
                alpha: 0.24,
            },
            Elevation::Level5 => ShadowSpec {
                blur_px: 24.0,
                offset_y_px: 8.0,
                alpha: 0.28,
            },
        }
    }

    /// 旧口径（M3 2022）：把 `surface-tint` 按这个 alpha 叠在 `surface` 上表达高程。
    ///
    /// **deprecated**——现行 M3 用 [`Elevation::surface_role`]；保留只为读旧稿时能对上数，
    /// 新代码不要用。
    pub const fn deprecated_tint_alpha(self) -> f32 {
        match self {
            Elevation::Level0 => 0.0,
            Elevation::Level1 => 0.05,
            Elevation::Level2 => 0.08,
            Elevation::Level3 => 0.11,
            Elevation::Level4 => 0.12,
            Elevation::Level5 => 0.14,
        }
    }
}

/// 一层阴影的绘制参数。
///
/// 数值由 [`Elevation::shadow`] 按玻璃视觉规格表给出，本结构不含取值逻辑。
/// 「一层」是规格的硬约束（`11b-m3-glass-params.md` R4/R5）：每个元素只画一层阴影，
/// 深浅靠升档而不是叠层，所以这里只有一组 blur / offset / alpha，没有层的列表。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowSpec {
    /// 高斯模糊半径（px）。
    pub blur_px: f32,
    /// 向下偏移（px）。
    pub offset_y_px: f32,
    /// 阴影色 alpha（叠在 [`crate::color::Scheme::shadow`] 之上）。
    pub alpha: f32,
}

impl ShadowSpec {
    /// 是否可见（0 表示这一级不画阴影）。
    pub fn is_visible(&self) -> bool {
        self.alpha > 0.0 && self.blur_px > 0.0
    }
}

/// 三个「材质层」的语义（沿用 `ui-integration-contract.md` §2 的三层，映射到 M3 令牌）。
///
/// 页面线按层取色，不许自己决定「这一层比那一层深一点」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialLayer {
    /// 底层（canvas）：窗口底，永远不透明。
    Canvas,
    /// 卡片层：页面内容/设置分组/AI 面板。
    Card,
    /// 浮层：对话框、菜单、Toast。
    Overlay,
}

impl MaterialLayer {
    /// 该层的底色角色。
    pub const fn surface_role(self) -> Role {
        match self {
            MaterialLayer::Canvas => Role::Surface,
            MaterialLayer::Card => Role::SurfaceContainerLow,
            MaterialLayer::Overlay => Role::SurfaceContainerHigh,
        }
    }

    /// 该层的高程级别。
    pub const fn elevation(self) -> Elevation {
        match self {
            MaterialLayer::Canvas => Elevation::Level0,
            MaterialLayer::Card => Elevation::Level1,
            MaterialLayer::Overlay => Elevation::Level3,
        }
    }

    /// 该层是否允许半透明（D27：**只有 GPU 路径**才允许；CPU 路径一律纯色）。
    pub const fn allows_translucency(self) -> bool {
        matches!(self, MaterialLayer::Card | MaterialLayer::Overlay)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elevation_dp_ladder() {
        let ladder: Vec<f32> = Elevation::ALL.iter().map(|e| e.dp()).collect();
        assert_eq!(ladder, vec![0.0, 1.0, 3.0, 6.0, 8.0, 12.0]);
    }

    /// 硬顶（`11-m3-glass-visual-spec.md` §5 / `11b` R5）：表里最大的一档就是 L5，没有余量。
    const MAX_BLUR: f32 = 24.0;
    const MAX_ALPHA: f32 = 0.28;
    const MAX_OFFSET_Y: f32 = 8.0;

    /// 六档逐个等值断言：照抄规格表，任何一档被改动都在这里变红。
    ///
    /// 期望值刻意写成**字面量**而不是 `Elevation::LevelN.shadow()`——这条用例的职责就是
    /// 把令牌钉到规格表上，自引用会变成永远为真的同义反复。
    #[test]
    fn shadow_ladder_matches_the_visual_spec_table() {
        // (档, offset_y, blur, alpha)：顺序即规格表列序。
        let table = [
            (Elevation::Level0, 0.0, 0.0, 0.0),
            (Elevation::Level1, 1.0, 3.0, 0.14),
            (Elevation::Level2, 2.0, 6.0, 0.16),
            (Elevation::Level3, 4.0, 12.0, 0.20),
            (Elevation::Level4, 6.0, 18.0, 0.24),
            (Elevation::Level5, 8.0, 24.0, 0.28),
        ];
        for (level, offset_y, blur, alpha) in table {
            let spec = level.shadow();
            assert_eq!(
                spec.offset_y_px, offset_y,
                "{level:?} 的 offset_y 不等于规格表"
            );
            assert_eq!(spec.blur_px, blur, "{level:?} 的 blur 不等于规格表");
            assert_eq!(spec.alpha, alpha, "{level:?} 的 alpha 不等于规格表");
        }
    }

    /// 硬顶断言：任何一档都不得越过 `blur ≤ 24` / `alpha ≤ 0.28` / `offset_y ≤ 8`。
    #[test]
    fn shadow_stays_within_the_hard_caps() {
        for level in Elevation::ALL {
            let spec = level.shadow();
            assert!(spec.blur_px <= MAX_BLUR, "{level:?} 的 blur 超过硬顶");
            assert!(spec.alpha <= MAX_ALPHA, "{level:?} 的 alpha 超过硬顶");
            assert!(
                spec.offset_y_px <= MAX_OFFSET_Y,
                "{level:?} 的 offset_y 超过硬顶"
            );
        }
    }

    /// 单调不降：三档参数随高程不减小（L0 → L5 逐对比较，含 L0→L1 这一对）。
    #[test]
    fn shadow_never_decreases_with_elevation() {
        let mut prev = Elevation::Level0.shadow();
        for level in &Elevation::ALL[1..] {
            let cur = level.shadow();
            assert!(cur.blur_px >= prev.blur_px, "{level:?} 的 blur 比低一档小");
            assert!(
                cur.offset_y_px >= prev.offset_y_px,
                "{level:?} 的 offset_y 比低一档小"
            );
            assert!(cur.alpha >= prev.alpha, "{level:?} 的 alpha 比低一档小");
            prev = cur;
        }
    }

    /// L0 就是「不画阴影」：三项全零且 `is_visible()` 为假（规格表里的「—」）。
    #[test]
    fn level_zero_draws_no_shadow() {
        let spec = Elevation::Level0.shadow();
        assert_eq!(
            spec,
            ShadowSpec {
                blur_px: 0.0,
                offset_y_px: 0.0,
                alpha: 0.0,
            }
        );
        assert!(!spec.is_visible());
        // 反证：其余五档必须可见，否则「L0 不画」这条会被「全都不画」蒙过去。
        for level in &Elevation::ALL[1..] {
            assert!(level.shadow().is_visible(), "{level:?} 应当有可见阴影");
        }
    }

    #[test]
    fn higher_levels_use_higher_surface_containers() {
        assert_eq!(
            Elevation::Level5.surface_role(),
            Role::SurfaceContainerHighest
        );
        assert_eq!(
            Elevation::Level0.surface_role(),
            Role::SurfaceContainerLowest
        );
    }

    #[test]
    fn material_layers_are_ordered() {
        assert_eq!(MaterialLayer::Canvas.surface_role(), Role::Surface);
        assert_eq!(
            MaterialLayer::Card.surface_role(),
            Role::SurfaceContainerLow
        );
        assert_eq!(
            MaterialLayer::Overlay.surface_role(),
            Role::SurfaceContainerHigh
        );
        assert!(!MaterialLayer::Canvas.allows_translucency());
        assert!(MaterialLayer::Overlay.allows_translucency());
    }
}
