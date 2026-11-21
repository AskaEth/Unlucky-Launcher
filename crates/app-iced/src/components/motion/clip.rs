//! **裁切容器**（M257）：可见宽可以逐帧变，**内容排版宽不变**。
//!
//! ## 为什么不能直接 `container(..).clip(true)`
//!
//! iced 的 `container::layout` 把父级 `Limits` 的 **max** 一路传给子级，而 `Limits::resolve` 会把
//! `Length::Fixed(w)` 夹到那个 max 上。于是「外层宽逐帧变 + 内层固定 320」的实际排版宽是
//! `min(320, 这一帧的外层宽)`：正文每帧重新折行 ⇒ 折叠那 200ms 里文字会「沸腾」（M257 的
//! 第 02 张取证图就是这么暴露的：空态文案在过渡中折成两行）。
//!
//! 本控件把两件事拆开：
//!
//! | 谁 | 用什么宽 |
//! |---|---|
//! | 子内容的**排版** | `content_width`（目标宽，整段过渡里不变 ⇒ 不重排） |
//! | 自己占的**格子**（= 裁剪框） | `visible`（逐帧变，由折叠进度给） |
//!
//! 绘制走 `Renderer::with_layer(bounds)`（两个后端都实现：tiny-skia 是 `push_clip`，
//! wgpu 是 `push_transformation` 同族的层栈），所以内容超出 `visible` 的部分**画不出来**——
//! 这就是「不溢出」在像素上的落点，不需要半透明。
//!
//! ## 内容锚在**右边**
//!
//! 子节点被放在 `visible − content_width`（负数）处：右栗贴在窗口右缘，折叠时**右边不动、左边
//! 被裁**。于是图标条 / 面板头部的右端元素不位移（「折叠后图标位置不跳」的另一半也在这一句里）。
//!
//! ## 已知局限（与 `zoom.rs` 同类，登记在报告里）
//!
//! **命中测试用排版矩形**：过渡期间内容的可点区域比可见区域宽（多出来的那一段在左侧、看不见）。
//! 只在 `visible != content_width` 的 200ms 里成立，收尾（或展开到位）时两者相等、逐像素与静息
//! 相同。

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{tree, Tree};
use iced::advanced::{mouse, Clipboard, Shell, Widget};
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector};

/// 把 `content` 按 `content_width` 排版、按 `visible` 裁切显示（内容**右端对齐**）。
pub fn clipped<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
    visible: f32,
    content_width: f32,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    Element::new(Clipped {
        content: content.into(),
        visible,
        content_width,
    })
}

struct Clipped<'a, Message, Theme, Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    /// 这一帧占的格子宽（= 裁剪框宽）。
    visible: f32,
    /// 内容排版宽（整段过渡里不变）。
    content_width: f32,
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Clipped<'_, Message, Theme, Renderer>
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
        // 排版宽**只**认 `content_width`：父级 max 不参与（那正是 container 会夹住它的地方）。
        let child_limits = layout::Limits::new(
            Size::ZERO,
            Size::new(self.content_width, limits.max().height),
        );
        let node = self
            .content
            .as_widget_mut()
            .layout(tree, renderer, &child_limits);
        let size = node.size();

        // 右对齐：内容右端贴在裁剪框右缘。
        let dx = self.visible - size.width;
        layout::Node::with_children(
            Size::new(self.visible, size.height),
            vec![node.move_to(Point::new(dx, 0.0))],
        )
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
        let Some(child) = layout.children().next() else {
            return;
        };
        let bounds = layout.bounds();
        let content = self.content.as_widget();
        // `with_layer` 在两个后端都裁到 `bounds`（见模块文档的表）。
        renderer.with_layer(bounds, |renderer| {
            content.draw(tree, renderer, theme, style, child, cursor, viewport);
        });
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn iced::advanced::widget::Operation,
    ) {
        let Some(child) = layout.children().next() else {
            return;
        };
        self.content
            .as_widget_mut()
            .operate(tree, child, renderer, operation);
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
        let Some(child) = layout.children().next() else {
            return;
        };
        self.content.as_widget_mut().update(
            tree, event, child, cursor, renderer, clipboard, shell, viewport,
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
        let Some(child) = layout.children().next() else {
            return mouse::Interaction::None;
        };
        self.content
            .as_widget()
            .mouse_interaction(tree, child, cursor, viewport, renderer)
    }

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'a>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<iced::overlay::Element<'a, Message, Theme, Renderer>> {
        let child = layout.children().next()?;
        self.content
            .as_widget_mut()
            .overlay(tree, child, renderer, viewport, translation)
    }
}
