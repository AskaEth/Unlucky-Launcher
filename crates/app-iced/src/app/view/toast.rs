//! Toast 覆盖层（M298）：`state.toasts` 的**唯一渲染点**。
//!
//! ## 缺口有多真
//!
//! M297 盘查：`state.toasts` 全仓只被 **push**（安装取消 / 启动收尾 / mod 启停失败 / 复制成功）
//! 与 **expire**（`app/update/clock.rs`），`view.rs` 与 `pages/**` 对 `toasts` 零命中 ⇒ 提示一条
//! 都看不见。与「收尾失败静默不弹框」叠在一起，启动失败可以**完全无反馈**。
//!
//! ## 这一层画什么
//!
//! MD3E 胶囊：`InverseSurface` 底、`InverseOnSurface` 字、**`ShapeScale::Full`**（高的一半 ⇒
//! 真胶囊）、贴**底部居中**、多条自下而上叠（最新的那条在最下面，与推送顺序一致）。
//! 几何取组件层 [`SnackbarGeometry`]（高 52 / 横内距 16 / 竖 6 / 间距 8 / 外边距 12 / 上限宽 600），
//! 不在这里写第二份数字。**不画阴影**：D27（CPU 档禁模糊）+ 组件层 D-4 的同一口径。
//!
//! ## 进 / 出过渡：复用模态那一对腿，不新造弹簧
//!
//! 进度由 [`ToastTracks`] 在 **update 侧**采样
//! （toast 自己的出生时刻 + `remaining_ms`），这里只把进度换成缩放：`motion::modal_scale`
//! 与 [`motion::scaled`]。档位、缓动、缩放起点全是既有令牌（D31：适配层不新造数）。
//! 关掉动效时进度恒 `1.0` ⇒ 恒等变换、出图逐像素不变。
//!
//! ## 为什么这一层不吃点击
//!
//! 它是一层铺满窗口的 `container`（`Fill`/`Fill`）：iced 的 `Stack` 只在子控件
//! **捕获**事件时才停止向下派发，而普通容器不捕获 ⇒ 点到胶囊以外（以及胶囊本身）的按下照样
//! 落到下面的页面树。所以这一层不提供「点掉」交互：`ToastCenter::dismiss` 今天没有调用点，
//! 到期由 `clock.rs` 的 `expire` 统一处理（任务要求：过期仍走既有那条）。

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{tree, Tree};
use iced::advanced::{mouse, Clipboard, Shell, Widget};
use iced::widget::{container, row, text, Space};
use iced::{
    Alignment, Background, Border, Element, Event, Length, Padding, Rectangle, Size, Vector,
};

use unlucky_design::color::Role;
use unlucky_design::shape::ShapeScale;
use unlucky_design::typography::TypeRole;
use unlucky_presenter::toast::{Toast, ToastKind};

use crate::app::components::feedback::panel::SnackbarGeometry;
use crate::app::components::motion;
use crate::theme::Ui;

use super::super::message::Message;
use super::super::state::toast::ToastTracks;

/// 把 toast 覆盖层叠在 `base` 上（队列为空时原样返回 `base`）。
pub(super) fn layer<'a>(
    ui: &'a Ui,
    base: Element<'a, Message>,
    toasts: &'a [Toast],
    tracks: &'a ToastTracks,
) -> Element<'a, Message> {
    if toasts.is_empty() {
        return base;
    }
    let geometry = SnackbarGeometry::of(ui);
    // 推送顺序 = 出现顺序：最早的在上，最新的贴底（MD3E 的堆叠方向）。
    let mut column = iced::widget::Column::new()
        .spacing(geometry.gap)
        .align_x(Alignment::Center);
    for toast in toasts {
        column = column.push(capsule(ui, &geometry, toast, tracks.progress(toast.id)));
    }
    let plane = container(column)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding::from(geometry.margin))
        .align_x(Alignment::Center)
        .align_y(Alignment::End);
    let texts = toasts
        .iter()
        .map(|toast| toast.text.clone())
        .collect::<Vec<_>>();
    iced::widget::stack![base, tagged(plane.into(), texts)].into()
}

/// 一条胶囊：语义色的点 + 一行字（高度固定，字按词折行）。
fn capsule<'a>(
    ui: &'a Ui,
    geometry: &SnackbarGeometry,
    toast: &'a Toast,
    progress: f32,
) -> Element<'a, Message> {
    let height = geometry.height_for(1);
    let style = ui.style(TypeRole::BodyMedium);
    let surface = ui.color(Role::InverseSurface);
    // 语义色**只**落在前面那个点上：底与字必须保持 inverse 那一对（否则深色档对比度会掉）。
    let accent = match toast.kind {
        ToastKind::Info | ToastKind::Success => Role::InversePrimary,
        ToastKind::Warn | ToastKind::Error => Role::Error,
    };
    // 点的直径取组件层的 **间距档**（8），圆角 `Full` ⇒ 正圆；不另写一个点径数字。
    let dot_side = geometry.gap;
    let dot: Element<'a, Message> = container(
        Space::new()
            .width(Length::Fixed(dot_side))
            .height(Length::Fixed(dot_side)),
    )
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(Background::Color(ui.color(accent))),
        border: Border {
            radius: ui.radius(ShapeScale::Full, dot_side),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into();

    let body = row![
        dot,
        text(toast.text.as_str())
            .size(style.size_px)
            .font(ui.font_of(style))
            .color(ui.color(Role::InverseOnSurface))
            .width(Length::Fill),
    ]
    .align_y(Alignment::Center)
    .spacing(geometry.gap);

    // 边界（M445）：**不画 rim**。胶囊底是 `InverseSurface`（反色表面），按主题取色的 rim 落在这块面上
    // 不是「看不见」（GPU 档 −3 / +3）就是「过重」（CPU 档 +148 / −168）——判据与实测见
    // `components/rim.rs` 的「什么时候**不该**用 rim」。所以这里只留圆角，不调 `panel_border`：一个
    // 「看不出」或「过了头」的边框，不如没有（M444 的 before/after 图在 `DevDocs/artifacts/m444/`）。
    let bar: Element<'a, Message> = container(body)
        .width(Length::Fill)
        .max_width(geometry.max_width)
        .height(Length::Fixed(height))
        .padding(Padding {
            top: geometry.pad_v,
            right: geometry.pad_h,
            bottom: geometry.pad_v,
            left: geometry.pad_h,
        })
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(surface)),
            border: Border {
                radius: capsule_radius(ui, height),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into();
    motion::scaled(bar, motion::modal_scale(progress))
}

/// 胶囊圆角：`ShapeScale::Full` 对高求值（= 高的一半 ⇒ 真胶囊）。
///
/// 与组件层 snackbar 的 **16** 不是同一个口径（那是规格卡的矩形提示条）；MD3E 的 toast 是
/// 胶囊，任务点名要 `ShapeScale::Full`，所以这里按**高的一半**算，且只在这一处求值。
fn capsule_radius(ui: &Ui, height: f32) -> iced::border::Radius {
    ui.radius(ShapeScale::Full, height)
}

/// 覆盖层的**自证状态**：树里带着它 = 这些提示真的进了控件树。
///
/// 为什么需要它：iced 0.14 没有「把整棵树遍历成文本」的入口（`Element` 不透出内容、
/// `Widget::children()` 只给 [`Tree`]，而 `Tree` 不带文字）。所以这一层把自己的**文案清单**
/// 作为 [`Widget::state`] 交出去——用例顺着 `Tree::state` 就能读到「画出来的到底是哪几句」，
/// 而不是只看 `state.toasts.len()`（那证明不了覆盖层被接上）。
#[derive(Debug)]
pub(super) struct CapsuleState {
    /// 这一帧真正画出去的文案（顺序 = 自下而上的堆叠顺序）。
    ///
    /// 只有用例读它（产品路径把状态交给 iced 就完事），所以非测试构建里显式放过 dead_code。
    #[allow(dead_code)]
    pub texts: Vec<String>,
}

/// 给覆盖层挂上自证状态（`tag` 用状态类型，见 iced 的 `container` / `text` 同一手法）。
pub(super) fn tagged<'a>(
    content: Element<'a, Message>,
    texts: Vec<String>,
) -> Element<'a, Message> {
    Element::new(Tagging { content, texts })
}

struct Tagging<'a, Theme, Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    texts: Vec<String>,
}

impl<Theme, Renderer> Widget<Message, Theme, Renderer> for Tagging<'_, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<CapsuleState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(CapsuleState {
            texts: self.texts.clone(),
        })
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
mod tests;
