//! Chip 的角色映射（`research/12b` §13.5 / §13.6 / §13.7）。

use unlucky_design::color::Role;

use crate::theme::{Ui, STROKE_PX};

use super::super::super::interaction::Interaction;
use super::{disabled_container, disabled_ink, state_layer, ChipPaint, Frame};

/// Chip 的三种（§13.6 / §13.7）：Assist / Filter / Input。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipVariant {
    /// Assist：**永远未选中**，前置图标 `Primary`（`AssistChipTokens.IconColor`）。
    Assist,
    /// Filter：可选中（`SecondaryContainer`），选中态无描边。
    Filter,
    /// Input：可选中 + 后置关闭图标。
    Input,
}

/// Chip 的外观。
///
/// 与 Checkbox/Radio 的区别：Chip 的**状态层就是它自己的容器**（M3 的 chip 没有圆形状态层），
/// 所以未选中 + hover 的 fill 是「状态层叠在 surface 上」的实色。
pub fn chip(
    ui: &Ui,
    variant: ChipVariant,
    selected: bool,
    state: Interaction,
    radius: f32,
) -> ChipPaint {
    let selected = selected && !matches!(variant, ChipVariant::Assist);
    let label_role = if selected {
        Role::OnSecondaryContainer
    } else if matches!(variant, ChipVariant::Assist) {
        Role::OnSurface
    } else {
        Role::OnSurfaceVariant
    };

    if state.is_disabled() {
        let ink = disabled_ink(ui, Role::OnSurface);
        return ChipPaint {
            frame: Frame {
                fill: selected.then(|| disabled_container(ui)),
                outline: (!selected).then(|| disabled_container(ui)),
                stroke: STROKE_PX,
                radius,
            },
            label: ink,
            icon: ink,
        };
    }

    let outline = (!selected).then(|| ui.color(Role::OutlineVariant));
    let fill = if selected {
        Some(ui.color(Role::SecondaryContainer))
    } else {
        state_layer(ui, state, label_role)
    };
    // Assist / 选中态的前置图标是 `Primary`（`AssistChipTokens.IconColor` /
    // `InputChipTokens.SelectedLeadingIconColor`）；其余跟文字同色。
    let icon_role = if matches!(variant, ChipVariant::Assist) || selected {
        Role::Primary
    } else {
        label_role
    };

    ChipPaint {
        frame: Frame {
            fill,
            outline,
            stroke: STROKE_PX,
            radius,
        },
        label: ui.color(label_role),
        icon: ui.color(icon_role),
    }
}
