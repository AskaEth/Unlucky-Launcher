//! **Ink Cat 深色板**（规格卡 `DevDocs/artifacts/components-ink.svg` 的深色方案）。
//!
//! 49 行按 [`Role`] 声明序排列（下标 = `Role as usize`），由 `boards` 模块的
//! `anchors_pin_the_spec` 测试用 [`ANCHORS`] 逐条对账——**行错位会当场红**。
//!
//! 取值分三类，来源都在行注释里：
//! - `[spec]`：规格卡上直接写出的色值（[`ANCHORS`] 里同一份，测试逐条比对 hex）；
//! - `[derive]`：规格卡没给的角色，按「同色相 + M3 的 tone 关系」从邻居推导后固化；
//! - `[D59]`：**强调族（26 个角色）= 图标紫重解**。色相换成图标采样值 `#3A3170` 的 CAM16
//!   色相 292.3397°（tertiary 族 +60° = 352.3397°），**每个角色的 tone 原样保留**——深色板强调族
//!   的 tone 是规格卡给的（primary 98 / container 93 / on-primary 40 / …），只换色相不动明度，
//!   所以 26 对文字对比度**几乎不变**（逐对最大 |Δ| = **0.056**：`on-primary-fixed` on
//!   `primary-fixed` 13.708 → 13.652；最差对 5.196 不动，无一对跌破 AA——差来自 8 bit 量化 +
//!   彩度重取）。彩度取 M3 Tonal Spot 常数（primary 36 / secondary 16 / tertiary 24）。登记见决策 D59。
//!
//! 推导口径（`[derive]` 行）：容器/文字的 tone 差沿用 M3 ColorSpec2021 的成对关系
//! （`on-*` 与承载色差 ≥50 tone 才稳过 AA），色相取承载色的 CAM16 色相，彩度只压不抬。
//! 换算过程是 `Hct::from_components(hue, chroma, tone)`，结果已固化成字面量，不在运行期重算——
//! 这样色板是**可逐值审阅的数据**，而不是「跑一遍才知道」的产物。

use crate::color::{Color, Role};

/// 规格卡直接给出的色值（`[spec]` 行）+ 强调族锚点（`[D59]`），用于逐条对账与文档生成。
///
/// 中性 / error 部分顺序即规格卡上出现的顺序（TEXT FIELDS → SWITCH/CHIPS → CARD/DIALOG/SNACKBAR
/// → COLOR ROLES / INK）。**这张表是色板的出处**：表里的值改了，测试 `anchors_pin_the_spec`
/// 会指着要改的行。
pub const ANCHORS: [(Role, &str); 22] = [
    (Role::Primary, "#FDF8FF"),
    (Role::OnPrimary, "#5F5891"),
    (Role::PrimaryContainer, "#EEE8FF"),
    (Role::OnPrimaryContainer, "#574F88"),
    (Role::Secondary, "#C9C3DC"),
    (Role::SecondaryContainer, "#474458"),
    (Role::OnSecondaryContainer, "#E5DFF8"),
    (Role::Tertiary, "#ECB8CD"),
    (Role::TertiaryContainer, "#DEABC0"),
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
    (Role::InversePrimary, "#5F5891"),
];

/// Ink 板 49 个角色（下标 = `Role as usize`）。
pub const COLORS: [Color; 49] = [
    Color::rgb(0xFD, 0xF8, 0xFF), // 01 primary            [D59] 按钮/文字按钮标签、进度、选中（tone 98 = 规格卡原档）
    Color::rgb(0x5F, 0x58, 0x91), // 02 on-primary         [D59] tone 40.3
    Color::rgb(0xEE, 0xE8, 0xFF), // 03 primary-container  [D59] FAB 底、头像底（tone 93 = 规格卡原档）
    Color::rgb(0x57, 0x4F, 0x88), // 04 on-primary-container [D59] FAB 图标（tone 36.7）
    Color::rgb(0xC9, 0xC3, 0xDC), // 05 secondary          [D59] tone 79.9
    Color::rgb(0x47, 0x44, 0x58), // 06 on-secondary       [D59] tone 29.9（与 07 同值：同一色相同一 tone）
    Color::rgb(0x47, 0x44, 0x58), // 07 secondary-container [D59] tonal 按钮底、选中 chip
    Color::rgb(0xE5, 0xDF, 0xF8), // 08 on-secondary-container [D59] tonal 按钮标签（tone 90）
    Color::rgb(0xEC, 0xB8, 0xCD), // 09 tertiary           [D59] tone 80，玫紫（= 主色相 +60°）
    Color::rgb(0x4E, 0x2B, 0x3C), // 10 on-tertiary        [D59] tone 22.5
    Color::rgb(0xDE, 0xAB, 0xC0), // 11 tertiary-container  [D59] tone 75.1
    Color::rgb(0x48, 0x25, 0x36), // 12 on-tertiary-container [D59] tone 20
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
    Color::rgb(0x5F, 0x58, 0x91), // 34 inverse-primary    [D59] snackbar 动作文字（tone 40.3）
    Color::rgb(0x00, 0x00, 0x00), // 35 shadow             M3 定值：纯黑 + 令牌 alpha
    Color::rgb(0x00, 0x00, 0x00), // 36 scrim              M3 定值：纯黑 + 令牌 alpha
    Color::rgb(0xFD, 0xF8, 0xFF), // 37 surface-tint       = primary（M3 旧 tint 口径）
    Color::rgb(0xEE, 0xE8, 0xFF), // 38 primary-fixed      [D59] tone 93（= container）
    Color::rgb(0xD7, 0xCF, 0xFF), // 39 primary-fixed-dim  [D59] tone 85
    Color::rgb(0x1F, 0x17, 0x4E), // 40 on-primary-fixed   [D59] tone 12
    Color::rgb(0x42, 0x3B, 0x72), // 41 on-primary-fixed-variant [D59] tone 28
    Color::rgb(0xE5, 0xDF, 0xF8), // 42 secondary-fixed    [D59] tone 90
    Color::rgb(0xC9, 0xC3, 0xDC), // 43 secondary-fixed-dim [D59] tone 80
    Color::rgb(0x1C, 0x19, 0x2B), // 44 on-secondary-fixed [D59] tone 10
    Color::rgb(0x43, 0x40, 0x54), // 45 on-secondary-fixed-variant [D59] tone 28
    Color::rgb(0xFF, 0xD4, 0xE5), // 46 tertiary-fixed     [D59] tone 88.9
    Color::rgb(0xEC, 0xB8, 0xCD), // 47 tertiary-fixed-dim [D59] tone 80（= tertiary）
    Color::rgb(0x30, 0x10, 0x20), // 48 on-tertiary-fixed  [D59] tone 10
    Color::rgb(0x5C, 0x37, 0x48), // 49 on-tertiary-fixed-variant [D59] tone 28
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
