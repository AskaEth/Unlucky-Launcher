//! **Ink Cat 深色板**（规格卡 `DevDocs/artifacts/components-ink.svg` 的深色方案）。
//!
//! 49 行按 [`Role`] 声明序排列（下标 = `Role as usize`），由 `boards` 模块的
//! `anchors_pin_the_spec` 测试用 [`ANCHORS`] 逐条对账——**行错位会当场红**。
//!
//! 取值分两类，来源都在行注释里：
//! - `[spec]`：规格卡上直接写出的色值（[`ANCHORS`] 里同一份，测试逐条比对 hex）；
//! - `[derive]`：规格卡没给的角色，按「同色相 + M3 的 tone 关系」从邻居推导后固化。
//!
//! 推导口径（`[derive]` 行）：容器/文字的 tone 差沿用 M3 ColorSpec2021 的成对关系
//! （`on-*` 与承载色差 ≥50 tone 才稳过 AA），色相取承载色的 CAM16 色相，彩度只压不抬。
//! 换算过程是 `Hct::from_components(hue, chroma, tone)`，结果已固化成字面量，不在运行期重算——
//! 这样色板是**可逐值审阅的数据**，而不是「跑一遍才知道」的产物。

use crate::color::{Color, Role};

/// 规格卡直接给出的色值（`[spec]` 行），用于逐条对账与文档生成。
///
/// 顺序即规格卡上出现的顺序（BUTTONS → FAB → TEXT FIELDS → SWITCH/CHIPS → CARD/DIALOG/SNACKBAR
/// → COLOR ROLES / INK）。**这张表是色板的出处**：表里的值改了，测试 `anchors_pin_the_spec`
/// 会指着要改的行。
pub const ANCHORS: [(Role, &str); 22] = [
    (Role::Primary, "#E8FFEF"),
    (Role::OnPrimary, "#256B4B"),
    (Role::PrimaryContainer, "#B3FBD2"),
    (Role::OnPrimaryContainer, "#1B6243"),
    (Role::Secondary, "#B4CCBC"),
    (Role::SecondaryContainer, "#364B3F"),
    (Role::OnSecondaryContainer, "#D0E8D7"),
    (Role::Tertiary, "#FFB77D"),
    (Role::TertiaryContainer, "#FFA453"),
    (Role::Surface, "#0F150F"),
    (Role::SurfaceContainer, "#151B15"),
    (Role::SurfaceContainerHigh, "#1D231D"),
    (Role::SurfaceContainerHighest, "#262B25"),
    (Role::OnSurface, "#E4EAE0"),
    (Role::OnSurfaceVariant, "#B6C5B4"),
    (Role::Outline, "#7B8A7B"),
    (Role::OutlineVariant, "#3C4A3D"),
    (Role::Error, "#FF716C"),
    (Role::OnError, "#490006"),
    (Role::InverseSurface, "#E4EAE0"),
    (Role::InverseOnSurface, "#282E27"),
    (Role::InversePrimary, "#256B4B"),
];

/// Ink 板 49 个角色（下标 = `Role as usize`）。
pub const COLORS: [Color; 49] = [
    Color::rgb(0xE8, 0xFF, 0xEF), // 01 primary            [spec] 按钮/文字按钮标签、进度、选中
    Color::rgb(0x25, 0x6B, 0x4B), // 02 on-primary         [spec]
    Color::rgb(0xB3, 0xFB, 0xD2), // 03 primary-container  [spec] FAB 底、头像底
    Color::rgb(0x1B, 0x62, 0x43), // 04 on-primary-container [spec] FAB 图标
    Color::rgb(0xB4, 0xCC, 0xBC), // 05 secondary          [spec]
    Color::rgb(0x37, 0x4B, 0x3E), // 06 on-secondary       [derive] secondary @ tone 30
    Color::rgb(0x36, 0x4B, 0x3F), // 07 secondary-container [spec] tonal 按钮底、选中 chip
    Color::rgb(0xD0, 0xE8, 0xD7), // 08 on-secondary-container [spec] tonal 按钮标签
    Color::rgb(0xFF, 0xB7, 0x7D), // 09 tertiary           [spec] 暖橙装饰
    Color::rgb(0x55, 0x2B, 0x00), // 10 on-tertiary        [derive] tertiary @ tone 22
    Color::rgb(0xFF, 0xA4, 0x53), // 11 tertiary-container  [spec]
    Color::rgb(0x4C, 0x27, 0x00), // 12 on-tertiary-container [derive] container @ tone 20
    Color::rgb(0xFF, 0x71, 0x6C), // 13 error              [spec] 错误文字、badge 底
    Color::rgb(0x49, 0x00, 0x06), // 14 on-error           [spec] badge 数字
    Color::rgb(0x93, 0x00, 0x0A), // 15 error-container    [derive] error @ tone 30（= M3 深色 error-container）
    Color::rgb(0xFF, 0xDA, 0xD6), // 16 on-error-container [derive] error @ tone 90
    Color::rgb(0x0F, 0x15, 0x0F), // 17 surface            [spec] 窗口底
    Color::rgb(0xE4, 0xEA, 0xE0), // 18 on-surface         [spec] 正文/标题
    Color::rgb(0x3C, 0x4A, 0x3D), // 19 surface-variant    [spec]（= outline-variant，M3 深色同 tone）
    Color::rgb(0xB6, 0xC5, 0xB4), // 20 on-surface-variant [spec] 次要文字、字段标签
    Color::rgb(0x0F, 0x15, 0x0F), // 21 background         [spec]（= surface）
    Color::rgb(0xE4, 0xEA, 0xE0), // 22 on-background      [spec]（= on-surface）
    Color::rgb(0x0C, 0x13, 0x0C), // 23 surface-dim        [derive] surface −1 tone 档
    Color::rgb(0x35, 0x3C, 0x34), // 24 surface-bright     [derive] surface @ tone 24
    Color::rgb(0x08, 0x0D, 0x08), // 25 surface-container-lowest [derive] tone 3
    Color::rgb(0x13, 0x18, 0x13), // 26 surface-container-low [derive] tone 8
    Color::rgb(0x15, 0x1B, 0x15), // 27 surface-container  [spec] 卡片/列表行底
    Color::rgb(0x1D, 0x23, 0x1D), // 28 surface-container-high [spec] 对话框底
    Color::rgb(0x26, 0x2B, 0x25), // 29 surface-container-highest [spec] 输入框底
    Color::rgb(0x7B, 0x8A, 0x7B), // 30 outline            [spec] 字段描边、switch 关态
    Color::rgb(0x3C, 0x4A, 0x3D), // 31 outline-variant    [spec] 分隔线、chip 描边
    Color::rgb(0xE4, 0xEA, 0xE0), // 32 inverse-surface    [spec] snackbar 底
    Color::rgb(0x28, 0x2E, 0x27), // 33 inverse-on-surface [spec] snackbar 文字
    Color::rgb(0x25, 0x6B, 0x4B), // 34 inverse-primary    [spec] snackbar 动作文字
    Color::rgb(0x00, 0x00, 0x00), // 35 shadow             M3 定值：纯黑 + 令牌 alpha
    Color::rgb(0x00, 0x00, 0x00), // 36 scrim              M3 定值：纯黑 + 令牌 alpha
    Color::rgb(0xE8, 0xFF, 0xEF), // 37 surface-tint       = primary（M3 旧 tint 口径）
    Color::rgb(0xB3, 0xFB, 0xD2), // 38 primary-fixed      [derive] tone 93（= container）
    Color::rgb(0x9D, 0xE4, 0xBC), // 39 primary-fixed-dim  [derive] tone 85
    Color::rgb(0x00, 0x26, 0x14), // 40 on-primary-fixed   [derive] tone 12
    Color::rgb(0x00, 0x4D, 0x2E), // 41 on-primary-fixed-variant [derive] tone 28
    Color::rgb(0xD0, 0xE8, 0xD7), // 42 secondary-fixed    [derive] tone 90
    Color::rgb(0xB4, 0xCC, 0xBC), // 43 secondary-fixed-dim [derive] tone 80
    Color::rgb(0x0D, 0x1F, 0x14), // 44 on-secondary-fixed [derive] tone 10
    Color::rgb(0x33, 0x47, 0x39), // 45 on-secondary-fixed-variant [derive] tone 28
    Color::rgb(0xFC, 0xD9, 0xBE), // 46 tertiary-fixed     [derive] tone 89
    Color::rgb(0xFF, 0xB7, 0x7D), // 47 tertiary-fixed-dim [derive] tone 80（= tertiary）
    Color::rgb(0x2A, 0x17, 0x00), // 48 on-tertiary-fixed  [derive] tone 10
    Color::rgb(0x56, 0x3D, 0x27), // 49 on-tertiary-fixed-variant [derive] tone 28
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
