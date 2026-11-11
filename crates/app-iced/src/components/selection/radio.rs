//! Radio button（`research/15` §19 + 规格卡）：外圈 **18** / 环线 **2** / 内点 **10**（不是 12）/
//! 选中环+点 `Primary` / 未选中环 `OnSurfaceVariant`（hover 提到 `OnSurface`）/ Ø40 状态层。
//!
//! ## 为什么它不是「基于 Checkbox 画的」
//!
//! §19.6 明令：Radio **只能**是「2px 环 + 实心内点」，Checkbox **只能**是「2px 方框 + 勾/横」；
//! 两者尺寸相同（**18**）但形状语言不同，禁止混用。所以本文件是独立实现，**没有** import
//! `super::checkbox`（gallery 的 probe 另有一条断言：radio 的墨迹里不得出现方角像素）。
//!
//! ## 几何怎么落到像素
//!
//! 环：**18×18** 容器 + `Border { width: 2, radius: 9 }`。iced 的 quad 渲染器把描边路径向内缩
//! 半个线宽（半径 9−1=8、路径外接 16），再以 2 描边 → 外缘正好 18、内缘 14。这与 Compose
//! `RadioButton.kt` 的「环半径 = side/2 − 1 且描边居中 → 外缘正好 side」逐像素同源，
//! 只看 side 取 18（规格卡 `r=9`）而不是官方的 20。
//! 内点：独立 10×10 圆容器居中在环里（绘制半径 12/2 − 2/2 = 5，即**直径 10**，§19.2）。

use iced::widget::{container, row, text};
use iced::{Alignment, Element, Length, Padding};

use unlucky_design::typography::TypeRole;

use crate::theme::Ui;

use super::super::interaction::{Base, Interaction};
use super::geometry::ControlGeometry;
use super::paint::{self, Frame};
use super::shell::{self, StateSource};

/// 一个单选框（环 + 可选标签；`on_select` 为 `None` = 禁用）。
#[derive(Debug, Clone)]
pub struct Radio<'a, M> {
    label: &'a str,
    selected: bool,
    on_select: Option<M>,
    state: Option<Interaction>,
    /// 脚下那一层的底（M234，禁用态合成用）。
    base: Base,
}

impl<'a, M> Radio<'a, M> {
    /// 一个可选的单选框。
    pub fn new(label: &'a str, selected: bool, on_select: M) -> Self {
        Self {
            label,
            selected,
            on_select: Some(on_select),
            state: None,
            base: Base::CANVAS,
        }
    }

    /// 禁用的单选框（无消息）。
    pub fn disabled(label: &'a str, selected: bool) -> Self {
        Self {
            label,
            selected,
            on_select: None,
            state: None,
            base: Base::CANVAS,
        }
    }

    /// 强制视觉态（gallery 取证用；运行期不要调）。
    pub fn state(mut self, state: Interaction) -> Self {
        self.state = Some(state);
        self
    }

    /// **告诉单选框它踩在什么底上**（M234）：禁用态的 38% 环/内点叠在这个底上。
    pub fn base(mut self, base: impl Into<Base>) -> Self {
        self.base = base.into();
        self
    }
}

/// 画一个单选框（环 + 内点 + 标签）。
pub fn radio<'a, M: Clone + 'a>(ui: &'a Ui, spec: Radio<'a, M>) -> Element<'a, M> {
    let geometry = ControlGeometry::radio(ui);
    let selected = spec.selected;
    let radius = geometry.radius;
    let stroke = geometry.stroke;
    let dot_side = geometry.inner_dot;
    let base = spec.base;
    let source = StateSource::new(spec.state, spec.on_select.is_some());

    // 内点与环的颜色与 hover/pressed 无关（A1：选中态四态都是 `Primary`），
    // 只随「选中 + 禁用」变 —— 所以点的颜色在这里静态求值即可。
    let at_rest = paint::radio(ui, base, selected, source.get(), radius, stroke);

    // Ø10 内点（半径 = 直径/2）
    let dot: Element<'a, M> = match at_rest.dot {
        Some(ink) => container(iced::widget::Space::new())
            .width(Length::Fixed(dot_side))
            .height(Length::Fixed(dot_side))
            .style(move |_theme| {
                Frame {
                    fill: Some(ink),
                    outline: None,
                    stroke: 0.0,
                    radius: dot_side / 2.0,
                }
                .container_style()
            })
            .into(),
        None => iced::widget::Space::new().into(),
    };

    // 20 外圈：环（未选中时环就是唯一的墨迹；选中时环 + 点同色）
    let ring = container(dot)
        .width(Length::Fixed(geometry.side))
        .height(Length::Fixed(geometry.side))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(source.container_style(move |state| {
            paint::radio(ui, base, selected, state, radius, stroke)
                .ring
                .container_style()
        }));

    // Ø40 状态层
    let layer_side = geometry.state_layer;
    let layer = container(ring)
        .width(Length::Fixed(layer_side))
        .height(Length::Fixed(layer_side))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(source.container_style(move |state| {
            Frame {
                fill: paint::state_layer(ui, base, state, paint::radio_layer_role(selected)),
                outline: None,
                stroke: 0.0,
                radius: layer_side / 2.0,
            }
            .container_style()
        }));

    let mut content = row![layer].align_y(Alignment::Center).spacing(geometry.gap);
    if !spec.label.is_empty() {
        let style = ui.style(TypeRole::BodyLarge);
        content = content.push(
            text(spec.label)
                .size(style.size_px)
                .font(ui.font_of(style))
                .color(paint::label(ui, base, source.get())),
        );
    }

    // 命中区 48×48：左右各留 `hit_padding` 透明内边距（无标签时行宽才不是 40；复审 P2-2）
    let pad = geometry.hit_padding();
    let body = container(content)
        .height(Length::Fill)
        .align_y(Alignment::Center)
        .padding(Padding {
            left: pad,
            right: pad,
            ..Padding::default()
        });

    shell::shell(
        &source,
        Length::Shrink,
        Length::Fixed(geometry.hit),
        spec.on_select,
        body.into(),
    )
}
