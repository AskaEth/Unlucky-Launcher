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

    /// 阴影参数（**占位值，待 M63 视觉规格** — 见下面的 TODO）。
    ///
    /// TODO(M63): 取值待视觉规格。官方只给 dp 与设计稿示意，没有可移植的 blur/alpha 公式；
    /// 这里的 blur/offset/alpha 是临时占位（按「blur ≈ dp × 1.3 + 1.5」凑的数），
    /// M63 的玻璃规格表落地后以那张表为准，别在页面里引用这些绝对值。
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
                alpha: 0.12,
            },
            Elevation::Level2 => ShadowSpec {
                blur_px: 6.0,
                offset_y_px: 2.0,
                alpha: 0.14,
            },
            Elevation::Level3 => ShadowSpec {
                blur_px: 9.0,
                offset_y_px: 4.0,
                alpha: 0.16,
            },
            Elevation::Level4 => ShadowSpec {
                blur_px: 12.0,
                offset_y_px: 6.0,
                alpha: 0.18,
            },
            Elevation::Level5 => ShadowSpec {
                blur_px: 17.0,
                offset_y_px: 8.0,
                alpha: 0.20,
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
/// TODO(M63): 取值待视觉规格（这份结构只是「接口预留」，数值由 M63 的玻璃规格表填充）。
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

    #[test]
    fn shadow_grows_with_elevation() {
        let mut prev = Elevation::Level0.shadow();
        for level in &Elevation::ALL[1..] {
            let cur = level.shadow();
            assert!(cur.blur_px > prev.blur_px, "{level:?} 的模糊应更大");
            assert!(cur.offset_y_px >= prev.offset_y_px);
            assert!(cur.alpha >= prev.alpha);
            assert!(cur.is_visible());
            prev = cur;
        }
        assert!(!Elevation::Level0.shadow().is_visible());
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
