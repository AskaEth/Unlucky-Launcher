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

use iced::widget::{container, space};
use iced::{Background, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::spacing::Space;

use crate::theme::{Ui, STROKE_PX};

use super::layout;

/// 通栏分隔线（区块分隔：0 缩进）。
pub fn full<'a, M: 'a>(ui: &'a Ui) -> Element<'a, M> {
    line(ui)
}

/// 列表内分隔线：左缩进 `Space::X4`（16），右侧到底（`ListTokens.DividerLeadingSpace`）。
///
/// 右缩进按官方也是 16，但列表行本身已有 16 的内边距时再缩一次会变成 32——
/// 本项目把「行内边距」交给行，本函数只负责**线相对行**的缩进，故右端通到行尾。
pub fn list<'a, M: 'a>(ui: &'a Ui) -> Element<'a, M> {
    layout::inset_start(ui.space(Space::X4), line(ui))
}

/// 一条 1px 的横线（厚度/颜色都只有一处定义）。
fn line<'a, M: 'a>(ui: &'a Ui) -> Element<'a, M> {
    container(space::horizontal())
        .width(Length::Fill)
        .height(thickness(ui))
        .style(|_theme| container::Style {
            background: Some(Background::Color(ui.color(Role::OutlineVariant))),
            ..container::Style::default()
        })
        .into()
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

    #[test]
    fn both_forms_build() {
        let ui = ui();
        let _: Element<'_, ()> = full(&ui);
        let _: Element<'_, ()> = list(&ui);
    }
}
