//! Switch（`research/12b` §10）：轨道 **52×32** / 圆角 full(16) / 未选中 2px `Outline` 描边 /
//! 滑块 **24·16**（按下 28，见偏离 D-SW1）/ 滑块内图标 16 / **Ø40 状态层**。
//!
//! ## 状态层为什么跟滑块走
//!
//! M3 的 switch 状态层是围着**滑块**的 Ø40 圆（`StateLayerSize = 40`），而轨道只有 32 高、
//! 52 宽、滑块最小 16——圆会同时超出轨道的上下与左右。所以绘制区比轨道大一圈：
//! 宽 = 52 + 2×4（[`SwitchGeometry::draw_width`]）、高 = 40，轨道在中间，滑块与状态层浮在上面。
//!
//! ## 偏离 D-SW1（登记在报告里）
//!
//! `PressedHandleWidth = 28` 的「按下胀开」**未做**：iced 的 `button` 只在事件时重绘，
//! 不会重建视图，而滑块尺寸是**布局量**（不是颜色）。要做它必须写自定义 `Widget`
//! （`interaction::Tracker` 那条路），本 mission 的时间盒留给四条线的像素断言与视觉证据。
//! 按下的可判定反馈仍在：状态层 α 从 8% 升到 10%（`StateLayer::Pressed`）。

use iced::widget::{container, row, text, Space as Spacer};
use iced::{Alignment, Element, Length, Padding};

use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::icons;
use crate::theme::Ui;

use super::super::interaction::{Base, Interaction};
use super::geometry::SwitchGeometry;
use super::paint::{self, Frame};
use super::shell::{self, StateSource};

/// 一个开关（轨道 + 滑块 + 可选标签；`on_toggle` 为 `None` = 禁用）。
#[derive(Debug, Clone)]
pub struct Switch<'a, M> {
    label: &'a str,
    value: bool,
    on_toggle: Option<M>,
    state: Option<Interaction>,
    /// 脚下那一层的底（M234，禁用态合成用）。
    base: Base,
}

impl<'a, M> Switch<'a, M> {
    /// 一个可切换的开关。
    pub fn new(label: &'a str, value: bool, on_toggle: M) -> Self {
        Self {
            label,
            value,
            on_toggle: Some(on_toggle),
            state: None,
            base: Base::CANVAS,
        }
    }

    /// 禁用的开关（无消息）。
    pub fn disabled(label: &'a str, value: bool) -> Self {
        Self {
            label,
            value,
            on_toggle: None,
            state: None,
            base: Base::CANVAS,
        }
    }

    /// 强制视觉态（gallery 取证用；运行期不要调）。
    pub fn state(mut self, state: Interaction) -> Self {
        self.state = Some(state);
        self
    }

    /// **告诉开关它踩在什么底上**（M234）：禁用态的 12% 轨道 / 38% 滑块叠在这个底上。
    pub fn base(mut self, base: impl Into<Base>) -> Self {
        self.base = base.into();
        self
    }
}

/// 画一个开关（轨道 + 滑块 + 标签）。
pub fn switch<'a, M: Clone + 'a>(ui: &'a Ui, spec: Switch<'a, M>) -> Element<'a, M> {
    let geometry = SwitchGeometry::of(ui);
    let selected = spec.value;
    let thumb_side = geometry.thumb(selected, false);
    let base = spec.base;
    let source = StateSource::new(spec.state, spec.on_toggle.is_some());

    // 滑块内的勾：颜色只随选中/禁用变（A1 的 `Selected*IconColor` 都是 `OnPrimaryContainer`），
    // 所以静态求值；未选中态官方图标色 = 轨道色（画了也看不见），因此不画。
    let at_rest = paint::switch(ui, base, selected, source.get());
    let glyph: Element<'a, M> = match at_rest.icon {
        Some(ink) => super::super::layout::centered_fixed(
            geometry.icon,
            geometry.icon,
            // 盒 16 ≠ RENDER_PX(24)：必须按 16 烘焙，否则线宽被布局压细（复审 r1 P2-2）。
            icons::view_at::<M>(IconKind::Check, ink, geometry.icon),
        ),
        None => Spacer::new().into(),
    };

    // 滑块（颜色跟态走：hover/pressed 是 `PrimaryContainer`）
    let thumb = container(glyph)
        .width(Length::Fixed(thumb_side))
        .height(Length::Fixed(thumb_side))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(source.container_style(move |state| {
            Frame {
                fill: Some(paint::switch(ui, base, selected, state).thumb),
                outline: None,
                stroke: 0.0,
                radius: thumb_side / 2.0,
            }
            .container_style()
        }));

    // Ø40 状态层（围着滑块，圆心与滑块同心）
    let layer_side = geometry.state_layer;
    let layer = container(thumb)
        .width(Length::Fixed(layer_side))
        .height(Length::Fixed(layer_side))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(source.container_style(move |state| {
            Frame {
                fill: paint::switch_layer(ui, state, selected),
                outline: None,
                stroke: 0.0,
                radius: layer_side / 2.0,
            }
            .container_style()
        }));

    // 轨道（52×32；未选中态有 2px `Outline` 描边）
    let track = container(Spacer::new())
        .width(Length::Fixed(geometry.track_width))
        .height(Length::Fixed(geometry.track_height))
        .style(source.container_style(move |state| {
            paint::switch(ui, base, selected, state)
                .track
                .container_style()
        }));

    // 绘制区 = 轨道 + 状态层外扩；两层叠放（层序：轨道在下，滑块与状态层在上）
    let draw_width = geometry.draw_width();
    let draw_height = geometry.draw_height();
    let area = iced::widget::stack![
        container(track)
            .width(Length::Fixed(draw_width))
            .height(Length::Fixed(draw_height))
            .padding(Padding {
                left: geometry.bleed(),
                ..Padding::default()
            })
            .align_y(Alignment::Center),
        container(layer)
            .width(Length::Fixed(draw_width))
            .height(Length::Fixed(draw_height))
            .padding(Padding {
                left: geometry.layer_offset(selected, thumb_side),
                ..Padding::default()
            }),
    ]
    .width(Length::Fixed(draw_width))
    .height(Length::Fixed(draw_height));

    let mut content = row![area]
        .align_y(Alignment::Center)
        .spacing(ui.space(Space::X3));
    if !spec.label.is_empty() {
        let style = ui.style(TypeRole::BodyMedium);
        content = content.push(
            text(spec.label)
                .size(style.size_px)
                .font(ui.font_of(style))
                .color(paint::label(ui, base, source.get())),
        );
    }

    let body = container(content)
        .height(Length::Fill)
        .align_y(Alignment::Center);

    shell::shell(
        &source,
        Length::Shrink,
        Length::Fixed(geometry.hit),
        spec.on_toggle,
        body.into(),
    )
}
