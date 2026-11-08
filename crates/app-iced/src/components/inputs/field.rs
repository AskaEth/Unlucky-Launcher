//! 文本框（`research/12b` §9）：Filled / Outlined 两式，**受控组件**。
//!
//! ## 参考值（`[源]` = A1，本机 `material3-android:1.4.0` 的 `*TextFieldTokens` + `TextFieldDefaults`）
//!
//! | # | 检查项 | 参考值 | 落点 |
//! |---|---|---|---|
//! | 9.1 | 高度 | min **56**（带标签）；无标签单行本项目 **40** | `FIELD_HEIGHT`（G16）/ `LM.control_height` |
//! | 9.2 | 最小宽 | **280** → **G21 未施加**（iced 的 `Length` 无 min-width，同按钮 G15） | 文档 |
//! | 9.3 | 形状 | Outlined **4**；Filled 顶圆角 4、底直角 | `Component::TextField.spec().shape` |
//! | 9.4 | 描边 / 指示线 | Outlined 静息 1dp `Outline` → hover 1dp `OnSurface` → 聚焦 **2dp `Primary`**；Filled 底指示线静息 `OnSurfaceVariant` | [`Filled`] / [`Palette`] |
//! | 9.5 | 字级 | 输入/标签 `BodyLarge 16/24`；辅助/错误 `BodySmall 12/16` | `TypeRole` |
//! | 9.6 | 图标 | 前后 **24**、`OnSurfaceVariant`；图标可视边 **12**、文本从 **48** 起 | `FieldGeometry` |
//! | 9.7 | 错误态 | 线/标签/辅助文本全 **`Error`**、线宽 **2dp** | [`Field::error`] |
//! | 9.8 | 禁用态 | Filled 容器 `OnSurface`@**4%**（→ 令牌只有 12%，**登记偏离 D10**）；Outlined 描边 `OnSurface`@**12%**；内容 @**38%** | `interaction::*` |
//!
//! ## 三处口径说明
//!
//! 1. **状态差异落在指示线/描边/标签上，容器底色不叠状态层**（A1 的口径，与按钮家族的 8%/10% 状态层不同）；press 无独立 token，按 hover 处理（D11）。
//! 2. 标签是「**常浮起**」的两段式（占上面 16px，不做上浮动画，登记 D12）；缺省宽度 `Fill`（最小宽 280 无法在 iced 表达，G21）。

use iced::widget::text::LineHeight;
use iced::widget::{column, container, row, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding, Pixels};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::icons;
use crate::theme::Ui;

use super::super::interaction::{self, Interaction};
use super::super::layout;
use super::geometry::FieldGeometry;

/// 两式容器（12b 9.3 / 9.4 的差别：Filled 有容器色 + 底部指示线；Outlined 只有描边）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldVariant {
    /// 实心：`SurfaceContainerHighest` + 底部指示线，顶圆角 4 底直角。
    Filled,
    /// 描边：透明容器 + 1dp `Outline`，四角 4。
    Outlined,
}

impl FieldVariant {
    /// 两式全序（gallery 的行序）。
    pub const ALL: [FieldVariant; 2] = [FieldVariant::Filled, FieldVariant::Outlined];

    /// 稳定名字（图例 / 测试信息）。
    pub const fn name(self) -> &'static str {
        match self {
            FieldVariant::Filled => "filled",
            FieldVariant::Outlined => "outlined",
        }
    }
}

/// 一个文本框的装配描述（不持有 `String`：受控组件只借值，契约 §2）。
#[derive(Debug, Clone)]
pub struct Field<'a> {
    variant: FieldVariant,
    value: &'a str,
    label: Option<&'a str>,
    placeholder: &'a str,
    supporting: Option<&'a str>,
    leading: Option<IconKind>,
    trailing: Option<IconKind>,
    error: bool,
    state: Option<Interaction>,
    width: Length,
    secure: bool,
}

impl<'a> Field<'a> {
    /// 一个 Filled 文本框。
    pub fn new(value: &'a str) -> Self {
        Self {
            variant: FieldVariant::Filled,
            value,
            label: None,
            placeholder: "",
            supporting: None,
            leading: None,
            trailing: None,
            error: false,
            state: None,
            width: Length::Fill,
            secure: false,
        }
    }

    /// 选式（Filled / Outlined）。
    pub fn variant(mut self, variant: FieldVariant) -> Self {
        self.variant = variant;
        self
    }

    /// 浮动标签（`BodySmall`，常浮起，见模块文档第 2 条）。
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self
    }

    /// 占位文案（只在值为空时可见，`OnSurfaceVariant`）。
    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = placeholder;
        self
    }

    /// 辅助/错误说明（字段下方 `BodySmall`）。
    pub fn supporting(mut self, supporting: &'a str) -> Self {
        self.supporting = Some(supporting);
        self
    }

    /// 前置图标（24，图标可视边 12）。
    pub fn leading(mut self, kind: IconKind) -> Self {
        self.leading = Some(kind);
        self
    }

    /// 后置图标（24，右侧内边距 16）。
    pub fn trailing(mut self, kind: IconKind) -> Self {
        self.trailing = Some(kind);
        self
    }

    /// 错误态：线 / 标签 / 辅助文本转 `Error`，线宽 2dp（9.7）。
    pub fn error(mut self) -> Self {
        self.error = true;
        self
    }

    /// 强制视觉态（gallery 状态矩阵取证；运行期不要调）。
    pub fn state(mut self, state: Interaction) -> Self {
        self.state = Some(state);
        self
    }

    /// 定宽（默认 `Fill`；gallery 的矩阵格用它钉宽度）。
    pub fn width(mut self, width: Length) -> Self {
        self.width = width;
        self
    }

    /// 密码式输入（M114）：上屏是掩码（明文上屏＝UI 侧凭据泄漏）。
    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }
}

/// 一个字段在某一态下的全套颜色（9.4 / 9.7 / 9.8 的落点）。
#[derive(Debug, Clone, Copy, PartialEq)]
struct Palette {
    /// 容器底色；`None` = 透明（Outlined）。
    fill: Option<Color>,
    /// 描边/指示线色。
    line: Color,
    /// 描边/指示线宽（1 或 2）。
    stroke: f32,
    /// 标签（`BodySmall`）。
    label: Color,
    /// 输入文本。
    input: Color,
    /// 前后图标。
    icon: Color,
    /// 辅助文本。
    supporting: Color,
    /// 占位文本。
    placeholder: Color,
}

/// 解析某一态的颜色（**唯一**做角色映射的地方）。
fn palette(ui: &Ui, variant: FieldVariant, state: Interaction, error: bool) -> Palette {
    let geometry = FieldGeometry::of(ui, true, None);

    if state.is_disabled() {
        // 9.8：内容一律 OnSurface@38%；Filled 容器叠 12%（官方 4%，令牌无 4% 档 → D10）；
        // Outlined 描边正好是官方 12%。
        let container = interaction::disabled_colors(ui, Role::Surface).0;
        let ink = interaction::disabled_content_over_role(
            ui,
            Role::OnSurface,
            matches!(variant, FieldVariant::Filled),
        );
        return Palette {
            fill: match variant {
                FieldVariant::Filled => Some(container),
                FieldVariant::Outlined => None,
            },
            line: match variant {
                FieldVariant::Filled => ink,
                FieldVariant::Outlined => container,
            },
            stroke: geometry.stroke,
            label: ink,
            input: ink,
            icon: ink,
            supporting: ink,
            placeholder: ink,
        };
    }

    let focused = matches!(state, Interaction::Focused);
    let hovered = matches!(state, Interaction::Hovered | Interaction::Pressed);

    let (line, stroke) = if error {
        (ui.color(Role::Error), geometry.focus_stroke)
    } else if focused {
        (ui.color(Role::Primary), geometry.focus_stroke)
    } else if hovered {
        (ui.color(Role::OnSurface), geometry.stroke)
    } else {
        (
            match variant {
                FieldVariant::Filled => ui.color(Role::OnSurfaceVariant),
                FieldVariant::Outlined => ui.color(Role::Outline),
            },
            geometry.stroke,
        )
    };

    Palette {
        fill: match variant {
            FieldVariant::Filled => Some(ui.color(Role::SurfaceContainerHighest)),
            FieldVariant::Outlined => None,
        },
        line,
        stroke,
        label: if error {
            ui.color(Role::Error)
        } else if focused {
            ui.color(Role::Primary)
        } else {
            ui.color(Role::OnSurfaceVariant)
        },
        input: ui.color(Role::OnSurface),
        icon: if error {
            ui.color(Role::Error)
        } else {
            ui.color(Role::OnSurfaceVariant)
        },
        supporting: if error {
            ui.color(Role::Error)
        } else {
            ui.color(Role::OnSurfaceVariant)
        },
        placeholder: ui.color(Role::OnSurfaceVariant),
    }
}

/// 一个文本框（Filled / Outlined 由 [`Field::variant`] 选）。
///
/// 受控：值由外部持有，输入经 `on_input` 回传；`state` 强制成 `Disabled` 时输入被摘掉
/// （`on_input_maybe(None)`），所以禁用字段真的点不动，不只是变灰。
pub fn text_field<'a, M: Clone + 'a>(
    ui: &'a Ui,
    field: Field<'a>,
    on_input: impl Fn(String) -> M + 'a,
) -> Element<'a, M> {
    let geometry = FieldGeometry::of(ui, field.label.is_some(), field.leading);
    let state = field.state.unwrap_or(Interaction::Enabled);
    let colors = palette(ui, field.variant, state, field.error);

    let input_style = ui.style(TypeRole::BodyLarge);
    let label_style = ui.style(TypeRole::BodySmall);

    let text_input = text_input(field.placeholder, field.value)
        .on_input_maybe(if state.is_disabled() {
            None
        } else {
            Some(on_input)
        })
        .secure(field.secure)
        .size(input_style.size_px)
        .line_height(LineHeight::Absolute(Pixels(input_style.line_height_px)))
        .font(ui.font_of(input_style))
        .padding(Padding::default())
        .width(Length::Fill)
        .style(move |_theme, _status| text_input::Style {
            background: Background::Color(Color::TRANSPARENT),
            border: Border::default(),
            icon: colors.icon,
            placeholder: colors.placeholder,
            value: colors.input,
            selection: colors.line,
        });

    let mut content = row![].align_y(Alignment::Center).spacing(geometry.icon_gap);
    if let Some(kind) = field.leading {
        // Q9：图标必须经固定尺寸容器，否则会被 Fill 约束撑满（M68 的 46px 字形）
        content = content.push(layout::fixed(
            geometry.icon,
            geometry.icon,
            icons::view::<M>(kind, colors.icon),
        ));
    }
    content = content.push(text_input);
    if let Some(kind) = field.trailing {
        content = content.push(layout::fixed(
            geometry.icon,
            geometry.icon,
            icons::view::<M>(kind, colors.icon),
        ));
    }

    let label = field.label.map(|label| {
        text(label)
            .size(label_style.size_px)
            .line_height(LineHeight::Absolute(Pixels(label_style.line_height_px)))
            .font(ui.font_of(label_style))
            .color(colors.label)
    });

    // 内容块：带标签 = [标签 16 + 输入 24] 共 40，居中在内容区里；无标签 = 单行 24 居中。
    // **垂直居中交给容器**（Q8 的教训：不要指望子元素自己贴边）。
    let inner: Element<'a, M> = match label {
        Some(label) => column![label, content].width(Length::Fill).into(),
        None => content.into(),
    };
    let content_area = container(inner)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::Center)
        .padding(Padding {
            top: 0.0,
            right: geometry.padding.right,
            bottom: 0.0,
            left: geometry.padding.left,
        });

    // Filled：容器色 + **底部指示线**（静息 1dp / 聚焦与错误 2dp）；Outlined：透明 + 描边。
    // 指示线占容器底边那 1~2px（内容区因此少 1~2px，官方也是画在 padding-box 底边上的）。
    let body: Element<'a, M> = match field.variant {
        FieldVariant::Filled => {
            let indicator = container(iced::widget::space::horizontal())
                .width(Length::Fill)
                .height(Length::Fixed(colors.stroke))
                .style(move |_theme| container::Style {
                    background: Some(Background::Color(colors.line)),
                    ..container::Style::default()
                });
            column![content_area, indicator].into()
        }
        FieldVariant::Outlined => content_area.into(),
    };

    let border = Border {
        color: if matches!(field.variant, FieldVariant::Outlined) {
            colors.line
        } else {
            Color::TRANSPARENT
        },
        width: if matches!(field.variant, FieldVariant::Outlined) {
            colors.stroke
        } else {
            0.0
        },
        // Filled 是「顶圆角 4、底直角」= CornerExtraSmallTop；ShapeScale 没有「部分角」，
        // 这里按官方语义手工指定上下角（登记 D13）。
        radius: match field.variant {
            FieldVariant::Filled => iced::border::Radius {
                top_left: ui.radius_dp(geometry.shape).top_left,
                top_right: ui.radius_dp(geometry.shape).top_right,
                bottom_left: 0.0,
                bottom_right: 0.0,
            },
            FieldVariant::Outlined => ui.radius_dp(geometry.shape),
        },
    };

    let field_el: Element<'a, M> = container(body)
        .width(field.width)
        .height(Length::Fixed(geometry.height))
        .style(move |_theme| container::Style {
            background: colors.fill.map(Background::Color),
            border,
            ..container::Style::default()
        })
        .into();

    match field.supporting {
        Some(supporting) => {
            let note = text(supporting)
                .size(label_style.size_px)
                .line_height(LineHeight::Absolute(Pixels(label_style.line_height_px)))
                .font(ui.font_of(label_style))
                .color(colors.supporting);
            column![field_el, layout::inset(0.0, ui.space(Space::X1), note)].into()
        }
        None => field_el,
    }
}

#[cfg(test)]
mod tests;
