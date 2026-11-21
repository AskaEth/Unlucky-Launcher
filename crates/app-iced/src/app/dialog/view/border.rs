//! 卡片边界（M444 → **M449 分两条路**）：面板不透明才预合成 rim，带 α 的面板**显式降级**。
//!
//! ## 为什么要一条分支（M446 调查报告列的第一条生产缺陷）
//!
//! M444 把卡片边界接到 `components/rim.rs` 的 `panel_border`，但预合成的「底」传的是
//! `ui.color(Overlay.surface_role())`——**恒 α = 1.0 的角色色**；而面板画的是
//! `ui.layer(Overlay)`——壁纸模式下用户可以把面板 α 拉到 0.55（D36，下限 0.4）。
//! 两个式子各自都对，只是**不是同一个底**：
//!
//! ```text
//! 预合成算的边：  P ⊕ rim              （P = 角色的不透明色）
//! 屏幕上该是：    (α·P ⊕ (1−α)·B) ⊕ rim （B = 面板背后实际那层，今天是遮罩）
//! 两者之差：      Δ = (1 − rimα) × (1 − α) × (B − P)
//! ```
//!
//! 浅档 `rimα = 0.08`、面板 `α = 0.55`、背后是遮罩（B − P = −59）时 |Δ| ≈ **22 级**，深档（B − P = −19）
//! ≈ **6 级**——而且浅档那次的符号是反的：rim 墨色是黑，预合成出来的边（210）比面板自己（204）
//! **亮** 6 级，暗描边变成了白描边。实测逐值见 `DevDocs/artifacts/m449/`（`probe.py` 的输出）。
//!
//! `rim_over_fill` 的 `debug_assert_eq!(fill.a, 1.0)` **抓不到它**：调用方传的确实是不透明色，
//! 只是那个色不等于实际绘制底。所以这里不靠 assert 兜，而是**先看面板实际色**：
//!
//! | 面板实际 α | 走哪条 | 结果 |
//! |---|---|---|
//! | `== 1.0`（出厂 / CPU 档） | `panel_border` | 与 M444 **逐位相同**（`layer()` 在 α=1 时退化成 `color(surface_role())`，逐位相等） |
//! | `< 1.0`（壁纸模式 GPU 档） | 本文件显式降级 | **只留圆角、不画边** |
//!
//! ## 为什么降级是「不画」而不是「换个色画」
//!
//! 半透明底上，一道边该压在哪一层取决于面板**背后**是什么（遮罩？壁纸？照片？），而 `Border`
//! 这一层拿不到那个信息：交一个半透明边框给渲染器，wgpu 会把它画成「挖掉填充」（见
//! `components/rim.rs` 的预合成来由），CPU 档的 stroke 又是另一种口径。**两种失败都不如没有边**
//! ——与 M445 退掉提示条 rim 是同一条口径。判据与「抓得住 / 抓不住」写在
//! `components/rim.rs` 的「半透明底：**不许**预合成」。
//!
//! ## 这条分支的边缘情形（说在明处）
//!
//! CPU 档的面板 α 被 D36 强制成 1.0（`pages::appearance::alpha`），所以 α<1 × CPU 这格在
//! 产品路径上**走不到**；真要走到，这里也一并不画边（降级不看渲染档，理由是「不知道背后是什么」
//! 与渲染档无关）。

use iced::{Border, Color};

use unlucky_design::glass::GlassTier;
use unlucky_design::shape::ShapeScale;

use crate::app::components::rim::panel_border;
use crate::theme::Ui;

/// 卡片是 **S4 Modal**（`11b` §1.0：模态遮罩上层 = 对话框 / 确认框），只用来取 `rim` / `outline` 两列。
const DIALOG_TIER: GlassTier = GlassTier::S4;

/// 卡片边界：`panel` 是**面板实际画的那个色**（`Ui::layer(MaterialLayer::Overlay)`）。
///
/// 不透明 ⇒ 走组件库唯一那份 `panel_border`（GPU 档 S4 rim 预合成 / CPU 档 1px `OutlineVariant`）；
/// 带 α ⇒ 不预合成，只留圆角。
pub(super) fn card_border(ui: &Ui, panel: Color, shape: ShapeScale) -> Border {
    let radius = ui.radius_dp(shape);
    if panel.a < 1.0 {
        // 显式降级：底带 α 时预合成没有唯一的答案（见模块文档），宁可没有边。
        return Border {
            radius,
            ..Border::default()
        };
    }
    panel_border(ui, DIALOG_TIER, panel, radius)
}

#[cfg(test)]
mod tests;
