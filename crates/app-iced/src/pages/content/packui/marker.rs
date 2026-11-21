//! 「这一段真的被画出来了」的自证状态（M333 P33-1）：把段名塞进 [`Tree::state`]。
//!
//! ## 为什么文字抓不住、结构抓得住
//!
//! iced 0.14 的 `Element` 不透出内容：`Element` 拿不到文案，`Widget::children()` 只给 [`Tree`]。
//! 文案本身则躺在**私有字段**里读不回来：`text` 其实**覆盖了** `tag()` / `state()`
//! （`iced_core-0.14.0/src/widget/text.rs`），它挂在树上的状态是
//! `iced_core::text::paragraph::Plain { raw, content: String }`，`content` 正是画出去的那串字——
//! 但两个字段都私有、也没有访问器 ⇒ 「段头那句话画出来了没」走真控件树还是读不回来。
//! **但结构读得回来**：`Tree::state` 是 iced 给控件挂自证状态的官方位置
//! （`container` / `text` 拿它存自己的缓存），把这个段落的标识作为 `state` 交出去——类型与字段
//! 都是本仓自己的，用例能 `downcast` ⇒ 顺着 `Tree::new(el.as_widget())` 走整棵树就能把
//! 「这一帧到底画了哪几段」读回来。
//!
//! 范式来源是 `app/view/toast.rs` 的 `CapsuleState`（M298）：它用同一手法钉住了
//! 「把 `view.rs` 里那行删掉即红」。这里抄的是那个法子，不是同一个类型。
//!
//! ## 抓得住 / 抓不住（说在明处）
//!
//! | 抓得住 | 抓不住 |
//! |---|---|
//! | 报告里**有没有**这一段、这一段挂了几条（`rows`）、有没有被 `push` 进结果块 | 段头那句**文案**（`text` 的状态是私有的 `paragraph::Plain`，没有访问器）、位置与像素（`DevDocs/artifacts/m333/` 的图守） |

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{tree, Tree};
use iced::advanced::{mouse, Clipboard, Shell, Widget};
use iced::{Element, Event, Length, Rectangle, Size, Vector};

use crate::pages::Message as PageMessage;

/// 处理器段的段名（[`SectionMark::name`] 的取值）。
pub(super) const PROCESSORS: &str = "processors";

/// 预检屏「要处理」那一段的段名（M341）。
pub(super) const PICK_WARNINGS: &str = "pick.warnings";

/// 预检屏「只是告知」那一段的段名（M341）。
pub(super) const PICK_NOTES: &str = "pick.notes";

/// 一段的自证标识：树里带着它 = 这一段真的被 `push` 进了结果块。
///
/// 只有用例读这两个字段（产品路径把状态交给 iced 就完事），所以非测试构建里显式放过 dead_code
/// ——与 `app/view/toast.rs` 的 `CapsuleState::texts` 同一条处置。
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SectionMark {
    /// 段名（[`PROCESSORS`]）。
    pub name: &'static str,
    /// 这一段挂了几条（处理器段 = 步骤数；0 = 链跑了但一步都没列，界面走 `empty` 那一句）。
    pub rows: usize,
}

/// 给一段挂上自证状态（`tag` 用状态类型，与 iced 的 `container` / `text` 同一手法）。
pub(super) fn tagged<'a>(
    content: Element<'a, PageMessage>,
    mark: SectionMark,
) -> Element<'a, PageMessage> {
    Element::new(Tagging { content, mark })
}

struct Tagging<'a, Theme, Renderer> {
    content: Element<'a, PageMessage, Theme, Renderer>,
    mark: SectionMark,
}

impl<Theme, Renderer> Widget<PageMessage, Theme, Renderer> for Tagging<'_, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<SectionMark>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(self.mark.clone())
    }

    fn children(&self) -> Vec<Tree> {
        self.content.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.state = self.state();
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
        self.content
            .as_widget()
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
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
        shell: &mut Shell<'_, PageMessage>,
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
    ) -> Option<iced::overlay::Element<'a, PageMessage, Theme, Renderer>> {
        self.content
            .as_widget_mut()
            .overlay(tree, layout, renderer, viewport, translation)
    }
}

/// 顺着**真控件树**把这一帧所有 `SectionMark` 收集出来（顺序 = 树里的先后 = 版面从上到下），
/// 每条带上这一段在树里**挂了几行**。
#[cfg(test)]
pub(super) fn painted(element: &Element<'_, PageMessage>) -> Vec<PaintedSection> {
    fn walk(tree: &Tree, out: &mut Vec<PaintedSection>) {
        if tree.tag == tree::Tag::of::<SectionMark>() {
            out.push(PaintedSection {
                mark: tree.state.downcast_ref::<SectionMark>().clone(),
                lines: tree.children.len(),
            });
        }
        for child in &tree.children {
            walk(child, out);
        }
    }
    let tree = Tree::new(element.as_widget());
    let mut out = Vec::new();
    walk(&tree, &mut out);
    out
}

/// 用例读回的一段：自证标识 + 这一段在树里**挂了几行**（元素的直接子节点数 = `column![..]` 的项数）。
///
/// `lines` 是「那一句到底画出来了没」唯一读得回来的结构证据：`rows == 0` 那一支（链跑了却零步骤）
/// 靠它——4 行 = 段头 / Java 行 / 汇总 /「没有步骤」那句；只挂 3 行就是那句没画，
/// 整段没进结果块则根本收集不到 [`PaintedSection`]。
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PaintedSection {
    /// 段的自证标识（段名 + 模型里的条数）。
    pub mark: SectionMark,
    /// 树里挂了几行（直接子节点数）。
    pub lines: usize,
}
