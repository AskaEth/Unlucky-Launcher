//! Checkbox 的角色映射（`research/15` §18.2 / §18.7 / §18.8）。

use unlucky_design::color::Role;

use crate::theme::Ui;

use super::super::super::interaction::Interaction;
use super::{disabled_ink, lifted, CheckboxPaint, Frame};

/// Checkbox 的外观。
///
/// **描边宽度恒 2**（禁用态也是 2，§18.8：禁用不是「变淡变细」）；
/// 选中态没有描边（`SelectedOutlineWidth = 0`，走填充分支）。
pub fn checkbox(
    ui: &Ui,
    selected: bool,
    error: bool,
    state: Interaction,
    radius: f32,
    stroke: f32,
) -> CheckboxPaint {
    if state.is_disabled() {
        return CheckboxPaint {
            frame: Frame {
                // A1：`SelectedDisabledContainerColor = OnSurface` +
                // `SelectedDisabledContainerOpacity = 0.38` —— **不是** 12%（复审 P2-3）。
                // 与未选中禁用的描边是同一个 38% 叠色，两格看起来才成对。
                fill: selected.then(|| disabled_ink(ui, Role::OnSurface)),
                outline: (!selected).then(|| disabled_ink(ui, Role::OnSurface)),
                stroke,
                radius,
            },
            // `SelectedDisabledIconColor = Surface`：勾是「透出底色」的浅色
            glyph: ui.color(Role::Surface),
        };
    }

    match (selected, error) {
        (true, false) => CheckboxPaint {
            frame: Frame {
                fill: Some(ui.color(Role::Primary)),
                outline: None,
                stroke: 0.0,
                radius,
            },
            glyph: ui.color(Role::OnPrimary),
        },
        (true, true) => CheckboxPaint {
            frame: Frame {
                fill: Some(ui.color(Role::Error)),
                outline: None,
                stroke: 0.0,
                radius,
            },
            glyph: ui.color(Role::OnError),
        },
        (false, error) => {
            let role = if error {
                Role::Error
            } else if lifted(state) {
                // `UnselectedHover/Focus/PressedOutlineColor = OnSurface`（A1；不是 OnSurfaceVariant）
                Role::OnSurface
            } else {
                Role::OnSurfaceVariant
            };
            CheckboxPaint {
                frame: Frame {
                    fill: None,
                    outline: Some(ui.color(role)),
                    stroke,
                    radius,
                },
                glyph: ui.color(role),
            }
        }
    }
}

/// Checkbox 状态层的 content 角色（选中/部分选中 = `Primary`，错误 = `Error`，否则 `OnSurfaceVariant`）。
///
/// 出处：Compose 的 ripple 色就是 `selectedContainerColor` / `unselectedOutlineColor`。
pub fn checkbox_layer_role(filled: bool, error: bool) -> Role {
    match (filled, error) {
        (true, false) => Role::Primary,
        (true, true) | (false, true) => Role::Error,
        (false, false) => Role::OnSurfaceVariant,
    }
}
