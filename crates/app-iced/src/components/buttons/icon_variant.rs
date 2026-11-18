//! IconButton 的四式（规格卡 `ICON BUTTONS / FILLED - TONAL - OUTLINED - PLAIN`）。
//!
//! M234 纯搬移：本文件原来只是 `buttons/mod.rs` 的第 166~225 行（`mod.rs` 已顶到 399 行，
//! 加 `.base(..)` 与 `icon_on` 之前必须先腾地方）。搬移**零行为改动**，`IconVariant` 的文档、
//! 角色表、A1 出处一字未改；对外路径由 `mod.rs` 的 `pub use` 保持为 `buttons::IconVariant`。

use unlucky_design::color::Role;

/// IconButton 的**四式**（规格卡 `ICON BUTTONS / FILLED - TONAL - OUTLINED - PLAIN`）。
///
/// 规格卡画了四颗：实心 / 色调 / 描边 / 纯图标。12b §2.5 只有前三式（Standard / Filled / Tonal），
/// M229 按规格卡把 Standard 拆成 **Plain**（纯图标）与 **Outlined**（1px 描边圆圈），
/// `Standard` 这个名字保留为 `Plain` 的别名语义，避免既有调用点全改。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconVariant {
    /// 纯图标（规格卡 `PLAIN`）：容器透明、图标 `OnSurfaceVariant`（各态图标色不变，12b 2.4）。
    Plain,
    /// 实心：`Primary` + `OnPrimary`。
    Filled,
    /// Tonal：`SecondaryContainer` + `OnSecondaryContainer`。
    Tonal,
    /// 描边：透明容器 + 1px `Outline`（规格卡第三颗，M229 新增）。
    Outlined,
}

impl IconVariant {
    /// 旧名字的别名：`Standard` = [`IconVariant::Plain`]（既有调用点不必改语义）。
    #[allow(non_upper_case_globals)]
    pub const Standard: IconVariant = IconVariant::Plain;

    /// 四式全序（gallery 顺序 = 规格卡从左到右）。
    pub const ALL: [IconVariant; 4] = [
        IconVariant::Filled,
        IconVariant::Tonal,
        IconVariant::Outlined,
        IconVariant::Plain,
    ];

    /// 稳定名字。
    pub const fn name(self) -> &'static str {
        match self {
            IconVariant::Plain => "plain",
            IconVariant::Filled => "filled",
            IconVariant::Tonal => "tonal",
            IconVariant::Outlined => "outlined",
        }
    }

    /// **禁用态图标色**：四式都是 `OnSurface` @0.38 —— A1
    /// （`StandardIconButtonTokens.DisabledColor` / `FilledIconButtonTokens.DisabledColor`）。
    pub(crate) const fn disabled_content(self) -> Role {
        Role::OnSurface
    }

    /// 色彩角色三元组（同 [`Variant::roles`] 的口径）。
    pub(crate) const fn roles(self) -> (Option<Role>, Role, Option<Role>) {
        match self {
            IconVariant::Plain => (None, Role::OnSurfaceVariant, None),
            IconVariant::Filled => (Some(Role::Primary), Role::OnPrimary, None),
            IconVariant::Tonal => (
                Some(Role::SecondaryContainer),
                Role::OnSecondaryContainer,
                None,
            ),
            IconVariant::Outlined => (None, Role::OnSurfaceVariant, Some(Role::Outline)),
        }
    }
}
