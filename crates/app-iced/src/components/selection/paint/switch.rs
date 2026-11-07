//! Switch 的角色映射（`research/12b` §10.3 / §10.4 / §10.6 / §10.7）。

use iced::Color;

use unlucky_design::color::Role;
use unlucky_design::shape::ShapeScale;

use crate::theme::Ui;

use super::super::super::interaction::{self, Interaction};
use super::super::geometry::SwitchGeometry;
use super::{disabled_container, disabled_ink, lifted, Frame, SwitchPaint};

/// Switch 的外观。
pub fn switch(ui: &Ui, selected: bool, state: Interaction) -> SwitchPaint {
    let geometry = SwitchGeometry::of(ui);
    let radius = ShapeScale::Full.radius_px(geometry.track_height);

    if state.is_disabled() {
        let container = disabled_container(ui);
        return if selected {
            SwitchPaint {
                track: Frame {
                    fill: Some(container),
                    outline: None,
                    stroke: 0.0,
                    radius,
                },
                // `DisabledSelectedHandleColor = Surface` 且不透明度 1（A1 的 fconst_1）
                thumb: ui.color(Role::Surface),
                icon: Some(disabled_ink(ui, Role::OnSurface)),
            }
        } else {
            SwitchPaint {
                track: Frame {
                    fill: Some(ui.color(Role::SurfaceContainerHighest)),
                    outline: Some(container),
                    stroke: geometry.track_stroke,
                    radius,
                },
                thumb: disabled_ink(ui, Role::OnSurface),
                icon: None,
            }
        };
    }

    if selected {
        SwitchPaint {
            track: Frame {
                fill: Some(ui.color(Role::Primary)),
                outline: None,
                stroke: 0.0,
                radius,
            },
            // `SelectedHover/Focus/PressedHandleColor = PrimaryContainer`（A1，1.4.0 起）
            thumb: if lifted(state) {
                ui.color(Role::PrimaryContainer)
            } else {
                ui.color(Role::OnPrimary)
            },
            icon: Some(ui.color(Role::OnPrimaryContainer)),
        }
    } else {
        SwitchPaint {
            track: Frame {
                fill: Some(ui.color(Role::SurfaceContainerHighest)),
                outline: Some(ui.color(Role::Outline)),
                stroke: geometry.track_stroke,
                radius,
            },
            // `UnselectedHover/Focus/PressedHandleColor = OnSurfaceVariant`（A1）
            thumb: if lifted(state) {
                ui.color(Role::OnSurfaceVariant)
            } else {
                ui.color(Role::Outline)
            },
            icon: None,
        }
    }
}

/// 状态层的 content 角色（选中 = 滑块色 `OnPrimary`，未选中 = `Outline`）。
pub fn switch_layer_role(selected: bool) -> Role {
    if selected {
        Role::OnPrimary
    } else {
        Role::Outline
    }
}

/// Switch 状态层的颜色：**叠在轨道色上**，不是叠在 surface 上。
///
/// switch 的状态层圆（Ø40）有一大半压在轨道（52×32）上：若照别的控件那样叠 surface，
/// 圆会在轨道里画出一块「更亮的补丁」（实测第一版就是这个样子，轨道像被啃掉一块）。
pub fn switch_layer(ui: &Ui, state: Interaction, selected: bool) -> Option<Color> {
    let track = if selected {
        Role::Primary
    } else {
        Role::SurfaceContainerHighest
    };
    interaction::overlay_color(ui, state, switch_layer_role(selected), track)
}
