//! **hover / pressed 的取样控件**（M257 / A8）：把「状态变了」这件事搬到 `update` 侧。
//!
//! ## 为什么需要它（一句话）
//!
//! `button` 的 hover 状态只在 `style` 闭包里可见，而闭包在 **`draw`** 里跑——那已经在订阅表
//! 求值之后了。若在 draw 侧起跑过渡，「鼠标停住不动」的整段时间里**一帧都不会来**，状态层会永远
//! 停在 `progress = 0`（看不见）。本控件用 `Widget::update`（早于订阅表求值）把目标态写进
//! [`super::layer`]，于是同一拍 `needs_frames()` 就变真。
//!
//! ## 它做什么 / 不做什么
//!
//! | 做 | 不做 |
//! |---|---|
//! | 用 [`Tracker`] 把鼠标事件折成五态 | **不**消费事件（`shell` 的捕获位不动，按钮照常收到点击） |
//! | 改自己 `Tree` 状态里的 [`Layer`] 目标 | **不**改颜色、不改布局（颜色仍由 `style` 闭包求值） |
//! | `draw` 时把自己的层压到栈顶（[`layer::with_current`]） | **不**包装禁用控件（没有状态层可动） |
//!
//! ## 命中测试用的是**布局矩形**
//!
//! `cursor.is_over(layout.bounds())`：与 `button` 自己的判定同源（它也读布局矩形）。触摸板的
//! 「按下后拖出再拖回」在 `Tracker` 里按 `pressed` 跟随 `over` 折，与 iced 的 `button` 语义一致。
//!
//! ## 抓不住什么
//!
//! 覆盖在按钮**之上**的另一层（模态遮罩 / tooltip）不算：本控件只看自己的矩形，而 `button`
//! 的 hover 会被上层遮住的行为由 iced 的 overlay 决定。两者在这类遮挡下可能短暂不一致
//! （按钮自己认为被 hover、本控件也认为被 hover），但既然按钮被遮住就点不到，观感无差。

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{tree, Tree};
use iced::advanced::{mouse, Clipboard, Shell, Widget};
use iced::{Element, Event, Length, Rectangle, Size, Vector};

use super::layer::{self, Layer};
use super::Tracker;
use crate::app::components::motion;

/// 包一层「状态取样」：`enabled == false` 时**完全不跟踪**（禁用控件没有状态层）。
pub fn tracked<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
    enabled: bool,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    Element::new(Tracked {
        content: content.into(),
        enabled,
    })
}

/// 本控件自己的 `Tree` 状态：状态机 + 它那一条过渡。
#[derive(Debug)]
struct State {
    tracker: Tracker,
    layer: Layer,
}

impl Default for State {
    fn default() -> Self {
        Self {
            tracker: Tracker::new(true),
            layer: Layer::new(),
        }
    }
}

struct Tracked<'a, Message, Theme, Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    enabled: bool,
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Tracked<'_, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
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
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
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
        // 先让内容吃这条事件（按钮的点击 / 键盘都靠它），再做本控件自己的取样。
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );

        if !self.enabled {
            return;
        }

        let over = cursor.is_over(layout.bounds());
        let state = tree.state.downcast_mut::<State>();
        if !state.tracker.event(event, over) {
            return;
        }
        // 起跑**在这里**（update 侧）：同一拍 `needs_frames()` 就能看见它。
        if state.layer.retarget(state.tracker.state(), motion::scale()) {
            shell.request_redraw();
        }
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
        let state = tree.state.downcast_ref::<State>();
        let content = self.content.as_widget();
        // 状态颜色由 `style` 闭包求值，而闭包只拿得到 `&Theme` —— 所以「这一帧处于哪条过渡」
        // 只能经开画栈传给它们（见 `layer::with_current`）。
        layer::with_current(state.layer, || {
            content.draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                layout,
                cursor,
                viewport,
            );
        });
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
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'a>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<iced::overlay::Element<'a, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}
