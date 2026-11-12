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
