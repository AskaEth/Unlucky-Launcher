//! Chip（`research/12b` §13）：高 **32** / 圆角 **8** / 水平内边距 **8** / 元素间距 **8** /
//! 图标 **18**（`IconSize::Button` = 官方 `AssistChipTokens.IconSize`）/ 描边 1px `OutlineVariant`。
//!
//! ## 三类（§13 的 Assist / Filter / Input）
//!
//! | 类 | 选中态 | 容器 | 文字 | 前置图标 |
//! |---|---|---|---|---|
//! | Assist | **无**（一次动作，不是开关） | 透明 + 1px 描边 | `OnSurface` | `Primary` |
//! | Filter | 有 | 未选透明+描边；选中 `SecondaryContainer` **无描边** | `OnSurfaceVariant` / `OnSecondaryContainer` | 选中态才给勾 |
//! | Input | 有 | 同 Filter | 同 Filter | 可给 + 后置关闭图标 |
//!
//! ## 为什么 Chip 不用 `selection::shell` 那条透明外壳
//!
//! M3 的 chip **没有圆形状态层**：hover / pressed 就是把状态层叠在**自己的容器**上，
//! 而容器尺寸 = 命中区尺寸。所以 chip 直接走 `button` 的样式闭包（与按钮家族同一条产线），
//! 不需要「48 外壳 + 40 圆」那套。

use iced::widget::{button, container, row, text};
use iced::{Alignment, Background, Border, Element, Length, Padding, Shadow};

use unlucky_design::icons::IconKind;
use unlucky_design::typography::TypeRole;

use crate::icons;
use crate::theme::Ui;

use super::super::interaction::Interaction;
use super::geometry::ChipGeometry;
use super::paint::{self, ChipVariant};

/// 一个 chip（可带前后图标；`on_press` 为 `None` = 禁用）。
#[derive(Debug, Clone)]
pub struct Chip<'a, M> {
    label: &'a str,
    variant: ChipVariant,
    selected: bool,
    leading: Option<IconKind>,
    trailing: Option<IconKind>,
    on_press: Option<M>,
    state: Option<Interaction>,
}

impl<'a, M> Chip<'a, M> {
    /// Assist chip：辅助动作，**没有**选中态。
    pub fn assist(label: &'a str, on_press: M) -> Self {
        Self::new(label, ChipVariant::Assist, false, on_press)
    }

    /// Filter chip：筛选开关（`selected` 决定容器是否 `SecondaryContainer`）。
    pub fn filter(label: &'a str, selected: bool, on_press: M) -> Self {
        Self::new(label, ChipVariant::Filter, selected, on_press)
    }

    /// Input chip：用户输入的实体（可选中 + 可带关闭图标）。
    pub fn input(label: &'a str, on_press: M) -> Self {
        Self::new(label, ChipVariant::Input, false, on_press)
    }

    /// 转成禁用：**只清消息**，图标 / 选中态 / 状态层全部保留（复审 P2-1）。
    ///
    /// 官方禁用态是「容器/描边 12%、文字与**图标** 38%」——只降**不删**。早先的
    /// `Chip::disabled(label, variant)` 把 leading / trailing / selected 一起丢掉，实测
    /// assist 78→54、filter·on 74→50（连填充和勾都没了），禁用列里 `filter·on` 与 `filter`
    /// 逐像素同形，看不出选中与否。
    pub fn disabled(mut self) -> Self {
        self.on_press = None;
        self
    }

    fn new(label: &'a str, variant: ChipVariant, selected: bool, on_press: M) -> Self {
        Self {
            label,
            variant,
            selected,
            leading: None,
            trailing: None,
            on_press: Some(on_press),
            state: None,
        }
    }

    /// 前置图标（Assist / Filter 用）。
    pub fn leading(mut self, kind: IconKind) -> Self {
        self.leading = Some(kind);
        self
    }

    /// 后置图标（Input chip 的关闭键）。
    pub fn trailing(mut self, kind: IconKind) -> Self {
        self.trailing = Some(kind);
        self
    }

    /// 强制视觉态（gallery 取证用；运行期不要调）。
    pub fn state(mut self, state: Interaction) -> Self {
        self.state = Some(state);
        self
    }
}

/// 画一个 chip。
pub fn chip<'a, M: Clone + 'a>(ui: &'a Ui, spec: Chip<'a, M>) -> Element<'a, M> {
    let geometry = ChipGeometry::of(ui);
    let variant = spec.variant;
    let selected = spec.selected;
    let disabled = spec.on_press.is_none();
    let forced = spec.state;

    // 文字/图标色只看「选中 + 禁用」（A1：Filter/Input 的 label 色在 hover/focus/pressed 不变），
    // 所以这里静态求值；容器（含状态层）在样式闭包里跟态走。
    let at_rest = paint::chip(
        ui,
        variant,
        selected,
        if disabled {
            Interaction::Disabled
        } else {
            forced.unwrap_or(Interaction::Enabled)
        },
        geometry.radius,
    );

    let mut content = row![].align_y(Alignment::Center).spacing(geometry.gap);
    if let Some(kind) = spec.leading {
        // Q9：图标一律经固定尺寸容器；盒 18 ≠ RENDER_PX(24)，所以必须走 `view_at` 按 18 烘焙，
        // 否则线宽会被布局缩水（复审 r1 P2-2）。
        content = content.push(super::super::layout::centered_fixed(
            geometry.icon,
            geometry.icon,
            icons::view_at::<M>(kind, at_rest.icon, geometry.icon),
        ));
    }

    let style = ui.style(TypeRole::LabelLarge);
    content = content.push(
        text(spec.label)
            .size(style.size_px)
            .font(ui.font_of(ui.emphasized(TypeRole::LabelLarge)))
            .color(at_rest.label),
    );

    if let Some(kind) = spec.trailing {
        content = content.push(super::super::layout::centered_fixed(
            geometry.icon,
            geometry.icon,
            icons::view_at::<M>(kind, at_rest.icon, geometry.icon),
        ));
    }

    // 垂直居中必须在内容侧做（`button` 把内容放左上角）
    let body = container(content)
        .height(Length::Fill)
        .align_y(Alignment::Center);

    let element = button(body)
        .height(Length::Fixed(geometry.height))
        .padding(Padding {
            top: 0.0,
            right: geometry.pad,
            bottom: 0.0,
            left: geometry.pad,
        })
        .style(move |_theme, status| {
            let state = forced.unwrap_or_else(|| Interaction::from_status(status));
            let paint = paint::chip(ui, variant, selected, state, geometry.radius);
            button::Style {
                background: paint.frame.fill.map(Background::Color),
                text_color: paint.label,
                border: Border {
                    color: paint.frame.outline.unwrap_or(iced::Color::TRANSPARENT),
                    width: if paint.frame.outline.is_some() {
                        paint.frame.stroke
                    } else {
                        0.0
                    },
                    radius: iced::border::Radius::new(paint.frame.radius),
                },
                shadow: Shadow::default(),
                snap: true,
            }
        });

    match spec.on_press {
        Some(message) => element.on_press(message).into(),
        None => element.into(),
    }
}
