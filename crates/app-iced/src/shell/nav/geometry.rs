//! 导航轨的**纯几何**（M300 从 `nav.rs` 拆出）：两个宽度档的插值 + 药丸矩形 + 项高。
//!
//! 为什么拆：`nav.rs` 在 M300 要加「折叠态的一项」与「折叠按钮」两件事，几何这一块（约 85 行）
//! 又不与视图组装耦合——它就是几条令牌推导。拆开之后 `nav.rs` 只剩「谁放在哪」。
//!
//! **唯一实现点不变**：`indicator_size` / `state_layer_box` / `nav_item_height` 仍各只有一份，
//! `nav.rs` 用 `pub(super) use` 把它们按原名再导出，于是 `shell/tests.rs` 里
//! `super::nav::{indicator_size, nav_item_height, state_layer_box}` 的路径一个字符都没变
//! （那是 M283/M299 的几何判据，不许为了搬文件改断言）。
//!
//! `pill_color` **留在了 `nav.rs`**：`shell/tests.rs` 与 M299 的
//! `components/selection/tests/single_entry.rs` 都按**文件名**扫「这一处必须经
//! `selected_fill` / `layer_color` 求值」，把它搬走会让那两条门禁静默失守（`selected_fill` 的
//! 白名单里写的是 `shell/nav.rs`）。搬文件不许改别人的门禁 —— 这条比「按职责拆分」优先。

use unlucky_design::spacing::{LayoutMetrics, Space};
use unlucky_design::typography::TypeRole;

use crate::theme::Ui;

/// 导航轨宽度（M69 起是令牌层的 `nav_rail_width`）。
///
/// M69（`141e9b1`）把 `LayoutMetrics` 的 nav 字段做了语义纠正：
///
/// | 旧 | 新 |
/// |---|---|
/// | `nav_expanded_width = 88` | **`nav_rail_width = 88`**（值不变，名字从「展开」改成「轨」） |
/// | `nav_collapsed_width = 48` | 删除（48 连 56 宽的选中指示器都放不下） |
/// | —（缺） | `nav_drawer_width = 240`（D33 的展开态抽屉，本 crate 未用） |
///
/// 三处调用点都走这里：将来令牌再改名，仍然只改这一行。
pub(in crate::shell) fn nav_rail_width(metrics: LayoutMetrics) -> f32 {
    metrics.nav_rail_width
}

/// 这一帧的轨宽：折叠档 ⇄ 展开档之间按动效进度插值（M300）。
///
/// - `progress = 1.0` → **逐位**等于 [`nav_rail_width`]（88）——静息展开态与 M283/M299 的图一字不差；
/// - `progress = 0.0` → **逐位**等于 [`LayoutMetrics::nav_collapsed_width`]（64）；
/// - 中间帧是线性的（曲线已经在帧泵那一侧按 `Emphasized*` 采过样，这里再套一条曲线就是两次缓动）。
///
/// 为什么这一层住在这里而不是 `design::spacing`：它只是「两个令牌之间的插值」，不是新令牌；
/// 而折叠档能容下 56 宽胶囊这件事由 `spacing.rs` 的断言钉住，两处不重叠。
pub(in crate::shell) fn nav_width(metrics: LayoutMetrics, progress: f32) -> f32 {
    let collapsed = metrics.nav_collapsed_width;
    let open = nav_rail_width(metrics);
    collapsed + (open - collapsed) * progress.clamp(0.0, 1.0)
}

/// 选中指示器尺寸（12b 5.4 参考 56×32），全部由令牌推导、不写死：
///
/// - 宽 = [`nav_rail_width`] − 2 × `Space::X4` = 88 − 32 = **56**（轨的左右内边距 16）
/// - 高 = `control_height − Space::X2` = 40 − 8 = **32**（可见高度上下各让 4）
pub(in crate::shell) fn indicator_size(metrics: LayoutMetrics) -> (f32, f32) {
    (
        nav_rail_width(metrics) - Space::X4.dp() * 2.0,
        metrics.control_height - Space::X2.dp(),
    )
}

/// 状态层（hover / pressed / 过渡帧）画在**哪块矩形**上：药丸，与选中指示器同一块几何（M283）。
///
/// 这是那块矩形的唯一定义点——`rail_item` 用它定尺寸，`shell::tests` 的几何判据钉住它。
/// 它**不是**按钮的 `nav_rail_width × 项高`（88×52）：那正是用户点名的「父容器整个方形区域加深」。
pub(in crate::shell) fn state_layer_box(metrics: LayoutMetrics) -> (f32, f32) {
    indicator_size(metrics)
}

/// 导航项高度 = 指示器 + 4 + 标签行高 = 32 + 4 + 16 = 52（`LM.nav_item_height` 是列表行节奏，
/// 药丸 + 标签的轨项放不进 44，偏差登记在交接文档里）。
pub(in crate::shell) fn nav_item_height(ui: &Ui, indicator_height: f32) -> f32 {
    indicator_height + ui.space(Space::X1) + ui.style(TypeRole::LabelMedium).line_height_px
}
