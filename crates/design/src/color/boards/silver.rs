//! **Silver Cat 浅色板**（规格卡 `SILVER CAT / LIGHT SCHEME` 一节 + 同页浅色字段/按钮）。
//!
//! 与 [`super::ink`] 同一套口径：49 行按 [`Role`] 声明序排列。取值分三类：
//! - `[spec]`：规格卡直写值（与 [`ANCHORS`] 同源，测试逐条比对 hex）；
//! - `[derive]`：规格卡没给的角色，按同色相 + M3 tone 关系推导后固化；
//! - `[D59]`：**强调族（26 个角色）= 图标紫重解**——色相换成图标采样值
//!   `#3A3170` 的 CAM16 色相 292.3397°（tertiary 族 +60° = 352.3397°），
//!   **每个角色的 tone 原样保留**，彩度取 M3 Tonal Spot 常数（primary 36 / secondary 16 /
//!   tertiary 24）。这是「只换色相、不动明度」的一条硬规则，所以 26 对文字对比度**几乎不变**
//!   （逐对最大 |Δ| = **0.053**：`on-tertiary-container` on `tertiary-container` 13.270 → 13.217；
//!   最差对 4.982 → 5.001 反而变好，无一对跌破 AA——差来自 8 bit 量化 + 彩度重取），
//!   登记见决策 D59。

use crate::color::{Color, Role};

/// 规格卡直接给出的浅色色值，逐条对账用。
///
/// 出处分三处：SILVER CAT / LIGHT SCHEME 的色板行、同页 Filled/Tonal/Outlined 按钮与两个字段
/// （`on-primary` 取自 Filled 的文字、`outline-variant` 取自色板方块的描边 `#bec8cd`），
/// 其余是 M378/D59 的强调族锚点（紫）。
///
/// ⚠️ 字段底 `#dcddde` 落在 **`surface-container-highest`**（不是 high）：它与 Ink 的字段底
/// `#262b25` 是同一层角色（`component_map` 的 `TextField` 口径 = 最高容器层）。浅色板的
/// `surface-container-high` 因此是**推导值** `#e4e4e5`（tone 90），只为让阶梯在 94.1 → 88.1
/// 之间保持单调、步长 ≥1.0 L\\*；若把 `#dcddde` 定在 high，Silver 的字段底会比 Ink 低一层，
/// 单一角色接法（只读 Highest）会读到 4.38:1 的 `Error` 标签（掉 AA）。
pub const ANCHORS: [(Role, &str); 13] = [
    (Role::Primary, "#5F5891"),
    (Role::OnPrimary, "#FCF7FF"),
    (Role::PrimaryContainer, "#C3B9FA"),
    (Role::SecondaryContainer, "#E5DFF9"),
    (Role::OnSecondaryContainer, "#1C192C"),
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
    Color::rgb(0x5F, 0x58, 0x91), // 01 primary            [D59] 实心按钮底、文字按钮标签、进度（tone 40.3）
    Color::rgb(0xFC, 0xF7, 0xFF), // 02 on-primary         [D59] Filled 按钮文字（tone 97.8）
    Color::rgb(0xC3, 0xB9, 0xFA), // 03 primary-container  [D59] FAB 底、头像底（tone 78 = 规格卡原档）
    Color::rgb(0x30, 0x28, 0x5F), // 04 on-primary-container [D59] FAB 图标（tone 20 = 规格卡原档）
    Color::rgb(0x5F, 0x5B, 0x71), // 05 secondary          [D59] tone 40，紫灰
    Color::rgb(0xFC, 0xF7, 0xFF), // 06 on-secondary       [D59] tone 98
    Color::rgb(0xE5, 0xDF, 0xF9), // 07 secondary-container [D59] Tonal 按钮底、选中 chip（tone 90）
    Color::rgb(0x1C, 0x19, 0x2C), // 08 on-secondary-container [D59] Tonal 按钮文字（tone 10）
    Color::rgb(0x78, 0x4F, 0x61), // 09 tertiary           [D59] tone 38.6，玫紫（= 主色相 +60°）
    Color::rgb(0xFF, 0xFF, 0xFF), // 10 on-tertiary        [D59] tone 100
    Color::rgb(0xFF, 0xD8, 0xE7), // 11 tertiary-container [D59] tone 90
    Color::rgb(0x30, 0x11, 0x21), // 12 on-tertiary-container [D59] tone 10
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
    Color::rgb(0xC3, 0xB9, 0xFA), // 34 inverse-primary    [D59]（= primary-container）
    Color::rgb(0x00, 0x00, 0x00), // 35 shadow             M3 定值：纯黑 + 令牌 alpha
    Color::rgb(0x00, 0x00, 0x00), // 36 scrim              M3 定值：纯黑 + 令牌 alpha
    Color::rgb(0x5F, 0x58, 0x91), // 37 surface-tint       = primary（M3 旧 tint 口径）
    Color::rgb(0xC3, 0xB9, 0xFA), // 38 primary-fixed      [D59] tone 78（= container）
    Color::rgb(0xA8, 0x9F, 0xDE), // 39 primary-fixed-dim  [D59] tone 68
    Color::rgb(0x1B, 0x11, 0x49), // 40 on-primary-fixed   [D59] tone 10
    Color::rgb(0x36, 0x2E, 0x65), // 41 on-primary-fixed-variant [D59] tone 22.7
    Color::rgb(0xE5, 0xDF, 0xF9), // 42 secondary-fixed    [D59] tone 90
    Color::rgb(0xC9, 0xC3, 0xDC), // 43 secondary-fixed-dim [D59] tone 80
    Color::rgb(0x1C, 0x19, 0x2C), // 44 on-secondary-fixed [D59] tone 10
    Color::rgb(0x47, 0x43, 0x58), // 45 on-secondary-fixed-variant [D59] tone 29.6
    Color::rgb(0xFF, 0xD8, 0xE7), // 46 tertiary-fixed     [D59] tone 90
    Color::rgb(0xED, 0xB8, 0xCE), // 47 tertiary-fixed-dim [D59] tone 80.1
    Color::rgb(0x30, 0x11, 0x21), // 48 on-tertiary-fixed  [D59] tone 10
    Color::rgb(0x60, 0x3A, 0x4B), // 49 on-tertiary-fixed-variant [D59] tone 29.5
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
