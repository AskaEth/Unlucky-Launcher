//! Chip 角色与状态层的单测（M234 从 `selection/tests.rs` 纯搬移，守 400 行硬线）。
//!
//! 搬移零改动：判据与注释一字未动，只是**换文件**（`tests.rs` 加了 `.base(..)` 之后到 422 行）。

use super::*;

/// §13.5 / §13.6 / §13.7：选中态**无描边**、Assist 不可选中、禁用文字 @38%。
#[test]
fn chip_roles_follow_the_tokens() {
    let ui = ui();
    let radius = ChipGeometry::of(&ui).radius;

    let selected = paint::chip(
        &ui,
        Base::CANVAS,
        ChipVariant::Filter,
        true,
        Interaction::Enabled,
        radius,
    );
    assert_eq!(
        selected.frame.fill,
        Some(ui.color(Role::SecondaryContainer))
    );
    assert_eq!(selected.frame.outline, None, "13.5：选中描边 0dp");
    assert_eq!(selected.label, ui.color(Role::OnSecondaryContainer));

    let unselected = paint::chip(
        &ui,
        Base::CANVAS,
        ChipVariant::Filter,
        false,
        Interaction::Enabled,
        radius,
    );
    assert_eq!(
        unselected.frame.outline,
        Some(ui.color(Role::OutlineVariant))
    );
    assert_eq!(unselected.label, ui.color(Role::OnSurfaceVariant));

    // Assist 没有选中态：即使传 selected = true 也不会变 SecondaryContainer
    let assist = paint::chip(
        &ui,
        Base::CANVAS,
        ChipVariant::Assist,
        true,
        Interaction::Enabled,
        radius,
    );
    assert_ne!(assist.frame.fill, Some(ui.color(Role::SecondaryContainer)));
    assert_eq!(
        assist.icon,
        ui.color(Role::Primary),
        "13.4：Assist 前置图标 Primary"
    );

    let disabled = paint::chip(
        &ui,
        Base::CANVAS,
        ChipVariant::Filter,
        false,
        Interaction::Disabled,
        radius,
    );
    assert_ne!(disabled.label, unselected.label, "13.7：禁用文字降到 38%");
}

/// **选中的 chip 也吃状态层**（M299）：M3 的 chip 状态层就是容器自己，选中后底是实色
/// `SecondaryContainer`，hover / pressed 叠在它上面。
///
/// 抓得住：①把选中支改回「写死 `ui.color(SecondaryContainer)`」（`state` 参数没用上）⇒ hover /
/// pressed 都等于静息，前三条红；②hover 与 pressed 用同一个 α ⇒ 第四条红；③把静息也叠一层或
/// 换成别的角色 ⇒ 第一条逐值断言红。
/// 抓不住：像素（gallery selection 矩阵的 `chip·filter·on` 那一行 + `selection/probe.rs`）；
/// 也抓不住「自己抄了一份**正确**的叠层」——由 `selection/tests/single_entry.rs` 的接线扫描负责。
#[test]
fn a_selected_chip_reacts_to_hover_and_press() {
    use unlucky_design::color::StateLayer;

    let ui = ui();
    let radius = ChipGeometry::of(&ui).radius;
    let fill = |state| {
        paint::chip(&ui, Base::CANVAS, ChipVariant::Filter, true, state, radius)
            .frame
            .fill
            .expect("选中 chip 静息也有底")
    };

    let rest = fill(Interaction::Enabled);
    let hover = fill(Interaction::Hovered);
    let pressed = fill(Interaction::Pressed);

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
        ),
        "叠的那一层必须逐位等于令牌求值"
    );
    for color in [rest, hover, pressed] {
        assert_eq!(color.a, 1.0, "结果必须不透明（D27：CPU/GPU 同像素）");
    }
    // 未选中支没变：静息透明、hover 有状态层
    let unselected = |state| {
        paint::chip(&ui, Base::CANVAS, ChipVariant::Filter, false, state, radius)
            .frame
            .fill
    };
    assert_eq!(unselected(Interaction::Enabled), None);
    assert!(unselected(Interaction::Hovered).is_some());
}

/// 状态层只在三态存在，且**不是**满档角色色（α 由 `interaction` 给）。
#[test]
fn state_layer_only_exists_on_lifted_states() {
    let ui = ui();
    for (state, expected) in [
        (Interaction::Enabled, false),
        (Interaction::Hovered, true),
        (Interaction::Focused, true),
        (Interaction::Pressed, true),
        (Interaction::Disabled, false),
    ] {
        let layer = paint::state_layer(&ui, Base::CANVAS, state, Role::OnSurfaceVariant);
        assert_eq!(layer.is_some(), expected, "{state:?} 的状态层有无");
        if let Some(color) = layer {
            assert_eq!(color.a, 1.0, "状态层必须是实色（D27：CPU/GPU 一致）");
        }
    }
    // 同态下 hover 比 pressed 浅（8% < 10%）
    let hover =
        paint::state_layer(&ui, Base::CANVAS, Interaction::Hovered, Role::OnSurface).unwrap();
    let pressed =
        paint::state_layer(&ui, Base::CANVAS, Interaction::Pressed, Role::OnSurface).unwrap();
    assert_ne!(hover, pressed);
}
