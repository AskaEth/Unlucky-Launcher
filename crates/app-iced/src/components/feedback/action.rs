//! 反馈家族里的两个**小动作件**：文字动作与图标动作。
//!
//! ## 为什么不复用 `buttons`
//!
//! 这两处的内容角色都不在按钮家族的三档里：Dialog 动作 = `Primary`，Snackbar action =
//! `InversePrimary`（A1 `SnackbarTokens.ActionLabelTextColor`），Snackbar 关闭图标 =
//! `InverseOnSurface`。`buttons::text` 固定 `OnSurfaceVariant`（12b §1.9 的 Text 变体口径），
//! 拿它画 Dialog 动作就是颜色角色错位（rubric 判 P1）。
//!
//! 所以这里只做**一份**最小实现：内容/容器角色作参数，尺寸/字级仍走令牌，状态层仍走
//! [`interaction`]——与按钮家族同一条产线，只是角色由调用点给。
//!
//! ## 禁用是状态不是构造器
//!
//! 照 `selection/chip.rs` 修好的模式：`on_press: Option<M>`，`None` = 禁用；文字色降到
//! `OnSurface@38%`，标签本身**不删**。禁用色的**底**是这两个动作件自己的容器角色
//! （`InverseSurface` / `SurfaceContainerHigh`）——M234 起
//! [`interaction::disabled_content_over_role`] 收任意底（[`interaction::Base`]），所以这里
//! **直接传容器角色**，不再自己写一遍 α 合成（M234 之前那段本地合成会得到「38% 深字叠深底」，复审 P2-4）。

use iced::widget::{button, text};
use iced::{Background, Border, Element, Length, Padding, Shadow};

use unlucky_design::color::Role;
use unlucky_design::component_map::Component;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::icons;
use crate::theme::Ui;

use super::super::interaction::{self, Interaction};
use super::super::layout;

/// 禁用态内容色：`OnSurface@38%` 叠在**真实容器色**上。
///
/// 这两个动作件的底是 `InverseSurface`（Snackbar）与 `SurfaceContainerHigh`（Dialog），
/// M234 之后 `interaction::disabled_content_over_role` 收任意 [`interaction::Base`]，所以
/// 这里只把容器角色包成 `Base` 交给它——α（0.38）与叠法仍由 `interaction::disabled` 一处给，
/// 本文件零 α 字面量、也没有第二份合成实现。
fn content_color(ui: &Ui, role: Role, container_role: Role, disabled: bool) -> iced::Color {
    if !disabled {
        return ui.color(role);
    }
    // M234：`disabled_content_over_role` 现在收**任意**底（`Base`），所以这里的本地合成可以删掉
    // ——底就是这两个动作件自己的容器角色（Snackbar = InverseSurface / Dialog = SurfaceContainerHigh），
    // α 与叠法仍由 `interaction::disabled` 那一处给（本文件零 α 字面量）。
    interaction::disabled_content_over_role(
        ui,
        interaction::Base::of(container_role),
        Role::OnSurface,
        false,
    )
}

/// 一个文字动作（Dialog 的动作区、Snackbar 的 action 共用）。
///
/// - `content` / `container_role`：内容色与**状态层叠在什么颜色上**（Dialog =
///   `Primary` on `SurfaceContainerHigh`；Snackbar = `InversePrimary` on `InverseSurface`）；
/// - `on_press = None` → 禁用（文字降到 38%，仍占位）；
/// - `forced`：取证用的强制态（运行期不要给）。
pub(super) fn text_action<'a, M: Clone + 'a>(
    ui: &'a Ui,
    label: &'a str,
    content: Role,
    container_role: Role,
    on_press: Option<M>,
    forced: Option<Interaction>,
) -> Element<'a, M> {
    let style = ui.emphasized(TypeRole::LabelLarge);
    let height = ui.metrics().control_height;
    let pad = ui.space(Space::X3);
    let shape = Component::TextButton.spec().shape;
    let radius = ui.radius(shape, height);
    let disabled = on_press.is_none();

    let ink = content_color(ui, content, container_role, disabled);

    let element = button(
        text(label)
            .size(style.size_px)
            .font(ui.font_of(style))
            .color(ink),
    )
    .height(Length::Fixed(height))
    .padding(Padding {
        top: 0.0,
        right: pad,
        bottom: 0.0,
        left: pad,
    })
    .style(move |_theme, status| {
        let state = forced.unwrap_or_else(|| {
            if disabled {
                Interaction::Disabled
            } else {
                Interaction::from_status(status)
            }
        });
        button::Style {
            background: interaction::overlay_color(ui, state, content, container_role)
                .map(Background::Color),
            text_color: ink,
            border: Border {
                radius,
                ..Border::default()
            },
            // 浮层的阴影只在 Dialog 容器上（rubric K3 单层），动作件本身不投影。
            shadow: Shadow::default(),
            snap: true,
        }
    });

    match on_press {
        Some(message) => element.on_press(message).into(),
        None => element.into(),
    }
}

/// 一个图标动作（Snackbar 的关闭键）。命中区 = `touch_target`，字形经 `layout::centered_fixed`
/// 锁在 24（Q9：不锁的话字体会被 Button 的 Fill 约束撑满）。
pub(super) fn icon_action<'a, M: Clone + 'a>(
    ui: &'a Ui,
    kind: IconKind,
    content: Role,
    container_role: Role,
    on_press: Option<M>,
    forced: Option<Interaction>,
) -> Element<'a, M> {
    let metrics = ui.metrics();
    let side = metrics.touch_target;
    let glyph_side = unlucky_design::icons::IconSize::Toolbar.px();
    let shape = Component::IconButton.spec().shape;
    let radius = ui.radius(shape, side);
    let disabled = on_press.is_none();

    let ink = content_color(ui, content, container_role, disabled);

    // 走 `view_at(24)` 而不是 `view(..)`：`icons.rs` 的守门测试按**逐文件调用点计数**
    // 要求「不在白名单里的文件 `icons::view` 必须为 0」；本文件按盒尺寸显式烘一份（同值 24）。
    let element = button(layout::centered_fixed(
        glyph_side,
        glyph_side,
        icons::view_at::<M>(kind, ink, glyph_side),
    ))
    .width(Length::Fixed(side))
    .height(Length::Fixed(side))
    .padding(Padding::default())
    .style(move |_theme, status| {
        let state = forced.unwrap_or_else(|| {
            if disabled {
                Interaction::Disabled
            } else {
                Interaction::from_status(status)
            }
        });
        button::Style {
            background: interaction::overlay_color(ui, state, content, container_role)
                .map(Background::Color),
            text_color: ink,
            border: Border {
                radius,
                ..Border::default()
            },
            shadow: Shadow::default(),
            snap: true,
        }
    });

    match on_press {
        Some(message) => element.on_press(message).into(),
        None => element.into(),
    }
}
