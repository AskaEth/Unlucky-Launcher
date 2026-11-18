//! **Silver Cat 浅色板**（规格卡 `SILVER CAT / LIGHT SCHEME` 一节 + 同页浅色字段/按钮）。
//!
//! 与 [`super::ink`] 同一套口径：49 行按 [`Role`] 声明序排列，`[spec]` = 规格卡直写值
//! （与 [`ANCHORS`] 同源，测试逐条比对 hex），`[derive]` = 按同色相 + M3 tone 关系推导后固化。

use crate::color::{Color, Role};

/// 规格卡直接给出的浅色色值，逐条对账用。
///
/// 出处分三处：SILVER CAT / LIGHT SCHEME 的色板行、同页 Filled/Tonal/Outlined 按钮与两个字段
/// （`on-primary` 取自 Filled 的文字 `#f6f9ff`、`on-secondary-container` 取自 Tonal 的文字
/// `#0d1d2a`、`outline-variant` 取自色板方块的描边 `#bec8cd`）。
///
/// ⚠️ 字段底 `#dcddde` 落在 **`surface-container-highest`**（不是 high）：它与 Ink 的字段底
/// `#262b25` 是同一层角色（`component_map` 的 `TextField` 口径 = 最高容器层）。浅色板的
/// `surface-container-high` 因此是**推导值** `#e4e4e5`（tone 90），只为让阶梯在 94.1 → 88.1
/// 之间保持单调、步长 ≥1.0 L\\*；若把 `#dcddde` 定在 high，Silver 的字段底会比 Ink 低一层，
/// 单一角色接法（只读 Highest）会读到 4.38:1 的 `Error` 标签（掉 AA）。
pub const ANCHORS: [(Role, &str); 13] = [
    (Role::Primary, "#006499"),
    (Role::OnPrimary, "#F6F9FF"),
    (Role::PrimaryContainer, "#87C7FF"),
    (Role::SecondaryContainer, "#D4E4F6"),
    (Role::OnSecondaryContainer, "#0D1D2A"),
    (Role::Surface, "#F9F9FA"),
    (Role::SurfaceContainer, "#EEEEEF"),
    (Role::SurfaceContainerHighest, "#DCDDDE"),
    (Role::OnSurface, "#1A1C1D"),
    (Role::OnSurfaceVariant, "#434D51"),
    (Role::Outline, "#6F797D"),
    (Role::OutlineVariant, "#BEC8CD"),
    (Role::Error, "#AC3434"),
];

/// Silver 板 49 个角色（下标 = `Role as usize`）。
pub const COLORS: [Color; 49] = [
    Color::rgb(0x00, 0x64, 0x99), // 01 primary            [spec] 实心按钮底、文字按钮标签
    Color::rgb(0xF6, 0xF9, 0xFF), // 02 on-primary         [spec] Filled 按钮文字
    Color::rgb(0x87, 0xC7, 0xFF), // 03 primary-container  [spec]
    Color::rgb(0x00, 0x34, 0x4E), // 04 on-primary-container [derive] container @ tone 20
    Color::rgb(0x4A, 0x60, 0x76), // 05 secondary          [derive] tone 40，蓝灰
    Color::rgb(0xF6, 0xF9, 0xFF), // 06 on-secondary       [derive] tone 98
    Color::rgb(0xD4, 0xE4, 0xF6), // 07 secondary-container [spec] Tonal 按钮底、选中 chip
    Color::rgb(0x0D, 0x1D, 0x2A), // 08 on-secondary-container [spec] Tonal 按钮文字
    Color::rgb(0x7C, 0x52, 0x25), // 09 tertiary           [derive] tone 40，暖棕（与主色互补）
    Color::rgb(0xFF, 0xFF, 0xFF), // 10 on-tertiary        [derive] tone 100
    Color::rgb(0xFF, 0xDC, 0xC1), // 11 tertiary-container [derive] tone 90
    Color::rgb(0x2B, 0x17, 0x00), // 12 on-tertiary-container [derive] tone 10
    Color::rgb(0xAC, 0x34, 0x34), // 13 error              [spec] 错误文字
    Color::rgb(0xFF, 0xFF, 0xFF), // 14 on-error           [derive] tone 100
    Color::rgb(0xFF, 0xDA, 0xD6), // 15 error-container    [derive] error @ tone 90
    Color::rgb(0x41, 0x00, 0x02), // 16 on-error-container [derive] error @ tone 10
    Color::rgb(0xF9, 0xF9, 0xFA), // 17 surface            [spec] 窗口底
    Color::rgb(0x1A, 0x1C, 0x1D), // 18 on-surface         [spec] 正文/标题
    Color::rgb(0xD9, 0xDE, 0xE2), // 19 surface-variant    [derive] neutral-variant tone 88
    Color::rgb(0x43, 0x4D, 0x51), // 20 on-surface-variant [spec] 次要文字、字段标签
    Color::rgb(0xF9, 0xF9, 0xFA), // 21 background         [spec]（= surface）
    Color::rgb(0x1A, 0x1C, 0x1D), // 22 on-background      [spec]（= on-surface）
    Color::rgb(0xD8, 0xDA, 0xDA), // 23 surface-dim        [derive] tone 87
    Color::rgb(0xF9, 0xF9, 0xFA), // 24 surface-bright     [derive]（= surface，M3 浅色同 tone 98）
    Color::rgb(0xFF, 0xFF, 0xFF), // 25 surface-container-lowest [derive] tone 100
    Color::rgb(0xF3, 0xF3, 0xF4), // 26 surface-container-low [derive] tone 96
    Color::rgb(0xEE, 0xEE, 0xEF), // 27 surface-container  [spec] 卡片/列表行底
    Color::rgb(0xE4, 0xE4, 0xE5), // 28 surface-container-high [derive] tone 90（比 highest 高 2.5 L*，阶梯步长 3.5）
    Color::rgb(0xDC, 0xDD, 0xDE), // 29 surface-container-highest [spec] 输入框底（与 Ink 的 #262B25 同位：字段底就是最高容器层）
    Color::rgb(0x6F, 0x79, 0x7D), // 30 outline            [spec] 字段描边
    Color::rgb(0xBE, 0xC8, 0xCD), // 31 outline-variant    [spec] 分隔线、chip 描边
    Color::rgb(0x2F, 0x31, 0x33), // 32 inverse-surface    [derive] tone 20
    Color::rgb(0xF0, 0xF0, 0xF1), // 33 inverse-on-surface [derive] tone 95
    Color::rgb(0x87, 0xC7, 0xFF), // 34 inverse-primary    [derive]（= primary-container）
    Color::rgb(0x00, 0x00, 0x00), // 35 shadow             M3 定值：纯黑 + 令牌 alpha
    Color::rgb(0x00, 0x00, 0x00), // 36 scrim              M3 定值：纯黑 + 令牌 alpha
    Color::rgb(0x00, 0x64, 0x99), // 37 surface-tint       = primary（M3 旧 tint 口径）
    Color::rgb(0x87, 0xC7, 0xFF), // 38 primary-fixed      [derive] tone 78（= container）
    Color::rgb(0x5F, 0xAE, 0xE6), // 39 primary-fixed-dim  [derive] tone 68
    Color::rgb(0x00, 0x1E, 0x2E), // 40 on-primary-fixed   [derive] tone 10
    Color::rgb(0x00, 0x3A, 0x57), // 41 on-primary-fixed-variant [derive] tone 23
    Color::rgb(0xD4, 0xE4, 0xF6), // 42 secondary-fixed    [derive] tone 90
    Color::rgb(0xB8, 0xC8, 0xDA), // 43 secondary-fixed-dim [derive] tone 80
    Color::rgb(0x0D, 0x1D, 0x2A), // 44 on-secondary-fixed [derive] tone 10
    Color::rgb(0x33, 0x48, 0x5A), // 45 on-secondary-fixed-variant [derive] tone 30
    Color::rgb(0xFF, 0xDC, 0xC1), // 46 tertiary-fixed     [derive] tone 90
    Color::rgb(0xF0, 0xBD, 0x8E), // 47 tertiary-fixed-dim [derive] tone 80
    Color::rgb(0x2B, 0x17, 0x00), // 48 on-tertiary-fixed  [derive] tone 10
    Color::rgb(0x5E, 0x3F, 0x1A), // 49 on-tertiary-fixed-variant [derive] tone 29
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_pin_the_spec() {
        for (role, hex) in ANCHORS {
            let expected = Color::from_hex(hex).expect("规格卡色值");
            assert_eq!(
                COLORS[role as usize],
                expected,
                "{} 的色值与规格卡（{hex}）不符：表里是 {}",
                role.name(),
                COLORS[role as usize].to_hex()
            );
        }
    }
}
