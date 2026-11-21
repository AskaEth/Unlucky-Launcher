//! 矩阵**逐行**判据：禁用态内容色（12b 1.8）× 9 行 + 图标按钮第四式（Outlined）的 1px 环。
//!
//! 从 `probe.rs` 纯搬移（M229 复审 P2：加第 9 行与环断言后 `probe.rs` 409 行，超 400 硬线）。
//! 判据一行未改，只有入口从「行内代码」变成 [`assert_matrix_rows`]。

use unlucky_design::color::Role;

use super::super::scan::{rgb, Shot};
use crate::app::components::interaction;
use crate::app::components::interaction::Base;
use crate::theme::Ui;

/// 矩阵行距（40 高按钮 + 8 间距）。
pub(super) const ROW_STEP: usize = 48;
/// 行内扫描高度（可见 40 + 上下各 2 的余量）。
const ROW_HEIGHT: usize = 44;
/// 矩阵行数（5 个文字变体 + 4 个图标变体）：probe 的汇总日志用它。
pub(super) const ROW_COUNT: usize = 9;

/// 逐行断言：①每一行的禁用内容色（A1 逐变体不同）②Outlined 图标行有环、Plain 行没有。
pub(super) fn assert_matrix_rows(shot: &Shot, ui: &Ui, label_top: usize, label_left: usize) {
    // 行序 = `gallery/buttons.rs` 先 `Variant::ALL`（5 行）再 `IconVariant::ALL`（**4 行**，M229 起
    // outlined 是第四式）：共 **9 行**。`on_container` 必须与 `assemble::content_color` 一致
    // ——图标按钮的 filled/tonal 有容器（`fill.is_some()`），outlined/plain 没有。
    let rows: [(usize, Role, bool, &str); 9] = [
        (0, Role::OnSurfaceVariant, true, "filled"),
        (1, Role::OnSurface, true, "tonal"), // A1：FilledTonal 是例外
        (2, Role::OnSurfaceVariant, true, "elevated"),
        (3, Role::OnSurfaceVariant, false, "outlined"),
        (4, Role::OnSurfaceVariant, false, "text"),
        (5, Role::OnSurface, true, "icon·filled"), // A1：IconButton 全是 OnSurface
        (6, Role::OnSurface, true, "icon·tonal"),
        (7, Role::OnSurface, false, "icon·outlined"),
        (8, Role::OnSurface, false, "icon·plain"),
    ];
    let column_from = shot.width * 2 / 3; // 矩阵最右列 = disabled 列
    for (index, role, on_container, name) in rows {
        let y0 = label_top + index * ROW_STEP;
        let y1 = y0 + ROW_HEIGHT;
        // **与实现同源**：按钮族的内容色一律走 `disabled_button_content`（38% 叠在 10% 底上）。
        // 用面状控件族的 12% 底会让断言比实现宽一档，差的 1/255 会放跑「容器对内容错」。
        let ink = interaction::disabled_button_content(ui, Base::CANVAS, role, on_container);
        let count = shot.count_in(rgb(ink), column_from, shot.width, y0, y1);
        // 阈值 **30**：最窄的一行是只有 4 个字符的 `text` 变体（实测 38 px 墨迹，M229 复审后
        // 按钮内距 24→20 让标签左移，余量只剩 2 px）。角色接错时会掉到个位数甚至 0。
        assert!(
            count > 30,
            "{name} 行的禁用内容（{role:?}@38%）在 disabled 列里只有 {count} px —— 这一行没走 A1 的禁用角色"
        );
        // 反向判据（不靠阈值）：**满档**内容色不许比 38% 的还多——那样说明这一行没降透明度。
        let full = shot.count_in(rgb(ui.color(role)), column_from, shot.width, y0, y1);
        assert!(
            full < count,
            "{name} 行里满档 {role:?} 有 {full} px、38% 只有 {count} px：这一行的禁用透明度没生效"
        );
    }

    // ---- M229 复审 P1-1：Outlined 图标按钮必须有 1px 环，Plain 必须没有 ----
    //
    // 只有渲染层能证：`icon()` 曾把 `StyleSpec.stroke` 写死 0，Outlined 与 Plain 逐像素相同。
    // 纯函数那条在 `buttons::tests::only_the_outlined_icon_variant_draws_a_ring`（证明「传了 1.0」），
    // 这条证明「画出来了」。
    // **只能扫「环带」，且只能按「有没有墨」判**：1px 环在软渲染里是 Outline 叠画布的抗锯齿
    // 混色（实测左缘 `(110,124,110)`，而 Outline 本体是 `(123,138,123)`）——按颜色匹配会漏，
    // 而且 `Outline` 与「字形压在画布上的边缘」在容差内几乎同色（扫整格会把 Plain 行数成 16 px 假环）。
    //
    // 环带的判据是**几何 + 墨量**：40×40 圆按钮的左缘在 x = left…left+4（半径 20 的弧垂直中线处
    // 贴左），而 24×24 字形从 left+8 才开始 —— Plain 行这一段实测是纯画布色 0 px。
    let surface = rgb(ui.color(Role::Surface));
    let is_ink = |pixel: [u8; 3]| pixel.iter().zip(surface).any(|(a, b)| a.abs_diff(b) > 8);
    let band_ink = |y0: usize| {
        let mut n = 0;
        for y in (y0 + 14)..(y0 + 26) {
            for x in label_left..(label_left + 4) {
                if is_ink(shot.at(x, y)) {
                    n += 1;
                }
            }
        }
        n
    };
    let outlined = band_ink(label_top + 7 * ROW_STEP);
    let plain = band_ink(label_top + 8 * ROW_STEP);
    assert!(
        outlined >= 4,
        "Outlined 图标按钮这一行左缘没有环墨（{outlined} px）：`icon()` 的 stroke 又被写死了？"
    );
    assert_eq!(
        plain, 0,
        "Plain 图标按钮这一行左缘出现了 {plain} px 环墨（Plain 不该有描边）"
    );
}
