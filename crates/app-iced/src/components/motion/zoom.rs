//! 缩放层（M256）：把一棵子树整体缩放（模态卡片进出场用）。
//!
//! ## 为什么自己写一个
//!
//! iced 0.14 的控件清单里**没有**通用的 transform / opacity 控件（`iced_widget` 只有 `stack` /
//! `opaque` / `float` / `pin`）。但渲染器层是有的：`Renderer::with_transformation` 在**两个后端
//! 都实现了**——`iced_tiny_skia-0.14.1/src/lib.rs:211-217` 与 `iced_wgpu-0.14.0/src/lib.rs:682-688`
//! 各是一行 `layers.push_transformation(...)`。而且 tiny_skia 对文字也乘了 `transformation`：
//! `layer.rs:73-96` 把字号、行高、bounds 都按 `scale_factor()` 缩过 ⇒ **文字是真缩放**，
//! 不是"只缩背景、字不动"。
//!
//! ## 静息态**逐像素不变**
//!
//! `scale == 1.0` 时直接 `content.draw(..)`，**不套**变换：出图基线（D30）不受这层影响，
//! 也不给渲染器多压一层矩阵。
//!
//! ## 已知局限（登记在报告里）
//!
//! 命中测试用的是**布局矩形**（未缩放），所以缩放途中的点击落点仍按 1.0 的框算。
//! 模态只在开/关那 150~200 ms 里缩放，而那两段本来就不该被点击（`app/update/dialog.rs`
//! 在退场期间直接吞掉弹窗消息），所以这件事没有可观察的后果。

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{tree, Tree};
use iced::advanced::{mouse, Clipboard, Shell, Widget};
use iced::{Element, Event, Length, Rectangle, Size, Transformation, Vector};

/// 把 `content` 缩放 `scale` 倍（**绕自身中心**），布局尺寸不变。
pub fn scaled<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
    scale: f32,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    Element::new(Scaled {
        content: content.into(),
        scale,
    })
}

/// 绕**中心**缩放的变换：`T(c) · S · T(-c)`。
///
/// `Mul` 的乘法序是**先右后左**（`impl Mul for Transformation` 用 `self.0 * rhs.0`，点按
/// `p' = M·p` 变换）⇒ 这个式子算出来是 `p → s(p − c) + c = s·p + (1−s)c`，**不动点正是 `c`**。
///
/// ⚠️ 上一次写反了：`T(-c) · S · T(c)` 会得到 `s·p − (1−s)c`，不动点在 `(−cx, −cy)` ——窗口外的
/// 镜像点，于是"缩放"变成了"从左上斜滑进来"（复审 r3 用真 iced_core 实跑抓到：card 中心
/// (700,450) 漂到 (644,414)）。`tests::about_center_keeps_the_center_fixed` 把这条钉住。
fn about_center(bounds: Rectangle, scale: f32) -> Transformation {
    let (cx, cy) = (
        bounds.x + bounds.width / 2.0,
        bounds.y + bounds.height / 2.0,
    );
    Transformation::translate(cx, cy)
        * Transformation::scale(scale)
        * Transformation::translate(-cx, -cy)
}

struct Scaled<'a, Message, Theme, Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    scale: f32,
}

impl<Message, Theme, Renderer> Scaled<'_, Message, Theme, Renderer> {
    /// 这条变换要不要真的套（`1.0` 时什么都不做）。
    fn transformation(&self, bounds: Rectangle) -> Option<Transformation> {
        // 0.1% 以下视为到位：亚像素缩放只会让文字发虚，没有观感收益。
        if (self.scale - 1.0).abs() < 1.0e-3 {
            return None;
        }
        Some(about_center(bounds, self.scale.max(0.01)))
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Scaled<'_, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> tree::Tag {
        self.content.as_widget().tag()
    }

    fn state(&self) -> tree::State {
        self.content.as_widget().state()
    }

    fn children(&self) -> Vec<Tree> {
        self.content.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
        self.content.as_widget().diff(tree);
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content.as_widget_mut().layout(tree, renderer, limits)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        // 先算变换（借 layout 的 bounds），再借用 content —— 两个借用互不重叠。
        let transformation = self.transformation(layout.bounds());
        let content = self.content.as_widget();

        match transformation {
            None => content.draw(tree, renderer, theme, style, layout, cursor, viewport),
            Some(transformation) => renderer.with_transformation(transformation, |renderer| {
                content.draw(tree, renderer, theme, style, layout, cursor, viewport);
            }),
        }
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn iced::advanced::widget::Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(tree, layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            tree, event, layout, cursor, renderer, clipboard, shell, viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content
            .as_widget()
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'a>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<iced::overlay::Element<'a, Message, Theme, Renderer>> {
        self.content
            .as_widget_mut()
            .overlay(tree, layout, renderer, viewport, translation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::{Point, Size};

    /// **绕中心缩放：中心不动、尺寸按倍率**（复审 r3 的 P1 就是这条从未被断言过）。
    ///
    /// 抓得住：①把两个 `translate` 乘反（上一版的 bug：不动点落在 `(−cx, −cy)`，窗口外的镜像点
    /// ⇒ 卡片从左上斜滑进来）——中心断言立刻以几十像素的偏差红；②倍率写错 / 漏乘（尺寸断言红）；
    /// ③把 `about_center` 改成纯缩放（不含平移）——同样红。
    /// 抓不住：**像素**。这里断言的是几何矩阵；两个渲染后端都按同一个 `Transformation` 画
    /// （`filled`/`text` 的 `bounds * transformation`），所以"几何等价 ⇒ 像素等价"成立的前提是
    /// **后端真的把矩阵乘上了**——那一条由真机截图看，不由本用例保证。
    #[test]
    fn about_center_keeps_the_center_fixed() {
        // 默认窗口 1180×760 里居中的卡片（DialogGeometry::max_width = 560）
        for (rect, scale) in [
            (
                Rectangle::new(Point::new(310.0, 220.0), Size::new(560.0, 320.0)),
                0.96_f32,
            ),
            (
                Rectangle::new(Point::new(310.0, 220.0), Size::new(560.0, 320.0)),
                0.5,
            ),
            (
                Rectangle::new(Point::new(0.0, 0.0), Size::new(384.0, 288.0)),
                1.04,
            ),
        ] {
            let matrix = about_center(rect, scale);
            let center = rect.center();
            let moved = center * matrix;
            assert!(
                (moved.x - center.x).abs() < 1.0e-3 && (moved.y - center.y).abs() < 1.0e-3,
                "scale={scale}: 中心必须不动，实测 ({}, {}) 应为 ({}, {})",
                moved.x,
                moved.y,
                center.x,
                center.y
            );

            let size = rect.size() * matrix;
            assert!(
                (size.width - rect.width * scale).abs() < 1.0e-3
                    && (size.height - rect.height * scale).abs() < 1.0e-3,
                "scale={scale}: 尺寸必须是 {scale} 倍，实测 {size:?}"
            );

            // 缩小时左上角必须往中心方向收（不是往外飞）
            if scale < 1.0 {
                let corner = rect.position() * matrix;
                assert!(
                    corner.x > rect.x && corner.x < center.x,
                    "scale={scale}: 左上角 x 应收向中心，实测 {}",
                    corner.x
                );
            }
        }
    }
}
