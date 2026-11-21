//! 分隔线（`research/15` §22，A1：`DividerTokens.kt` / `Divider.kt` / `ListTokens.kt`）。
//!
//! | # | 检查项 | 参考值 | 落点 |
//! |---|---|---|---|
//! | 22.1 | 厚度 | **1dp** → 1px ±0 @100% DPI | `LayoutMetrics::divider_thickness` = [`STROKE_PX`] |
//! | 22.2 | 颜色 | **`OutlineVariant`**（不是 `Outline`，那个更深、会抢层级） | `Role::OutlineVariant` |
//! | 22.3 | 缩进 | 列表内 **左 16 / 右 16**；区块分隔 **0（通栏）** | [`list`] / [`full`] |
//! | 22.4 | 用量 | 同屏同类线 ≤3；同容器内不得既有描边又有分隔线 | 页面线自查（本模块不强制） |
//! | 22.5 | 何时不用 | tone 能分层就不画线；纵向分隔本项目不用 | 文档约定 |
//!
//! ## 语义纪律（22.5 的一句话口径）
//!
//! **分隔线是兜底手段，不是排版手段**：能靠 `Surface` / `SurfaceContainerLow` /
//! `SurfaceContainerHigh` 的色差把层级说清的地方就不画线；卡片已经有 1px `OutlineVariant`
//! 描边时，卡片**之间**不再画线（同时出现 = 双重分隔，rubric K3 那类廉价感）。
//!
//! ## 为什么只有两个入口
//!
//! §22.3 只承认两种缩进：通栏（区块分隔）与列表内 16。第三种缩进一定是「页面自己想出来的」，
//! 所以不提供任意数值入口——真要第三种，先在 `SubAgentsTalk/` 登记。
//!
//! ## M454 · 1px 线的重量为什么曾经随缩放档跳变
//!
//! iced 的 `crisp` feature（`crates/app-iced/Cargo.toml` 显式打开）让每个 quad 默认
//! `snap: true`（`iced_core::renderer::Quad::default()`）。wgpu 后端据此把 quad 的**两条边各自
//! `round` 到整物理像素**（`iced_wgpu/src/shader/quad.wgsl` 的 `pos_snap` / `scale_snap`），于是
//! 「1 逻辑 px 高的填充盒」在 125% 上被压成 1 行（有效逻辑粗 **0.80**）、在 150% 上被撑成 2 行
//! （**1.33**）。同一帧走 `Border` 掩膜的 1px 卡片描边却恒 **1.00**：吸附只改 quad 的矩形，
//! **描边宽度是片元侧量 `border_width * scale`**（1.25 / 1.5 物理 px），所以它有抗锯齿灰边。
//!
//! ⇒ 1px 线**既不能用 `Background` 填充、也不能靠 `Border` 修**（`Border` 下 quad 仍被吸附，
//! 仍只剩 1 整行）。这里自己 `fill_quad` 一个 [`Quad::snap`] = `false` 的 quad（[`Hairline`]）：
//! 1.25 物理 px 的矩形于是摊成「1 行实色 + 0.25 幽灵」，与卡片描边同形。
//! tiny-skia 没有吸附这回事（`iced_tiny_skia` 里**没有** `snap` 字段，该值不被读），
//! 所以这个开关在 CPU 后端是恒等变换——两端剖面因此同形。

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::Quad;
use iced::advanced::widget::Tree;
use iced::advanced::{mouse, Widget};
use iced::{Background, Color, Element, Length, Rectangle, Size};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;

use crate::theme::{Ui, STROKE_PX};

/// 通栏分隔线（区块分隔：0 缩进）。
pub fn full<'a, M: 'a>(ui: &'a Ui) -> Element<'a, M> {
    line(ui)
}

/// 列表内分隔线：左缩进 `Space::X4`（16），右侧到底（`ListTokens.DividerLeadingSpace`）。
///
/// 右缩进按官方也是 16，但列表行本身已有 16 的内边距时再缩一次会变成 32——
/// 本项目把「行内边距」交给行，本函数只负责**线相对行**的缩进，故右端通到行尾。
pub fn list<'a, M: 'a>(ui: &'a Ui) -> Element<'a, M> {
    super::layout::inset_start(ui.space(Space::X4), line(ui))
}

/// 一条 `thickness` 高的实线（颜色取 `OutlineVariant`）。
fn line<'a, M: 'a>(ui: &'a Ui) -> Element<'a, M> {
    hairline(ui.color(Role::OutlineVariant), thickness(ui))
}

/// 一条给定颜色、给定厚度的**1px 级细线**（`snap` 关掉的自绘 quad）。
///
/// 只给「本身就是一条线」的用法（本模块的 `full` / `list`）。它不是通用描边——容器的 `Border`
/// 仍然是卡片描边的正确画法（大容器上描边宽度不会被取整，三档恒 1.00）。
/// `inputs::search` 的面板内分隔是同一形态的 1px 填充盒，但它走自己的 `container`（`Role::Outline`，
/// 20.8 的另一条线），本 mission 未改，见 `DevDocs/artifacts/m454/README.md` §6。
pub fn hairline<'a, M: 'a>(color: Color, height: f32) -> Element<'a, M> {
    Element::new(Hairline { color, height })
}

/// 细线本体：只有一个原子（宽 `Fill` × 高 `height`），画一个 `snap: false` 的实心 quad。
struct Hairline {
    color: Color,
    height: f32,
}

/// 细线的 quad。**`snap` 必须显式写 `false`**：`Quad::default()` 在 `crisp` feature 下是
/// `true`，改回 `..Quad::default()` 就是 M454 那个缺陷的复现（用例
/// `hairline_quad_opts_out_of_crisp_snapping` 就是挡这个的）。
fn quad(bounds: Rectangle, color: Color) -> (Quad, Background) {
    (
        Quad {
            bounds,
            snap: false,
            ..Quad::default()
        },
        Background::Color(color),
    )
}

impl<M, Theme, Renderer> Widget<M, Theme, Renderer> for Hairline
where
    Renderer: iced::advanced::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fill,
            height: Length::Fixed(self.height),
        }
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, Length::Fill, Length::Fixed(self.height))
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &iced::advanced::renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let (quad, background) = quad(layout.bounds(), self.color);
        renderer.fill_quad(quad, background);
    }
}

/// 线厚（= `LayoutMetrics::divider_thickness` = `stroke::HAIRLINE`，三者同一真源）。
pub fn thickness(ui: &Ui) -> f32 {
    debug_assert_eq!(
        ui.metrics().divider_thickness,
        STROKE_PX,
        "分隔线厚度必须是一像素线的唯一真源"
    );
    ui.metrics().divider_thickness
}

#[cfg(test)]
mod tests {
    use super::*;
    use unlucky_design::color::SchemeFlavor;
    use unlucky_design::typography::Platform;

    use crate::theme::Families;

    fn ui() -> Ui {
        Ui::with_platform(
            SchemeFlavor::Light,
            Platform::Windows,
            Families {
                text: "Segoe UI",
                display: "Segoe UI",
                mono: "Consolas",
            },
        )
    }

    /// 22.1：厚度 1（不是 2，也不是 1.5）。
    #[test]
    fn thickness_is_exactly_one_pixel() {
        let ui = ui();
        assert_eq!(thickness(&ui), 1.0);
        assert_eq!(thickness(&ui), STROKE_PX);
        assert_eq!(thickness(&ui), ui.metrics().divider_thickness);
    }

    /// 22.3：列表内缩进 16，来自 `Space::X4` 而不是字面量。
    #[test]
    fn list_inset_comes_from_the_spacing_token() {
        let ui = ui();
        assert_eq!(ui.space(Space::X4), 16.0, "ListTokens.DividerLeadingSpace");
    }

    /// M454：线的 quad **必须**关掉 `crisp` 吸附。
    ///
    /// 挡的是「改回 `Quad::default()`」——在 `crisp` feature 下它 `snap: true`，wgpu 会把
    /// 1 逻辑 px 的盒子吸附成整物理行（125% ⇒ 1 行 0.80、150% ⇒ 2 行 1.33）。
    /// 这条断言在 CPU 后端也照跑（值是按画法构造的，与后端无关）。
    #[test]
    fn hairline_quad_opts_out_of_crisp_snapping() {
        let (quad, _) = quad(
            Rectangle {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 1.0,
            },
            Color::BLACK,
        );
        assert!(
            !quad.snap,
            "1px 线不得走 crisp 吸附：吸附把 quad 的两条边各自 round 到整物理像素，\
             125%/150% 上线的有效粗会变成 0.80 / 1.33（M454）"
        );
        assert_eq!(quad.bounds.height, 1.0, "线高就是厚度，不许在这里二次取整");
    }

    #[test]
    fn both_forms_build() {
        let ui = ui();
        let _: Element<'_, ()> = full(&ui);
        let _: Element<'_, ()> = list(&ui);
        let _: Element<'_, ()> = hairline(ui.color(Role::Outline), STROKE_PX);
    }
}
