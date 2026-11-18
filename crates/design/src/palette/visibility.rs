//! # 容器层可视性裁决（债务台账「SecondaryContainer 对 Surface 仅 1.06」的令牌层结论）
//!
//! ## 问题
//!
//! m223 视觉复审实测：选中行底（`SecondaryContainer`）与它压着的卡片只差 **1.057:1**，
//! 大色块 + 色相偏移肉眼可辨，但**数不出层级**。逐个页面补色修不完（每个用选中底色的页面都要
//! 自己调一次，而且下一套色板又得来一遍），所以裁决必须落在令牌层。
//!
//! ## 裁决（三条，按顺序适用）
//!
//! 1. **文字对比**：所有 `on-*` 对承载色 ≥ 4.5:1 —— 既有 [`super::wcag`] 那道门，不重复定义。
//! 2. **状态容器对窗口底 ≥ [`STATE_CONTAINER_MIN`] = 1.20:1**：表达「状态」的容器
//!    （[`STATE_CONTAINERS`]：primary / secondary / tertiary / error 的 `*-container`）压在
//!    `surface` 上时必须数得出来。这是唯一一条**数字门禁**，[`SurfaceHierarchy::state_failures`]
//!    守它。
//! 3. **容器层之间的亮度差不设门禁，改为「组合指示」要求**：M3 的容器层天生贴在一起
//!    （官方口径浅色 `secondary-container` 对 `surface-container-high` 只有 1.06:1，
//!    本项目 Ink 板最低也只有 1.54:1 —— 这是配色结构的必然，不是哪套板的失误）。所以
//!    **禁止**把「容器底色差异」当作唯一的选中 / 层级指示：选中态必须是
//!    「容器底（过第 2 条）+ 文字对比（第 1 条）+ 形状 / 图标 / 描边指示」三件套。
//!    另一半同理：`surface-container-*` 只表达「几乎看不见的抬升」，
//!    拿它当选中态用连第 2 条都过不了。
//!
//! 第 3 条为什么不是「把阈值降到 1.02 就完事」：那样等于承认「选中态可以看不见」，
//! 而用户抱怨的正是这个。把不可测的那一半转成**构成要求**，才是能落地的那一半。
//!
//! ## 实测（`tests/ink_cat_visibility.rs` 守住第 2 条与阶梯）
//!
//! | 板 | 最弱状态容器 / `surface` | 状态容器 / 容器层（4×5 全矩阵，最低 ~ 最高） | 阶梯 L\\* 步长 |
//! |---|---|---|---|
//! | Ink | **1.97**（secondary-container） | 1.54 ~ **16.47** | +1.4 ~ +4.4 |
//! | Silver | **1.23**（tertiary/error-container） | **1.02** ~ 1.80 | −1.7 ~ −4.1 |
//! | 旧 baseline（债务现场） | 1.23 | **1.057**（对卡片，m223 实测） | — |
//!
//! 第三列就是第 3 条的由来：它过不了任何有意义的门禁（Silver 最差的一组是
//! `tertiary-container` 压 `surface-container-high`，1.016 —— 两者几乎同亮度），
//! 所以不设门禁、只设组合要求。
//!
//! ## 阶梯本身还是要查（[`SurfaceHierarchy::layer_violations`]）
//!
//! `surface-container-lowest` → `highest` 必须**单调**且相邻步长 ≥ [`LAYER_STEP_MIN_LSTAR`]
//! （1.0 个 L\\* 单位）。方向由深浅模式决定：深色板递增（越靠上越亮）、浅色板递减。
//! 这条抓的是「两档写成同一个色」「顺序填反」这类事故——不抓「差距够不够好看」。

use crate::color::{Role, Scheme, SchemeFlavor};

/// 状态容器对 `surface` 的最低对比度（唯一的**数字**门禁）。
pub const STATE_CONTAINER_MIN: f32 = 1.20;

/// 层级阶梯相邻两档的最小 L\\* 步长（低于这个数就分不出「哪层更高」）。
pub const LAYER_STEP_MIN_LSTAR: f32 = 1.0;

/// 表达**状态**的容器角色（必须过 [`STATE_CONTAINER_MIN`]）。
pub const STATE_CONTAINERS: [Role; 4] = [
    Role::PrimaryContainer,
    Role::SecondaryContainer,
    Role::TertiaryContainer,
    Role::ErrorContainer,
];

/// 表达**层级**的容器角色（由低到高；只查阶梯单调与步长）。
pub const LAYER_ROLES: [Role; 5] = [
    Role::SurfaceContainerLowest,
    Role::SurfaceContainerLow,
    Role::SurfaceContainer,
    Role::SurfaceContainerHigh,
    Role::SurfaceContainerHighest,
];

/// 阶梯上的一步：相对**前一档**的 L\\* 变化量（带符号；深色板应为正、浅色板为负）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerStep {
    /// 这一档是谁。
    pub role: Role,
    /// 相对前一档的 L\\* 差。
    pub delta_lstar: f32,
}

/// 一套方案的容器层可视性体检结果（纯数据，不改方案）。
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceHierarchy {
    /// 方案自己的深浅模式（决定阶梯方向）。
    pub flavor: SchemeFlavor,
    /// `surface` 的 L\\*（报告与对账用）。
    pub surface_lstar: f32,
    /// 每个状态容器对 `surface` 的实测对比度（顺序同 [`STATE_CONTAINERS`]）。
    pub state: [(Role, f32); 4],
    /// 层级阶梯的实测 L\\*（顺序同 [`LAYER_ROLES`]）。
    pub layers: [(Role, f32); 5],
}

impl SurfaceHierarchy {
    /// 状态容器里最低的一档（观察「刚好卡线」用）。
    pub fn worst_state(&self) -> (Role, f32) {
        self.state.iter().copied().fold(
            self.state[0],
            |acc, item| if item.1 < acc.1 { item } else { acc },
        )
    }

    /// 未达 [`STATE_CONTAINER_MIN`] 的状态容器。
    pub fn state_failures(&self) -> Vec<(Role, f32)> {
        self.state
            .iter()
            .copied()
            .filter(|(_, ratio)| *ratio < STATE_CONTAINER_MIN)
            .collect()
    }

    /// 阶梯的每一步（4 步：lowest→low→container→high→highest）。
    pub fn layer_steps(&self) -> [LayerStep; 4] {
        let mut out = [LayerStep {
            role: LAYER_ROLES[1],
            delta_lstar: 0.0,
        }; 4];
        for i in 1..self.layers.len() {
            out[i - 1] = LayerStep {
                role: self.layers[i].0,
                delta_lstar: self.layers[i].1 - self.layers[i - 1].1,
            };
        }
        out
    }

    /// 方向反了或步长不足的阶梯步（深色板要递增、浅色板要递减）。
    pub fn layer_violations(&self) -> Vec<LayerStep> {
        let sign = if self.flavor == SchemeFlavor::Dark {
            1.0
        } else {
            -1.0
        };
        self.layer_steps()
            .into_iter()
            .filter(|step| step.delta_lstar * sign < LAYER_STEP_MIN_LSTAR)
            .collect()
    }

    /// 第 2 条与阶梯都过。
    pub fn is_clean(&self) -> bool {
        self.state_failures().is_empty() && self.layer_violations().is_empty()
    }
}

/// 跑一遍容器层体检（纯函数）。
pub fn check_surface_hierarchy(scheme: &Scheme) -> SurfaceHierarchy {
    let surface = scheme.get(Role::Surface);
    SurfaceHierarchy {
        flavor: scheme.flavor(),
        surface_lstar: surface.lstar(),
        state: std::array::from_fn(|i| {
            let role = STATE_CONTAINERS[i];
            (role, scheme.contrast(role, Role::Surface))
        }),
        layers: std::array::from_fn(|i| {
            let role = LAYER_ROLES[i];
            (role, scheme.get(role).lstar())
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{Color, Scheme};

    #[test]
    fn dark_ladder_must_climb_and_light_ladder_must_fall() {
        // 造假：深色板把 highest 设成纯黑 → 阶梯反向，必须被抓住。
        let base = Scheme::baseline(SchemeFlavor::Dark);
        let mut colors = [Color::BLACK; 49];
        for (role, color) in base.iter() {
            colors[role as usize] = color;
        }
        colors[Role::SurfaceContainerHighest as usize] = Color::BLACK;
        let broken = Scheme::from_colors(SchemeFlavor::Dark, colors);
        let report = check_surface_hierarchy(&broken);
        assert!(
            report
                .layer_violations()
                .iter()
                .any(|step| step.role == Role::SurfaceContainerHighest),
            "{:?}",
            report.layer_steps()
        );
        assert!(!report.is_clean());
    }

    #[test]
    fn state_container_below_the_floor_is_reported() {
        // 造假：secondary-container 直接等于 surface（1:1）→ 必须报。
        let base = Scheme::baseline(SchemeFlavor::Light);
        let mut colors = [Color::BLACK; 49];
        for (role, color) in base.iter() {
            colors[role as usize] = color;
        }
        colors[Role::SecondaryContainer as usize] = base.get(Role::Surface);
        let broken = Scheme::from_colors(SchemeFlavor::Light, colors);
        let report = check_surface_hierarchy(&broken);
        assert_eq!(
            report.state_failures(),
            vec![(Role::SecondaryContainer, 1.0)]
        );
    }

    #[test]
    fn the_official_baseline_shows_why_the_debt_exists() {
        // 官方口径（tone 表）浅色板：状态容器对 surface 只有 ~1.23，对容器层只有 ~1.05。
        // 前半条过不了 1.20 的是「白银下界」，后半条根本不可门禁化——这就是裁决第 3 条。
        let report = check_surface_hierarchy(&Scheme::official_baseline(SchemeFlavor::Light));
        let (role, ratio) = report.worst_state();
        // 官方浅色的四个 container 都是 tone 90、surface 是 tone 98 ⇒ 四个值几乎相同，
        // 所以只断言量级（1.23 上下），不断言「哪一族最弱」。
        assert!(
            (1.15..1.30).contains(&ratio),
            "官方 baseline 的 {} 对 surface = {ratio:.3}，与文档的 1.23 不符",
            role.name()
        );
    }
}
