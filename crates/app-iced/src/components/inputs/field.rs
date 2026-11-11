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
//!
//! ## 回车与失焦（P18 / M252）：只有回车是真的事件
//!
//! `iced_widget-0.14.2/src/text_input.rs` 的 builder 一共这几条（**逐条从源码抄的**，别凭记忆）：
//!
//! | 方法 | 行 | 产物 |
//! |---|---|---|
//! | `on_input` / `on_input_maybe` | 172 / 184 | 每次改动后发一条消息 |
//! | `on_submit` / `on_submit_maybe` | 194 / 201 | **焦点在框内时**按回车发**一条**消息（`:1049-1053`：一个 `KeyPressed` 事件只 `publish` 一次；不在焦点内的回车走不到这里） |
//! | `on_paste` / `on_paste_maybe` | 208 / 218 | 粘贴 |
//! | ~~`on_unfocus`~~ | **整个文件 0 命中** | 失焦**没有**消息通道：`State::unfocus()`（`:1524`）与 `Focusable for State`（`:1563`）只改控件内部状态，不发消息 |
//!
//! 所以本封装的取舍是：**把有的事件透出去**（[`Field::on_submit`]），失焦继续用代理近似
//! （回车 + 点到别的控件），**不为它去 fork iced**。`on_submit` 是**可选**的：不调
//! [`Field::on_submit`] 时 `text_input` 的 `on_submit` 保持空（`on_submit_maybe(None)`），
//! 回车**什么消息都不发**——全站既有调用点因此逐位不变（回归见 `tests.rs`）。

use iced::widget::text::LineHeight;
use iced::widget::{column, container, row, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding, Pixels};

use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::icons;
use crate::theme::Ui;

use super::super::interaction::{Base, Interaction};

#[cfg(test)]
use super::super::interaction;
use super::super::layout;
use super::geometry::FieldGeometry;

mod paint;
use paint::palette;

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
///
/// `M` 是**消息类型**，只被 [`Field::on_submit`] 用到：不给提交回调时它跟 `on_input` 的消息同型，
/// 调用点照旧写 `Field::new(value)…`（类型由 `text_field` 的 `on_input` 一并推出，不用标注）。
pub struct Field<'a, M> {
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
    /// 脚下那一层的底（M234，禁用态合成用）。
    base: Base,
    /// 回车要发的那条消息（P18；`None` = 回车什么都不发生，既有调用点全是这一档）。
    on_submit: Option<Box<dyn Fn() -> M + 'a>>,
}

impl<'a, M> Field<'a, M> {
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
            base: Base::CANVAS,
            on_submit: None,
        }
    }

    /// **告诉字段它踩在什么底上**（M234）：禁用态的 12% 容器 / 描边与 38% 内容都叠在这个底上。
    ///
    /// 放进卡片/面板里必须给（卡片 = `surfaces::elevated::BASE`）；不给就按画布合成，
    /// 禁用字段会与容器底脱节（M223 的「形丢失」）。
    pub fn base(mut self, base: impl Into<Base>) -> Self {
        self.base = base.into();
        self
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

    /// **回车提交**（P18 / M252）：焦点在框内时按回车，发这一条消息。
    ///
    /// 不调就是「回车什么都不发生」——这是默认档，也是全站既有调用点的现状。
    /// 禁用态（[`Field::state`] = `Disabled`）里回车**不发**（与 `on_input` 同一条规矩：
    /// 点不动的框也不该能提交）。
    pub fn on_submit(mut self, on_submit: impl Fn() -> M + 'a) -> Self {
        self.on_submit = Some(Box::new(on_submit));
        self
    }

    /// 回车那条消息（`None` = 没接 [`Field::on_submit`]，或这个状态不能交互）。
    ///
    /// **唯一的读取口**：`text_field` 只从这里取，于是「接没接」「点不点得动」与「发不发」
    /// 是同一个事实——禁用态里回车不提交（与 `on_input` 同一条规矩）。
    fn submit_message(&self, state: Interaction) -> Option<M> {
        if state.is_disabled() {
            return None;
        }
        self.on_submit.as_ref().map(|on_submit| on_submit())
    }
}

/// 一个文本框（Filled / Outlined 由 [`Field::variant`] 选）。
///
/// 受控：值由外部持有，输入经 `on_input` 回传；`state` 强制成 `Disabled` 时输入被摘掉
/// （`on_input_maybe(None)`），所以禁用字段真的点不动，不只是变灰。
/// 回车（P18）：只在 [`Field::on_submit`] 被调过时才接（`on_submit_maybe`），没接就是
/// 「回车什么都不发生」。
pub fn text_field<'a, M: Clone + 'a>(
    ui: &'a Ui,
    field: Field<'a, M>,
    on_input: impl Fn(String) -> M + 'a,
) -> Element<'a, M> {
    let geometry = FieldGeometry::of(ui, field.label.is_some(), field.leading);
    let state = field.state.unwrap_or(Interaction::Enabled);
    let colors = palette(ui, field.base, field.variant, state, field.error);

    // 提交消息只在**可交互**时接（禁用规则住在 `Field::submit_message` 里，可单测）。
    let submit = field.submit_message(state);

    let input_style = ui.style(TypeRole::BodyLarge);
    let label_style = ui.style(TypeRole::BodySmall);

    let text_input = text_input(field.placeholder, field.value)
        .on_input_maybe(if state.is_disabled() {
            None
        } else {
            Some(on_input)
        })
        .on_submit_maybe(submit)
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
