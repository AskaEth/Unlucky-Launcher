//! 滑块（`research/12b` §11，A1）：**自绘 `Widget`**——M3 Expressive 的滑块是「竖条」，不是圆点。
//!
//! ## 参考值（`[源]` = A1，`SliderTokens`）
//!
//! | # | 检查项 | 参考值 | 落点 |
//! |---|---|---|---|
//! | 11.1 | 轨道高 | **16**，圆角 full（= 8） | `TRACK_HEIGHT`（G18） |
//! | 11.2 | handle 形态 | **4 宽 × 44 高**，圆角 full；focus / pressed 收窄到 **2** | `HANDLE_WIDTH` / `HANDLE_HEIGHT` |
//! | 11.3 | 颜色 | 激活轨 `Primary`、未激活轨 `SecondaryContainer`、handle `Primary` | `Role::*` |
//! | 11.4 | handle 与轨道 | 纵向溢出：上下各 **14**（(44−16)/2） | 推导 |
//! | 11.5 | 命中高度 | **48**（`LM.touch_target`；竖条 44 + 上下各 2） | `SliderGeometry::height` |
//! | 11.6 | 禁用 | 激活轨 `OnSurface`@**38%**、未激活 @**12%**、handle @**38%** | `interaction::*` |
//! | — | 离散停靠点 | **4** 圆点 `SecondaryContainer`（`StopIndicatorSize`）；尾随间距 6 | [`Slider::steps`] |
//!
//! ## 为什么必须自绘
//!
//! iced 内置 `slider` 只能画「轨道 + 圆形/矩形 handle（高度 = 控件高）」，
//! **没有停靠点、没有禁用态、无法强制取证态**；而 §11.2 的 4×44 竖条要求 handle 高（44）
//! 与轨道高（16）分离。所以这里用 `advanced::Widget` 自绘：布局只有「宽 × 48」一个原子，
//! 轨道/激活段/停靠点/竖条全部用 `fill_quad` 画进同一个 bounds——几何因此在像素级可断言。
//!
//! ## 状态层（[估]，登记 D16）
//!
//! `SliderTokens` **没有**状态层直径（M3 Expressive 去掉了旧版的水滴 halo），hover/pressed 只体现为
//! handle 宽度与颜色。为了让五态在验收图上可辨且 α 只认令牌，这里把**通用状态层**
//! （`interaction::overlay_color`：hover 8% / focus 10% / pressed 10%）叠在 handle 上，
//! 宽度仍按 A1 收窄——两件事都来自令牌，没有新数值。
//!
//! ## 拖动
//!
//! `on_change` 有值就启用拖动与点击跳转；`None` = 禁用（也不吃事件）。
//! 值由外部持有（受控组件），拖动只 `publish` 新比例：组件不持有业务状态（契约 §2）。

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::Quad;
use iced::advanced::widget::{tree, Tree};
use iced::advanced::{mouse, Clipboard, Shell, Widget};
use iced::{Background, Border, Color, Element, Event, Length, Rectangle, Size};

use unlucky_design::color::Role;

use crate::theme::Ui;

use super::super::interaction::{self, Interaction, Tracker};
use super::geometry::{SliderGeometry, HANDLE_HEIGHT, HANDLE_WIDTH_ACTIVE, STOP_GAP, STOP_SIZE};

mod track;

pub use track::TrackGeometry;

use track::{fraction_at, quantize, EPSILON};

/// 一个滑块（受控：`fraction` 由外部给，变化经 `on_change` 回传）。
pub struct Slider<'a, M> {
    ui: &'a Ui,
    fraction: f32,
    steps: Option<u32>,
    on_change: Option<Box<dyn Fn(f32) -> M + 'a>>,
    forced: Option<Interaction>,
}

impl<'a, M> Slider<'a, M> {
    /// 连续值滑块（0.0 ~= 1.0）。
    pub fn new(ui: &'a Ui, fraction: f32) -> Self {
        Self {
            ui,
            fraction: fraction.clamp(0.0, 1.0),
            steps: None,
            on_change: None,
            forced: None,
        }
    }

    /// 离散值：`steps` = 分段数（4 → 5 个停靠点 0 / 25 / 50 / 75 / 100%）。
    pub fn steps(mut self, steps: u32) -> Self {
        self.steps = Some(steps.max(1));
        self.fraction = quantize(self.fraction, self.steps);
        self
    }

    /// 值变化回调；不给 = 禁用。
    pub fn on_change(mut self, on_change: impl Fn(f32) -> M + 'a) -> Self {
        self.on_change = Some(Box::new(on_change));
        self
    }

    /// 强制视觉态（gallery 状态矩阵取证）。
    pub fn state(mut self, state: Interaction) -> Self {
        self.forced = Some(state);
        self
    }

    /// 当前比例（量化后）。
    fn value(&self) -> f32 {
        quantize(self.fraction, self.steps)
    }

    /// 禁用与否：没有回调就是禁用（与按钮 `on_press: Option<M>` 同一口径）。
    fn enabled(&self) -> bool {
        self.on_change.is_some() && !matches!(self.forced, Some(Interaction::Disabled))
    }
}

/// `Widget` 的状态：五态状态机（[`Tracker`]）+ 是否正在拖动。
#[derive(Debug, Default)]
struct State {
    tracker: Tracker,
    dragging: bool,
}

/// 一个圆角实心块（轨道的所有部件都是它）。
fn quad(bounds: Rectangle, radius: f32, color: Color) -> (Quad, Background) {
    (
        Quad {
            bounds,
            border: Border {
                radius: radius.into(),
                ..Border::default()
            },
            ..Quad::default()
        },
        Background::Color(color),
    )
}

impl<'a, M, Theme, Renderer> Widget<M, Theme, Renderer> for Slider<'a, M>
where
    M: Clone,
    Renderer: iced::advanced::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fill,
            height: Length::Fixed(SliderGeometry::of(self.ui).height),
        }
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            tracker: Tracker::new(self.enabled()),
            dragging: false,
        })
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(
            limits,
            Length::Fill,
            Length::Fixed(SliderGeometry::of(self.ui).height),
        )
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        _viewport: &Rectangle,
    ) {
        if self.forced.is_some() {
            return;
        }

        let enabled = self.enabled();
        let geometry = SliderGeometry::of(self.ui);
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<State>();
        if state.tracker.set_enabled(enabled) {
            shell.request_redraw();
        }

        let mut target: Option<f32> = None;
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if enabled && cursor.is_over(bounds) =>
            {
                if let Some(position) = cursor.position() {
                    state.dragging = true;
                    target = Some(fraction_at(bounds, position.x, geometry.handle));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if state.dragging => {
                if let Some(position) = cursor.position() {
                    target = Some(fraction_at(bounds, position.x, geometry.handle));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.dragging = false;
            }
            _ => {}
        }

        if let Some(fraction) = target {
            let next = quantize(fraction, self.steps);
            if (next - self.value()).abs() >= EPSILON {
                if let Some(on_change) = &self.on_change {
                    self.fraction = next;
                    shell.publish(on_change(next));
                }
            }
        }

        if state.tracker.event(event, cursor.is_over(bounds)) {
            shell.request_redraw();
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &iced::advanced::renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let ui = self.ui;
        let geometry = SliderGeometry::of(ui);
        let bounds = layout.bounds();
        let state = self
            .forced
            .unwrap_or_else(|| tree.state.downcast_ref::<State>().tracker.state());
        let disabled = state.is_disabled();

        let (active_color, inactive_color, handle_color, handle_width) = if disabled {
            (
                interaction::disabled_content_over_role(ui, Role::OnSurface, false),
                interaction::disabled_colors(ui, Role::Surface).0,
                interaction::disabled_content_over_role(ui, Role::OnSurface, false),
                geometry.handle,
            )
        } else {
            // 状态层只叠在 handle 上（D16）：α 全部来自 interaction（hover 8 / focus 10 / pressed 10）
            let handle_color =
                interaction::overlay_color(ui, state, Role::OnSurface, Role::Primary)
                    .unwrap_or_else(|| ui.color(Role::Primary));
            let handle_width = match state {
                Interaction::Focused | Interaction::Pressed => HANDLE_WIDTH_ACTIVE,
                _ => geometry.handle,
            };
            (
                ui.color(Role::Primary),
                ui.color(Role::SecondaryContainer),
                handle_color,
                handle_width,
            )
        };

        let centre_y = bounds.y + bounds.height / 2.0;
        let track_x = bounds.x;
        let track_w = bounds.width;
        let track = TrackGeometry::of(bounds, self.value());
        let centre_x = track.handle_centre();
        let track_rect = |x: f32, width: f32| Rectangle {
            x,
            y: centre_y - geometry.track / 2.0,
            width: width.max(0.0),
            height: geometry.track,
        };

        // 离散停靠点：A1 的 `StopIndicatorColor = SecondaryContainer` 与未激活轨**同色**，
        // 所以停靠点只有在「未激活轨在它两侧留出空隙」时才读得出来（官方也是这么画的：
        // `StopIndicatorSize = 4` + `StopIndicatorTrailingSpace = 6`）。空隙两侧各留半个点 + 6。
        // **不含两端**：0% 与 100% 上不画停靠点（那是轨道自己的圆角端，官方也不画）
        let stops: Vec<f32> = match self.steps {
            Some(steps) if steps >= 2 => (1..steps)
                .map(|index| TrackGeometry::of(bounds, index as f32 / steps as f32).handle_centre())
                .collect(),
            _ => Vec::new(),
        };
        let gap = STOP_SIZE / 2.0 + STOP_GAP;

        // 未激活轨：从**竖条中心**到右端，按停靠点切成段（每段自己圆角，端点读数更清楚）
        let mut cursor = centre_x;
        let mut segments: Vec<(f32, f32)> = Vec::new();
        for anchor in stops.iter().copied() {
            if anchor <= cursor {
                continue;
            }
            segments.push((cursor, anchor - gap - cursor));
            cursor = anchor + gap;
        }
        if track_x + track_w > cursor {
            segments.push((cursor, track_x + track_w - cursor));
        }
        for (x, width) in segments {
            if width <= 0.0 {
                continue;
            }
            let (inactive, inactive_bg) =
                quad(track_rect(x, width), geometry.track / 2.0, inactive_color);
            renderer.fill_quad(inactive, inactive_bg);
        }

        // 激活轨：从容器左缘到**竖条中心**（与官方一致：激活段托在竖条下面）
        let active_w = track.active_width;
        if active_w > 0.0 {
            let (active, active_bg) = quad(
                track_rect(track_x, active_w),
                geometry.track / 2.0,
                active_color,
            );
            renderer.fill_quad(active, active_bg);
        }

        // 停靠点本体：4dp 圆点，只画在**未激活侧**（激活段已经走过的不重复画）
        if !disabled {
            for anchor in stops
                .iter()
                .copied()
                .filter(|anchor| *anchor > centre_x + EPSILON)
            {
                let (stop, stop_bg) = quad(
                    Rectangle {
                        x: anchor - STOP_SIZE / 2.0,
                        y: centre_y - STOP_SIZE / 2.0,
                        width: STOP_SIZE,
                        height: STOP_SIZE,
                    },
                    STOP_SIZE / 2.0,
                    inactive_color,
                );
                renderer.fill_quad(stop, stop_bg);
            }
        }

        // 竖条：4×44（focus / pressed 收窄到 2），圆角 full
        let (bar, bar_bg) = quad(
            Rectangle {
                x: track.handle_centre() - handle_width / 2.0,
                y: centre_y - HANDLE_HEIGHT / 2.0,
                width: handle_width,
                height: HANDLE_HEIGHT,
            },
            handle_width / 2.0,
            handle_color,
        );
        renderer.fill_quad(bar, bar_bg);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if !self.enabled() {
            return mouse::Interaction::None;
        }
        if tree.state.downcast_ref::<State>().dragging {
            return mouse::Interaction::Grabbing;
        }
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::None
        }
    }
}

impl<'a, M, Theme, Renderer> From<Slider<'a, M>> for Element<'a, M, Theme, Renderer>
where
    M: Clone + 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    fn from(slider: Slider<'a, M>) -> Self {
        Element::new(slider)
    }
}

/// 便捷入口：把 [`Slider`] 直接变成 `Element`（与其它组件的函数式签名一致）。
pub fn slider<'a, M: Clone + 'a>(spec: Slider<'a, M>) -> Element<'a, M> {
    Element::new(spec)
}

#[cfg(test)]
mod tests;
