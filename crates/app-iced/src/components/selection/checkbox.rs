//! Checkbox（`research/15` §18 + 规格卡）：**18×18 / 圆角 4** / 描边向内收的 2px 框 /
//! 勾形 10.8dp 方头（路径比例 0.6 × 边长）/ indeterminate 形变为居中短横 / Ø40 状态层 / 48 命中区。
//!
//! 尺寸口径（M229）：官方 `Checkbox.kt` 画 20×20 / rx2，规格卡画 `rect 18×18 rx4`，本实现取规格卡
//! （`inkcat::metrics::CHECKBOX`）。勾形**比例不变**（0.2→0.8 的横向跨度），所以墨水跨度随边长从
//! 12dp 变成 10.8dp。
//!
//! ## 三条容易画错的（每条都有对应的像素断言或单测）
//!
//! 1. **描边向内**：外缘恒 **18**（§18.3 的尺寸换成本项目的 18）。iced 的 quad 渲染器把描边路径
//!    **向内缩半个线宽**（`iced_tiny_skia::engine::draw_quad`：`border_bounds + border_width / 2`），
//!    与 Compose `drawBox` 的 `topLeft = stroke/2, size = side − stroke` 同一口径，所以
//!    选中/未选中外缘同尺寸。
//! 2. **勾不是图标**：勾形按路径比例画（`shell::glyph`），代码里**没有第二个 `IconKind`**（§18.6）。
//! 3. **状态层是 Ø40 的圆**、命中区是 48：两者不同尺寸，所以外壳是透明的 48 按钮，
//!    里面套一个 40 的圆容器，再套 18 的方框（见 `shell` 的模块文档）。

use iced::widget::{container, row, text};
use iced::{Alignment, Element, Length, Padding};

use unlucky_design::typography::TypeRole;

use crate::theme::Ui;

use super::super::interaction::{Base, Interaction};
use super::geometry::ControlGeometry;
use super::paint::{self, Frame};
use super::shell::{self, StateSource};

/// 复选框的三种取值（§18.6：indeterminate 不是第四种「选中」，是「部分选中」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckValue {
    /// 未选中：透明内部 + 2px `OnSurfaceVariant` 描边。
    Unchecked,
    /// 选中：`Primary` 实心 + 勾。
    Checked,
    /// 部分选中：**同样是实心**，勾形变成一条居中短横（不换图标）。
    Indeterminate,
}

impl CheckValue {
    /// 框是否**填充**（选中与 indeterminate 都填，后者只是把勾换成短横）。
    pub const fn filled(self) -> bool {
        !matches!(self, CheckValue::Unchecked)
    }

    /// 是否要画短横而不是勾。
    pub const fn indeterminate(self) -> bool {
        matches!(self, CheckValue::Indeterminate)
    }
}

/// 一个复选框（框 + 可选标签；`on_toggle` 为 `None` = 禁用）。
#[derive(Debug, Clone)]
pub struct Checkbox<'a, M> {
    label: &'a str,
    value: CheckValue,
    on_toggle: Option<M>,
    error: bool,
    state: Option<Interaction>,
    /// 脚下那一层的底（M234，禁用态合成用）。
    base: Base,
}

impl<'a, M> Checkbox<'a, M> {
    /// 一个二态复选框。
    pub fn new(label: &'a str, selected: bool, on_toggle: M) -> Self {
        Self::tri(
            label,
            if selected {
                CheckValue::Checked
            } else {
                CheckValue::Unchecked
            },
            on_toggle,
        )
    }

    /// 三态复选框（`indeterminate` 由调用方给值）。
    pub fn tri(label: &'a str, value: CheckValue, on_toggle: M) -> Self {
        Self {
            label,
            value,
            on_toggle: Some(on_toggle),
            error: false,
            state: None,
            base: Base::CANVAS,
        }
    }

    /// 禁用的复选框（无消息；禁用不是「另一种颜色」，见 `paint::checkbox` 的禁用分支）。
    pub fn disabled(label: &'a str, value: CheckValue) -> Self {
        Self {
            label,
            value,
            on_toggle: None,
            error: false,
            state: None,
            base: Base::CANVAS,
        }
    }

    /// 校验失败态（§18.7：未选中描边 `Error`，选中填 `Error` + 勾 `OnError`）。
    pub fn error(mut self) -> Self {
        self.error = true;
        self
    }

    /// 强制视觉态（`--screen=gallery` 取证用；运行期不要调）。
    pub fn state(mut self, state: Interaction) -> Self {
        self.state = Some(state);
        self
    }

    /// **告诉复选框它踩在什么底上**（M234）：禁用态的 38% 内容/描边叠在这个底上。
    pub fn base(mut self, base: impl Into<Base>) -> Self {
        self.base = base.into();
        self
    }
}

/// 画一个复选框（框 + 标签）。
pub fn checkbox<'a, M: Clone + 'a>(ui: &'a Ui, spec: Checkbox<'a, M>) -> Element<'a, M> {
    let geometry = ControlGeometry::checkbox(ui);
    let filled = spec.value.filled();
    let indeterminate = spec.value.indeterminate();
    let error = spec.error;
    let radius = geometry.radius;
    let stroke = geometry.stroke;
    let base = spec.base;
    let source = StateSource::new(spec.state, spec.on_toggle.is_some());

    // 勾的颜色与 hover/pressed 无关（A1：`SelectedHover/Focus/PressedIconColor` 都是 `OnPrimary`），
    // 只随「选中 + 禁用」变 —— 所以这里用**静态**态求值（SVG 的颜色在构建期就要定）。
    let at_rest = paint::checkbox(ui, base, filled, error, source.get(), radius, stroke);
    let glyph: Element<'a, M> = if filled {
        shell::square(
            geometry.side,
            shell::glyph(geometry, indeterminate, at_rest.glyph),
        )
    } else {
        iced::widget::Space::new().into()
    };

    // 20 方框：填/描边（跟态走）+ 勾
    let boxed = container(glyph)
        .width(Length::Fixed(geometry.side))
        .height(Length::Fixed(geometry.side))
        .style(source.container_style(move |state| {
            paint::checkbox(ui, base, filled, error, state, radius, stroke)
                .frame
                .container_style()
        }));

    // Ø40 状态层：圆（半径 = 直径/2，`ShapeScale::Full` 的等价写法）
    let layer_side = geometry.state_layer;
    let layer = container(boxed)
        .width(Length::Fixed(layer_side))
        .height(Length::Fixed(layer_side))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(source.container_style(move |state| {
            Frame {
                fill: paint::state_layer(
                    ui,
                    base,
                    state,
                    paint::checkbox_layer_role(filled, error),
                ),
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

    // 命中区 48：方框 20 + 状态层 40 都在里面垂直居中；
    // 左右各留 `hit_padding` 透明内边距，无标签时命中区也是 48×48（复审 P2-2）
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
        spec.on_toggle,
        body.into(),
    )
}
