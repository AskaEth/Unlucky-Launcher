//! Radio 的角色映射（`research/15` §19.3 / §19.4 / §19.5）。

use unlucky_design::color::Role;

use crate::theme::Ui;

use super::super::super::interaction::Interaction;
use super::{disabled_ink, lifted, Frame, RadioPaint};

/// Radio 的外观。
///
/// **选中态 hover 不换色**：环与点在四态里都是 `Primary`（只有状态层变）。
pub fn radio(ui: &Ui, selected: bool, state: Interaction, radius: f32, stroke: f32) -> RadioPaint {
    let ink = if state.is_disabled() {
        disabled_ink(ui, Role::OnSurface)
    } else if selected {
        ui.color(Role::Primary)
    } else if lifted(state) {
        ui.color(Role::OnSurface)
    } else {
        ui.color(Role::OnSurfaceVariant)
    };

    RadioPaint {
        ring: Frame {
            fill: None,
            outline: Some(ink),
            stroke,
            radius,
        },
        dot: selected.then_some(ink),
    }
}

/// Radio 状态层的 content 角色（选中 = `Primary`，未选中 = `OnSurfaceVariant`）。
pub fn radio_layer_role(selected: bool) -> Role {
    if selected {
        Role::Primary
    } else {
        Role::OnSurfaceVariant
    }
}
