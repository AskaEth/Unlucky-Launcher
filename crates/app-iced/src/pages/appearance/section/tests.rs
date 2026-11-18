//! `appearance::section` 的单测（M299 从 `mod.rs` 纯搬移，守 400 行硬线）。
//!
//! 判据与注释一字未动，只是**换文件**：`mod.rs` 加了一条「选中的档位跟态走」之后到 438 行。
//! 模块路径仍是 `appearance::section::tests`（`mod.rs` 里改成 `mod tests;`），`super::*` 拿到的
//! 还是 `appearance::section`。

use super::*;
use unlucky_design::color::{SchemeFlavor, StateLayer};
use unlucky_design::typography::Platform;

use crate::theme::Families;

fn ui() -> Ui {
    Ui::with_platform(
        SchemeFlavor::Light,
        Platform::Windows,
        Families {
            text: "Segoe UI",
            display: "Segoe UI",
            mono: "Consolas",
        },
    )
}

fn solid(style: button::Style) -> Color {
    match style.background {
        Some(Background::Color(color)) => color,
        None => panic!("未选中态的 hover/pressed 必须叠状态层"),
        Some(other) => panic!("背景必须是实色，得到 {other:?}"),
    }
}

/// hover(8%) 与 pressed(10%) 是**两档**，且各自逐值等于令牌表（把 10% 改成 11% 也会红）。
///
/// 抓得住：①回到 `matches!(status, Hovered | Pressed)` 合并分支（两色相等 ⇒ 第一条红）；
/// ②pressed 用 hover 的 α、或合成底被换回画布（第二、三条红）。
/// 抓不住：观感；**过渡中间帧**（forced 路径不套 `tracked`，`layer_color` 直落端点值）。
///
/// 底的角色取 `MaterialLayer::Card.surface_role()`（`frame::BASE` 就是它）：既盯住「叠的是卡底」
/// 这条语义，也不往 `settings/*.rs` 的接线点计数里多塞一次字面量（`pages/tests/base_wiring.rs`）。
#[test]
fn the_choice_button_draws_pressed_darker_than_hover() {
    let ui = ui();
    let style = choice_style(
        &ui,
        false,
        ui.color(Role::OnSurfaceVariant),
        ui.radius(ShapeScale::Full, ui.metrics().control_height),
        None,
    );
    let theme = ui.theme();
    let hover = solid(style(&theme, button::Status::Hovered));
    let pressed = solid(style(&theme, button::Status::Pressed));

    assert_ne!(hover, pressed, "hover 与 pressed 必须是两档（8% / 10%）");
    let base = MaterialLayer::Card.surface_role();
    assert_eq!(
        hover,
        ui.state_over(StateLayer::Hover, Role::OnSurface, base)
    );
    assert_eq!(
        pressed,
        ui.state_over(StateLayer::Pressed, Role::OnSurface, base)
    );
    assert!(
        style(&theme, button::Status::Active).background.is_none(),
        "静息态不叠状态层"
    );
}

/// **选中的档位也跟态走**（M299）：底是实色 `SecondaryContainer`，状态层叠在它上面。
///
/// 抓得住：①把选中支改回 `Some(ui.color(SecondaryContainer))`（= 选中就不响应）⇒ hover /
/// pressed 都等于静息，前三条红；②hover 与 pressed 同 α ⇒ 第四条红；③合成底被换掉 ⇒ 后两条
/// 逐值断言红。
/// 抓不住：像素（`DevDocs/artifacts/m299/settings-appearance-*-hover|pressed.png`，`--force-state`
/// 经 `settings::view` → `appearance::section` 的 `forced`）；也抓不住「自己抄了一份**正确**的
/// 叠层」——由 `selection/tests/single_entry.rs` 的接线扫描负责。
#[test]
fn a_selected_choice_reacts_to_hover_and_press() {
    let ui = ui();
    let style = choice_style(
        &ui,
        true,
        ui.color(Role::OnSecondaryContainer),
        ui.radius(ShapeScale::Full, ui.metrics().control_height),
        None,
    );
    let theme = ui.theme();
    let rest = solid(style(&theme, button::Status::Active));
    let hover = solid(style(&theme, button::Status::Hovered));
    let pressed = solid(style(&theme, button::Status::Pressed));

    assert_eq!(
        rest,
        ui.color(Role::SecondaryContainer),
        "选中静息 = 实色容器"
    );
    assert_ne!(hover, rest, "选中 + hover 必须与静息不同");
    assert_ne!(pressed, rest, "选中 + pressed 必须与静息不同");
    assert_ne!(hover, pressed, "8% 与 10% 必须能区分");
    assert_eq!(
        hover,
        ui.state_over(
            StateLayer::Hover,
            Role::OnSecondaryContainer,
            Role::SecondaryContainer
        )
    );
    assert_eq!(
        pressed,
        ui.state_over(
            StateLayer::Pressed,
            Role::OnSecondaryContainer,
            Role::SecondaryContainer
        )
    );
}
