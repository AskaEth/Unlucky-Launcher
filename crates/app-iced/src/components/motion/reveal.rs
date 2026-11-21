//! **纵向裁切容器**（M305）：占的格子高可以逐帧变，**内容排版高不变**。
//!
//! ## 为什么需要它（与 `clip.rs` 是同一课，换了一个轴）
//!
//! 列表增删要「高度也一起收/放」：不收，退场行删掉的那一帧下面所有行整体上移一格 = **整表瞬间
//! 重排闪跳**（任务里点名禁止）；收，就必须让一行的高从 44 走到 0。而直接把 44 喂给行内容会让
//! 它按「半行高」重新排版（文字折行 / 尾部按钮挤掉），那 150ms 里行内会「沸腾」——`clip.rs`
//! 的模块文档记的正是这件事（M257 的取证图里空态文案在过渡中折成两行）。
//!
//! 所以两件事拆开：
//!
//! | 谁 | 用多大的高 |
//! |---|---|
//! | 子内容的**排版** | `content_height`（整段过渡里不变 ⇒ 不重排） |
//! | 自己占的**格子**（= 裁切框） | `visible_height`（逐帧变，由行的可见度给） |
//!
//! ## 绘制：裁切 + **一层与画布同色的收尾遮罩**
//!
//! 裁切走 `Renderer::with_layer(bounds)`（tiny-skia 是 `push_clip`，wgpu 是同族的层栈）：超出的
//! 部分画不出来——「收起到 0」在像素上就是「一格都不占」。
//!
//! 淡出走 [`revealed`] 的 `veil` 参数：一层**与脚下画布同色**（`MaterialLayer::Canvas`）的 quad，
//! `α = 1 − 可见度`，**且必须自成一层**。为什么是 α 而不是预合成、为什么必须自成一层：
//!
//! | 问题 | 答案 |
//! |---|---|
//! | 为什么不用预合成（`with_alpha(α).over(底)`，D27 的正统口径） | 预合成要先知道被盖住的像素，而「一行的内容」在 `Element` 里是黑盒；替代方案（把整份 `Scheme` 预合成后传一个 `Ui` 下去）要求那个 `Ui` 活得比 `Element<'a>` 久——视图函数里造不出来，泄漏成 `'static` 则是每帧每行泄漏一份 49 色方案 |
//! | 为什么必须 `with_layer` | tiny-skia 的绘制按「四边形 → 三角 → 图 → 文字」分批（`iced_tiny_skia/src/lib.rs::present`），同层里文字永远排在四边形之后。第一版把遮罩塞进当前层：红色 α=0.3 的遮罩实测**一个文字像素都没盖住**（整图 0 个偏红像素）；改成子层后同一实验 422 个偏红像素、正好落在动的那两行（y 262..307） |
//! | 静息帧 | `veil = None` ⇒ **一个 quad 都不画**，与改造前逐像素相同（出图基线与 M305 之前同源） |
//! | 口径归属 | 这是全仓**唯一一处「动画中间帧」的 α**：只有退场/入场那 150~200ms 里出现，静息帧严格不画。规则的真源写在 `motion/fade.rs` 的模块文档，那里同时登记了两处**静态**的 α（不在本文口径内）：`options/fade.rs::viewport_fade` 的常驻渐变、`ui.layer()` 的 `panel_alpha`（GPU 档开背景图时）。`interaction/mod.rs` 的 α 守卫只扫状态层取色、**扫不到**这里（不经过任何 `Style`）——盲区已登记在 `fade.rs` |
//!
//! ## 与 `clip.rs` 的区别（为什么不再写一个通用件）
//!
//! | | `clip.rs` | 本文件 |
//! |---|---|---|
//! | 轴 | 横向（宽度逐帧变） | 纵向（高度逐帧变） |
//! | 内容锚点 | **右**端对齐（右栗贴窗口右缘） | **上**端对齐 + 可给一个纵向偏移 |
//! | 子内容排版宽 | 显式 `content_width` | 吃父级给的最大宽（行本来就是整宽） |
//!
//! 两处的布局约束方向不同（一个是宽、一个是高），合成一个带 `enum Axis` 的控件会让两个调用点
//! 都得读对方的注释才知道自己在传什么——分开写反而更短。
//!
//! ## 已知局限（与 `clip.rs` 同一条，登记在报告里）
//!
//! **命中测试用排版矩形**：过渡期间一行的可点区域是它的**排版**高（内容高），不是裁切后的可视高。
//! 只在 150~200ms 内成立；末帧 `visible_height == content_height` 且偏移为 0 时两者重合，
//! 逐像素与静息相同。

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{tree, Tree};
use iced::advanced::{mouse, Clipboard, Shell, Widget};
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector};

/// 把 `content` 按 `content_height` 排版、按 `visible_height` 裁切显示（内容**上端对齐**）。
///
/// `offset_y` 是内容在裁切框里的纵向位移（正 = 向下）：入场从下方浮上来时给正值，
/// 退场上卷时给负值——两端都是 `0`（末帧与静息逐像素相同）。
pub fn revealed<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
    visible_height: f32,
    content_height: f32,
    offset_y: f32,
    veil: Option<iced::Color>,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    Element::new(Revealed {
        content: content.into(),
        visible_height,
        content_height,
        offset_y,
        veil,
    })
}

struct Revealed<'a, Message, Theme, Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    /// 这一帧占的格子高（= 裁切框高）。
    visible_height: f32,
    /// 内容排版高（整段过渡里不变）。
    content_height: f32,
    /// 内容在裁切框里的纵向位移。
    offset_y: f32,
    /// 收尾遮罩（与底色同色；`None` = 静息，不画）。见模块文档「淡出」。
    veil: Option<iced::Color>,
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Revealed<'_, Message, Theme, Renderer>
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
        // 排版高**只**认 `content_height`：父级给的 max 高不参与（那正是「半行高重排」的来源）。
        let child_limits = layout::Limits::new(
            Size::ZERO,
            Size::new(limits.max().width, self.content_height),
        );
        let node = self
            .content
            .as_widget_mut()
            .layout(tree, renderer, &child_limits);
        let width = if limits.max().width.is_finite() {
            limits.max().width
        } else {
            node.size().width
        };
        layout::Node::with_children(
            Size::new(width, self.visible_height.max(0.0)),
            vec![node.move_to(Point::new(0.0, self.offset_y))],
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
        // `with_layer` 在两个后端都裁到 `bounds`（见模块文档）。
        renderer.with_layer(bounds, |renderer| {
            content.draw(tree, renderer, theme, style, child, cursor, viewport);
        });
        if let Some(veil) = self.veil.filter(|veil| veil.a > 0.0) {
            // **必须自成一层**：tiny-skia 的绘制按「四边形 → 三角 → 图 → 文字」分批（`lib.rs` 的
            // `present`），同一层里文字永远排在四边形之后 ⇒ 直接把遮罩塞进当前层的话，
            // 它只盖得住底色，一个字都盖不住（M305 实测：红色 α=0.3 的遮罩，文字像素一像素没变）。
            // 换成子层之后，子层的批排在父层文字**之后** ⇒ 真的盖住行内容。
            renderer.with_layer(bounds, |renderer| {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds,
                        ..renderer::Quad::default()
                    },
                    iced::Background::Color(veil),
                );
            });
        }
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
        // 收起到底（一格都不占）时不再收事件：退场行不该抢到一个已经点不到的按钮。
        if self.visible_height < f32::EPSILON {
            return;
        }
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
        if self.visible_height < f32::EPSILON {
            return mouse::Interaction::None;
        }
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
