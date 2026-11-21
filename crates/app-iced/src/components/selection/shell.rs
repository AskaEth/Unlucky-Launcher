//! 交互外壳：`button` 的四态 → 内部自绘容器的样式；以及勾形的 SVG。
//!
//! ## 为什么需要 [`StateSource`] 这条「帧内通道」
//!
//! M3 的 Checkbox / Radio / Switch 状态层是**Ø40 的圆**，而命中区是 **48**
//! （`research/15` §18.5 / §18.9）——两者尺寸不同，所以状态层不可能就是 `button` 自己的
//! 矩形四角（`button` 只能给自己的 bounds 上色）。正确画法是「48 的透明按钮里放一个 40 的圆」，
//! 但这个圆的颜色要跟着**按钮的状态**走，而 iced 的 `container` 样式闭包只有 `&Theme`、拿不到
//! `button::Status`。
//!
//! 所以这里留一个帧内通道：`button` 的样式闭包先跑（父先于子绘制，`iced_widget::button::draw`
//! 第 382 行 `fill_quad` 在 `content.draw` 之前），把状态写进 [`StateSource`]；子容器的样式闭包
//! 随后读到**本帧**的状态。α 仍然只从 [`super::super::interaction`] 取，这里不做任何颜色算术。
//!
//! 取证（gallery 状态矩阵）时用 [`StateSource::forced`] 直接钉住某态，通道根本不参与。

use std::cell::Cell;
use std::rc::Rc;

use iced::widget::{button, svg};
use iced::{Color, Element, Length, Padding};

use super::super::interaction::Interaction;
use super::geometry::ControlGeometry;

/// 当前视觉态的取值口：**外部钉住**（取证）或**跟随 `button` 的实况**（运行期）。
#[derive(Debug, Clone)]
pub struct StateSource {
    forced: Option<Interaction>,
    live: Option<Rc<Cell<Interaction>>>,
    enabled: bool,
}

impl StateSource {
    /// `forced = Some(..)`：钉住某态（gallery 用）；`None`：跟随 `button`。
    ///
    /// `enabled = false`（没有消息 = 禁用）时直接返回 [`Interaction::Disabled`]：iced 的
    /// `Button::status` 在**首帧**还是 `None`（→ 按 `Disabled` 画），靠它判断禁用会让首帧
    /// 画错色，所以禁用这件事由调用方给定，通道只管 hover/pressed。
    pub fn new(forced: Option<Interaction>, enabled: bool) -> Self {
        Self {
            forced,
            live: (forced.is_none() && enabled).then(|| Rc::new(Cell::new(Interaction::Enabled))),
            enabled,
        }
    }

    /// 本帧要画的态。
    pub fn get(&self) -> Interaction {
        if let Some(forced) = self.forced {
            return forced;
        }
        if !self.enabled {
            return Interaction::Disabled;
        }
        self.live
            .as_ref()
            .map_or(Interaction::Enabled, |live| live.get())
    }

    /// `button` 的样式闭包里调（把实况写进通道）。
    fn record(&self, status: button::Status) {
        if let (Some(live), true) = (&self.live, self.enabled) {
            live.set(Interaction::from_status(status));
        }
    }

    /// 一个 `container::Style` 闭包：每次绘制按「本帧的态」求值。
    pub fn container_style<'a, F>(
        &self,
        build: F,
    ) -> impl Fn(&iced::Theme) -> iced::widget::container::Style + 'a
    where
        F: Fn(Interaction) -> iced::widget::container::Style + 'a,
    {
        let source = self.clone();
        move |_theme| build(source.get())
    }
}

/// 透明的交互外壳：尺寸由调用方给定，视觉**全部**由内部容器负责。
///
/// 交互外壳自己不画任何东西——它有且只有一个职责：把 48 的命中区交出去，并把
/// `button` 的状态写进 [`StateSource`]。
pub(super) fn shell<'a, M: Clone + 'a>(
    source: &StateSource,
    width: Length,
    height: Length,
    on_press: Option<M>,
    content: Element<'a, M>,
) -> Element<'a, M> {
    let recorder = source.clone();
    let element = button(content)
        .width(width)
        .height(height)
        .padding(Padding::default())
        .style(move |_theme, status| {
            recorder.record(status);
            button::Style {
                background: None,
                text_color: Color::TRANSPARENT,
                ..button::Style::default()
            }
        });

    match on_press {
        Some(message) => element.on_press(message).into(),
        None => element.into(),
    }
}

/// 勾形 / 短横的 SVG（`research/15` §18.4 / §18.6）。
///
/// 为什么用 SVG 而不是自绘：勾形是**斜线**，iced 的 `fill_quad` 只能画轴对齐矩形，而
/// `canvas` feature 没开（`crates/app-iced/Cargo.toml` 不在本 mission 的 scope）。
/// `stroke-linecap="square"` 与 Compose 的 `StrokeCap.Square` 同义；路径比例
/// （0.2 / 0.4 / 0.5 / 0.7 / 0.8）是 `research/15` §25 明确允许的组件级常量。
///
/// `indeterminate` = 三点共线于 `y = 0.5`（`crossCenterGravitation = 1`）→ **形变**成一条居中短横，
/// **不新增图标**（§18.6）。
pub(super) fn glyph<'a, M: 'a>(
    geometry: ControlGeometry,
    indeterminate: bool,
    ink: Color,
) -> Element<'a, M> {
    let side = geometry.side;
    let point = |x: f32, y: f32| (side * x, side * y);
    let path = if indeterminate {
        // 三个控制点被拉到同一条水平线（勾形消失，只剩短横）
        let (x0, y0) = point(0.2, 0.5);
        let (x1, y1) = point(0.8, 0.5);
        format!("M {x0:.2} {y0:.2} L {x1:.2} {y1:.2}")
    } else {
        let (x0, y0) = point(0.2, 0.5);
        let (x1, y1) = point(0.4, 0.7);
        let (x2, y2) = point(0.8, 0.3);
        format!("M {x0:.2} {y0:.2} L {x1:.2} {y1:.2} L {x2:.2} {y2:.2}")
    };
    let source = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{side}\" height=\"{side}\" \
         viewBox=\"0 0 {side} {side}\"><path d=\"{path}\" fill=\"none\" stroke=\"currentColor\" \
         stroke-width=\"{}\" stroke-linecap=\"square\" stroke-linejoin=\"miter\"/></svg>",
        geometry.stroke
    );

    svg(svg::Handle::from_memory(source.into_bytes()))
        .width(Length::Fixed(side))
        .height(Length::Fixed(side))
        .style(move |_theme, _status| svg::Style { color: Some(ink) })
        .into()
}

/// 一个固定边长的正方形（内部再居中）：Q9 的纪律，字形一律锁在盒子里。
pub(super) fn square<'a, M: 'a>(side: f32, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    super::super::layout::centered_fixed(side, side, content)
}
