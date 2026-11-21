//! M3 语义色角色：`Role` 枚举是**唯一的名字清单**（49 个，M3 基础集全覆盖）。
//!
//! 后续 iced 页面不许另起名字（如 `bg-card` / `fg-primary`）——Fluent 时代的旧名到此为止，
//! 要表达「卡片底」就用 [`Role::SurfaceContainerLow`]，要表达「次要文字」就用
//! [`Role::OnSurfaceVariant`]（对应关系见 `DevDocs/SubAgentsTalk/design-令牌约定.md`）。

/// M3 语义色角色（49 个）。
///
/// 顺序即 [`Role::ALL`] 的顺序，也是 [`super::Scheme`] 内部数组的下标：
/// **只允许在末尾追加**，插到中间会静默改变已存盘的颜色映射。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    /// 主色：主操作填充、选中指示。
    Primary,
    /// 主色之上的前景（文字/图标）。
    OnPrimary,
    /// 主色容器：次级强调块、选中片。
    PrimaryContainer,
    /// 主色容器之上的前景。
    OnPrimaryContainer,
    /// 次色：低频强调、图表。
    Secondary,
    /// 次色之上的前景。
    OnSecondary,
    /// 次色容器。
    SecondaryContainer,
    /// 次色容器之上的前景。
    OnSecondaryContainer,
    /// 第三色：与主/次形成对比的装饰强调。
    Tertiary,
    /// 第三色之上的前景。
    OnTertiary,
    /// 第三色容器。
    TertiaryContainer,
    /// 第三色容器之上的前景。
    OnTertiaryContainer,
    /// 错误色：错误文字、危险操作填充。
    Error,
    /// 错误色之上的前景。
    OnError,
    /// 错误容器：错误提示底。
    ErrorContainer,
    /// 错误容器之上的前景。
    OnErrorContainer,
    /// 表面：窗口/内容底色（M3 最新口径里取代 background）。
    Surface,
    /// 表面之上的主体文字。
    OnSurface,
    /// 表面变体：中性分隔/次级容器（**不是**文字色）。
    SurfaceVariant,
    /// 表面变体之上的次级文字/图标。
    OnSurfaceVariant,
    /// 背景（M3 已并入 surface，保留以兼容旧值）。
    Background,
    /// 背景之上的文本（同上，保留兼容）。
    OnBackground,
    /// 变暗的表面（模态背后的压暗层）。
    SurfaceDim,
    /// 变亮的表面（深色下的抬高底）。
    SurfaceBright,
    /// 最低层容器（比 surface 更低）。
    SurfaceContainerLowest,
    /// 低层容器：卡片。
    SurfaceContainerLow,
    /// 标准容器：卡片/面板。
    SurfaceContainer,
    /// 高层容器：浮层。
    SurfaceContainerHigh,
    /// 最高层容器：对话框/菜单。
    SurfaceContainerHighest,
    /// 描边：输入框边框等（非文字，3:1 即可）。
    Outline,
    /// 弱描边：分隔线（纯装饰）。
    OutlineVariant,
    /// 反相表面：Snackbar / Tooltip 底。
    InverseSurface,
    /// 反相表面之上的文字。
    InverseOnSurface,
    /// 反相表面之上的强调。
    InversePrimary,
    /// 阴影色（通常带 alpha，见 [`super::Scheme::shadow`]）。
    Shadow,
    /// 遮罩色（模态背景，通常带 alpha）。
    Scrim,
    /// 表面着色（M3 旧规范的 elevation tint，现已被 surface container 取代）。
    SurfaceTint,
    /// 固定主色（深浅模式同值，用于需要跨模式稳定的块）。
    PrimaryFixed,
    /// 固定主色的暗档。
    PrimaryFixedDim,
    /// 固定主色之上的前景。
    OnPrimaryFixed,
    /// 固定主色暗档之上的前景。
    OnPrimaryFixedVariant,
    /// 固定次色。
    SecondaryFixed,
    /// 固定次色的暗档。
    SecondaryFixedDim,
    /// 固定次色之上的前景。
    OnSecondaryFixed,
    /// 固定次色暗档之上的前景。
    OnSecondaryFixedVariant,
    /// 固定第三色。
    TertiaryFixed,
    /// 固定第三色的暗档。
    TertiaryFixedDim,
    /// 固定第三色之上的前景。
    OnTertiaryFixed,
    /// 固定第三色暗档之上的前景。
    OnTertiaryFixedVariant,
}

impl Role {
    /// 全部 49 个角色，按 `Role as usize` 顺序排列。
    pub const ALL: [Role; 49] = [
        Role::Primary,
        Role::OnPrimary,
        Role::PrimaryContainer,
        Role::OnPrimaryContainer,
        Role::Secondary,
        Role::OnSecondary,
        Role::SecondaryContainer,
        Role::OnSecondaryContainer,
        Role::Tertiary,
        Role::OnTertiary,
        Role::TertiaryContainer,
        Role::OnTertiaryContainer,
        Role::Error,
        Role::OnError,
        Role::ErrorContainer,
        Role::OnErrorContainer,
        Role::Surface,
        Role::OnSurface,
        Role::SurfaceVariant,
        Role::OnSurfaceVariant,
        Role::Background,
        Role::OnBackground,
        Role::SurfaceDim,
        Role::SurfaceBright,
        Role::SurfaceContainerLowest,
        Role::SurfaceContainerLow,
        Role::SurfaceContainer,
        Role::SurfaceContainerHigh,
        Role::SurfaceContainerHighest,
        Role::Outline,
        Role::OutlineVariant,
        Role::InverseSurface,
        Role::InverseOnSurface,
        Role::InversePrimary,
        Role::Shadow,
        Role::Scrim,
        Role::SurfaceTint,
        Role::PrimaryFixed,
        Role::PrimaryFixedDim,
        Role::OnPrimaryFixed,
        Role::OnPrimaryFixedVariant,
        Role::SecondaryFixed,
        Role::SecondaryFixedDim,
        Role::OnSecondaryFixed,
        Role::OnSecondaryFixedVariant,
        Role::TertiaryFixed,
        Role::TertiaryFixedDim,
        Role::OnTertiaryFixed,
        Role::OnTertiaryFixedVariant,
    ];

    /// 稳定字符串名（kebab-case），用于日志、配置、快照对拍与文档表格。
    pub const fn name(self) -> &'static str {
        match self {
            Role::Primary => "primary",
            Role::OnPrimary => "on-primary",
            Role::PrimaryContainer => "primary-container",
            Role::OnPrimaryContainer => "on-primary-container",
            Role::Secondary => "secondary",
            Role::OnSecondary => "on-secondary",
            Role::SecondaryContainer => "secondary-container",
            Role::OnSecondaryContainer => "on-secondary-container",
            Role::Tertiary => "tertiary",
            Role::OnTertiary => "on-tertiary",
            Role::TertiaryContainer => "tertiary-container",
            Role::OnTertiaryContainer => "on-tertiary-container",
            Role::Error => "error",
            Role::OnError => "on-error",
            Role::ErrorContainer => "error-container",
            Role::OnErrorContainer => "on-error-container",
            Role::Surface => "surface",
            Role::OnSurface => "on-surface",
            Role::SurfaceVariant => "surface-variant",
            Role::OnSurfaceVariant => "on-surface-variant",
            Role::Background => "background",
            Role::OnBackground => "on-background",
            Role::SurfaceDim => "surface-dim",
            Role::SurfaceBright => "surface-bright",
            Role::SurfaceContainerLowest => "surface-container-lowest",
            Role::SurfaceContainerLow => "surface-container-low",
            Role::SurfaceContainer => "surface-container",
            Role::SurfaceContainerHigh => "surface-container-high",
            Role::SurfaceContainerHighest => "surface-container-highest",
            Role::Outline => "outline",
            Role::OutlineVariant => "outline-variant",
            Role::InverseSurface => "inverse-surface",
            Role::InverseOnSurface => "inverse-on-surface",
            Role::InversePrimary => "inverse-primary",
            Role::Shadow => "shadow",
            Role::Scrim => "scrim",
            Role::SurfaceTint => "surface-tint",
            Role::PrimaryFixed => "primary-fixed",
            Role::PrimaryFixedDim => "primary-fixed-dim",
            Role::OnPrimaryFixed => "on-primary-fixed",
            Role::OnPrimaryFixedVariant => "on-primary-fixed-variant",
            Role::SecondaryFixed => "secondary-fixed",
            Role::SecondaryFixedDim => "secondary-fixed-dim",
            Role::OnSecondaryFixed => "on-secondary-fixed",
            Role::OnSecondaryFixedVariant => "on-secondary-fixed-variant",
            Role::TertiaryFixed => "tertiary-fixed",
            Role::TertiaryFixedDim => "tertiary-fixed-dim",
            Role::OnTertiaryFixed => "on-tertiary-fixed",
            Role::OnTertiaryFixedVariant => "on-tertiary-fixed-variant",
        }
    }

    /// 该角色是否是「文字/图标前景色」（`on-*` 与 `inverse-on-surface` 一类）。
    ///
    /// 用于对比度门禁区分两类要求：前景对容器 ≥ 4.5:1（WCAG AA 正文），
    /// 非文字元素 ≥ 3:1。
    pub const fn is_foreground(self) -> bool {
        matches!(
            self,
            Role::OnPrimary
                | Role::OnPrimaryContainer
                | Role::OnSecondary
                | Role::OnSecondaryContainer
                | Role::OnTertiary
                | Role::OnTertiaryContainer
                | Role::OnError
                | Role::OnErrorContainer
                | Role::OnSurface
                | Role::OnSurfaceVariant
                | Role::OnBackground
                | Role::InverseOnSurface
                | Role::InversePrimary
                | Role::OnPrimaryFixed
                | Role::OnPrimaryFixedVariant
                | Role::OnSecondaryFixed
                | Role::OnSecondaryFixedVariant
                | Role::OnTertiaryFixed
                | Role::OnTertiaryFixedVariant
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_is_in_enum_order() {
        for (i, role) in Role::ALL.iter().enumerate() {
            assert_eq!(*role as usize, i, "Role::ALL 顺序必须等于枚举声明顺序");
        }
    }

    #[test]
    fn names_are_unique_and_kebab() {
        let mut seen = std::collections::BTreeSet::new();
        for role in Role::ALL {
            let name = role.name();
            assert!(seen.insert(name), "角色名重复：{name}");
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "角色名必须是 kebab-case：{name}"
            );
        }
    }
}
