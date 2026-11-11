//! 装配与样式：五式共用的一条产线（尺寸 / 内边距 / 图标盒 / 状态层 / 描边）。
//!
//! 单独成文件的原因：`mod.rs` 已经是「对外契约 + 变体角色表」，把绘制细节放一起会顶到 400 行红线；
//! 而且这条产线是**唯一**会把「令牌 → 像素」翻一次的地方，独立出来才好审。
//!
//! 禁用态的合成底（[`Base`]，M234）也从这里往下传：`Button::base(..)` → [`StyleSpec::base`] →
//! `disabled_button_colors` / `disabled_button_content`。装配侧没有第二个猜底色的地方。

use iced::widget::text::LineHeight;
use iced::widget::{button, text, Row};
use iced::{Alignment, Background, Border, Element, Length, Pixels, Shadow};

use unlucky_design::color::Role;
use unlucky_design::elevation::Elevation;
use unlucky_design::shape::ShapeScale;

use crate::icons;
use crate::theme::Ui;

use super::super::interaction::{self, Base, Interaction};
use super::super::layout;
use super::{Button, Geometry, Variant};

/// 一个按钮样式要的全部输入（避免 `style_closure` 长出七个位置参数）。
#[derive(Debug, Clone, Copy)]
pub(crate) struct StyleSpec {
    /// 容器填充角色；`None` = 透明（Outlined / Text / StandardIcon）。
    pub fill: Option<Role>,
    /// 内容（文字 + 图标）角色。
    pub content: Role,
    /// 描边角色；`None` = 无描边。
    pub border: Option<Role>,
    /// 圆角档位（来自组件表，五式统一 `Full`）。
    pub shape: ShapeScale,
    /// 描边宽度（`Geometry::stroke`：有描边的变体 = `stroke::HAIRLINE`，其余 0）。
    pub stroke: f32,
    /// 圆角折算用的边长（药丸 = 可见高）。
    pub radius_side: f32,
    /// **脚下那一层的底**（M234）：禁用态的 10% 容器与 38% 内容都叠在它上面。
    ///
    /// 由构造点给（卡片 = `surfaces::elevated::BASE`、菜单项 = `inputs::menu::PANEL_BASE`、
    /// 直接摆在画布上 = [`Base::CANVAS`]）。M229 之前这里恒为 `Role::Surface`，
    /// 卡片里的禁用按钮因此把 10% 叠在画布色上（M223 的「形丢失」）。
    pub base: Base,
    /// 强制态（gallery 取证）；`None` = 跟随运行期的 `button::Status`。
    ///
    /// **`Some(Enabled)` 与 `None` 不是一回事**：截图取证必须能强制「静息」，否则光标恰好停在
    /// 某个按钮上时那一格会被画成 hover（实测踩过：text 行 enabled 列出现不该有的灰底容器，
    /// 看起来像 Text 按钮自带容器）。
    pub forced: Option<Interaction>,
}

/// 五式共用的装配：**只画一处**，五个变体函数都走这里。
pub(super) fn assemble<'a, M: Clone + 'a>(
    ui: &'a Ui,
    variant: Variant,
    spec: Button<'a, M>,
) -> Element<'a, M> {
    let geometry = Geometry::sized(ui, variant, spec.leading.is_some(), spec.size);
    let (fill, content, _border) = variant.roles();
    let forced = spec.forced();

    // 按钮家族没有浮层级高程：L1（Elevated）只用容器色调差表达（rubric §7 不画阴影）
    debug_assert!(
        !matches!(
            variant.elevation(),
            Elevation::Level3 | Elevation::Level4 | Elevation::Level5
        ),
        "按钮家族不许有浮层高程"
    );
    // **内容色必须跟着态走**：禁用时文字/图标是 on-surface@38%，不是满档内容色。
    // 早先这里恒取 `ui.color(content)`，再被 `text(...).color(..)` 钉死，`Style.text_color`
    // 里的 38% 永远下发不出去 —— 表现为「text / standard icon 的 enabled 格与 disabled 格
    // 逐像素相同」（视觉 reviewer 实测 changed_px = 0；filled 禁用标签 1.23:1）。
    let ink = content_color(
        ui,
        spec.base,
        forced,
        content,
        fill,
        variant.disabled_content(),
    );

    // 标签排版跟尺寸档走：`ControlSize::label()` 已把「字号跟字级、字重固定 500」定死
    // （XS/S = LabelLarge 14、M = BodyLarge 16、L = HeadlineSmall 24、XL = HeadlineLarge 32）。
    let label_style = geometry.size.label();
    let label = text(spec.label)
        .size(label_style.size_px)
        .line_height(LineHeight::Absolute(Pixels(label_style.line_height_px)))
        .font(ui.font_of(label_style))
        .color(ink);

    let mut row = Row::new().align_y(Alignment::Center);
    if let Some(kind) = spec.leading {
        // Q9：图标一律经固定尺寸容器（按钮图标 18、IconButton 24 都不例外）。
        // **必须走 `view_at`**：盒 18 ≠ `RENDER_PX`(24)，用 24 烘焙再缩到 18 会把视觉线宽从 1.5 压到 1.1
        // （复审 r1 P2-2 实测 1.04~1.25px）。
        row = row.push(layout::fixed(
            geometry.icon,
            geometry.icon,
            icons::view_at::<M>(kind, ink, geometry.icon),
        ));
    }
    row = row.push(label);
    if let Some(kind) = spec.trailing {
        row = row.push(layout::fixed(
            geometry.icon,
            geometry.icon,
            icons::view_at::<M>(kind, ink, geometry.icon),
        ));
    }
    let row = row.spacing(geometry.gap);

    // **垂直居中必须在内容侧做**：iced 的 `button::layout` 把内容放在 padding-box 的左上角
    // （`layout::padded` 只 `move_to(padding.top)`，不做居中），所以内容自己撑满高度再居中。
    // 少了这一层，40 高的按钮里文字会贴上沿 10px（实测首版就是这样）。
    let content_box = iced::widget::container(row)
        .height(Length::Fill)
        .align_y(iced::Alignment::Center);

    let style = style_closure(ui, text_style_spec(ui, variant, &spec));

    let element = button(content_box)
        .height(Length::Fixed(geometry.height))
        .padding(geometry.padding)
        .style(style);

    match spec.on_press {
        Some(message) => element.on_press(message).into(),
        None => element.into(),
    }
}

/// `Button` → 样式描述：**装配的唯一接线点**（`assemble` 与单测都走这里）。
///
/// 抽出来的理由与 [`super::icon_style_spec`] 同源：`button::Style` 是闭包、`Element` 是黑盒，
/// 两者都不可内省，所以「禁用态的底到底传了哪一个」只能在**结构体**上断言。
/// 写死一个 `StyleSpec` 再喂给 `style_closure` 的测试证明不了接线（M229 复审 P1-1 踩过）。
pub(super) fn text_style_spec<M>(ui: &Ui, variant: Variant, spec: &Button<'_, M>) -> StyleSpec {
    let (fill, content, border) = variant.roles();
    let geometry = Geometry::sized(ui, variant, spec.leading.is_some(), spec.size);
    StyleSpec {
        fill,
        content,
        border,
        shape: variant.shape(),
        stroke: geometry.stroke,
        radius_side: geometry.height,
        base: spec.base,
        forced: spec.forced(),
    }
}

/// 内容色（文字与图标共用）：禁用态走令牌的 38%，其余态就是角色色。
///
/// 单独成函数是为了让「内容色跟态走」这件事只有一处实现——`assemble`（文字按钮）与 `icon`
/// （图标按钮）都调它，两条路径不会再各自钉死满档色。
/// 内容色：**禁用时的角色由调用方按变体给**（[`super::Variant::disabled_content`] /
/// [`super::IconVariant::disabled_content`]）——A1 里 FilledTonal 与 IconButton 是 `OnSurface`，
/// 其余是 `OnSurfaceVariant`，不能在这里统一写死。
pub(super) fn content_color(
    ui: &Ui,
    base: Base,
    forced: Option<Interaction>,
    content: Role,
    fill: Option<Role>,
    disabled_content: Role,
) -> iced::Color {
    match forced {
        // 有容器：38% 叠在**按钮家族 10% 容器**上（Filled / Tonal / Elevated），容器再叠在 `base` 上
        Some(Interaction::Disabled) if fill.is_some() => {
            interaction::disabled_button_content(ui, base, disabled_content, true)
        }
        // 无容器：38% 直接叠在 **base** 上（Text / Outlined / standard IconButton）
        Some(Interaction::Disabled) => {
            interaction::disabled_button_content(ui, base, disabled_content, false)
        }
        _ => ui.color(content),
    }
}

/// 生成 `button::style` 闭包：状态层 / 禁用 / 描边全在这里落地。
pub(super) fn style_closure(
    ui: &Ui,
    spec: StyleSpec,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style + '_ {
    move |_theme: &iced::Theme, status| {
        // 强制态优先（gallery 状态矩阵），否则把 iced 的四态翻成 M3 五态
        let state = spec
            .forced
            .unwrap_or_else(|| Interaction::from_status(status));

        // 禁用那一对颜色**只求值一次**：背景与描边用的是同一对值（早先调了两次）
        let disabled = state
            .is_disabled()
            .then(|| interaction::disabled_button_colors(ui, spec.base));

        let (background, text_color) = match disabled {
            Some((container, content)) => {
                // 透明容器在禁用态依然透明（官方只改内容透明度）
                (spec.fill.map(|_| container), content)
            }
            None => {
                // 有填充：状态层叠在**容器角色**上；透明容器：叠在表面色上（否则 hover 看不见）
                let background = match spec.fill {
                    Some(role) => match state.layer() {
                        Some(_) => interaction::overlay_color(ui, state, spec.content, role)
                            .or(Some(ui.color(role))),
                        None => Some(ui.color(role)),
                    },
                    None => transparent_state_color(ui, spec.base, state, spec.content),
                };
                (background, ui.color(spec.content))
            }
        };

        let border_color = match disabled {
            Some((container, _content)) => container,
            None => spec
                .border
                .map_or_else(|| ui.color(Role::OutlineVariant), |role| ui.color(role)),
        };

        button::Style {
            background: background.map(Background::Color),
            text_color,
            border: Border {
                color: if spec.border.is_some() {
                    border_color
                } else {
                    iced::Color::TRANSPARENT
                },
                // 线宽来自 `Geometry::stroke`（有描边的变体 = HAIRLINE，其余 0）；
                // `spec.border.is_none()` 时颜色是透明的，宽度也就无所谓了。
                width: spec.stroke,
                radius: ui.radius(spec.shape, spec.radius_side),
            },
            // rubric §7：L0/L1 不用阴影（Elevated 的 L1 由容器色调差表达）
            shadow: Shadow::default(),
            // ⚠️ `snap` 目前**只有 wgpu 后端实现**；我们的验收图走 tiny-skia（CPU），
            // 所以这个 `true` 在验收图上不起作用——分隔线的「1dp 摊成 2 行」正是这个原因，
            // 不是「iced 没有 snap」（reviewer P2-4 纠正的正是我上一版报告里的说法）。
            snap: true,
        }
    }
}

/// 透明容器的状态层：`Outlined` / `Text` / `StandardIcon` 没有填充，可见反馈只能来自
/// 叠在**内容角色**上的状态层——把它们当作「叠在 `base`（脚下那一层）上」求值，得到不透明实色。
///
/// 之所以单独一个函数：这是 M3 里唯一「容器不存在但状态层存在」的情形，
/// 三个变体共用一条口径，免得各自发挥。
///
/// M234 复审 P2-3：`base` 之前写死 `Role::Surface`，卡片里的 Text/Outlined 按钮抬起时
/// 状态层叠在画布色上（与禁用支的口径不一致）。
pub(super) fn transparent_state_color(
    ui: &Ui,
    base: Base,
    state: Interaction,
    content: Role,
) -> Option<iced::Color> {
    interaction::overlay_color(ui, state, content, base.role())
}
