//! 文本框的颜色求值（`research/12b` §9.4 / §9.7 / §9.8 的落点）。
//!
//! M234 纯搬移：本文件原来是 `inputs/field.rs` 的 `Palette` + `palette()`（`field.rs` 已顶到
//! 398 行，加 `.base(..)` 之前必须腾地方）。搬移只做了一件事以外的零改动——`palette()` 多收
//! 一个 [`Base`]（禁用态的合成底，M234），其余角色映射一字未改。

use iced::Color;

use unlucky_design::color::Role;

use crate::theme::Ui;

use super::super::super::interaction::{self, Base, Interaction};
use super::super::geometry::FieldGeometry;
use super::FieldVariant;

/// 一个字段在某一态下的全套颜色（9.4 / 9.7 / 9.8 的落点）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Palette {
    /// 容器底色；`None` = 透明（Outlined）。
    pub(super) fill: Option<Color>,
    /// 描边/指示线色。
    pub(super) line: Color,
    /// 描边/指示线宽（1 或 2）。
    pub(super) stroke: f32,
    /// 标签（`BodySmall`）。
    pub(super) label: Color,
    /// 输入文本。
    pub(super) input: Color,
    /// 前后图标。
    pub(super) icon: Color,
    /// 辅助文本。
    pub(super) supporting: Color,
    /// 占位文本。
    pub(super) placeholder: Color,
}

/// 解析某一态的颜色（**唯一**做角色映射的地方）。
///
/// `base` 是字段**脚下那一层的底**（M234）：禁用态的 12% 容器与 38% 内容都叠在它上面
/// （Filled 容器 / Outlined 描边 / 全部标签与文本）。调用点给真实父底，不写死 `Role::Surface`。
pub(super) fn palette(
    ui: &Ui,
    base: Base,
    variant: FieldVariant,
    state: Interaction,
    error: bool,
) -> Palette {
    let geometry = FieldGeometry::of(ui, true, None);

    if state.is_disabled() {
        // 9.8：内容一律 OnSurface@38%；Filled 容器叠 12%（官方 4%，令牌无 4% 档 → D10）；
        // Outlined 描边正好是官方 12%。
        let container = interaction::disabled_colors(ui, base).0;
        let ink = interaction::disabled_content_over_role(
            ui,
            base,
            Role::OnSurface,
            matches!(variant, FieldVariant::Filled),
        );
        return Palette {
            fill: match variant {
                FieldVariant::Filled => Some(container),
                FieldVariant::Outlined => None,
            },
            line: match variant {
                FieldVariant::Filled => ink,
                FieldVariant::Outlined => container,
            },
            stroke: geometry.stroke,
            label: ink,
            input: ink,
            icon: ink,
            supporting: ink,
            placeholder: ink,
        };
    }

    let focused = matches!(state, Interaction::Focused);
    let hovered = matches!(state, Interaction::Hovered | Interaction::Pressed);

    let (line, stroke) = if error {
        (ui.color(Role::Error), geometry.focus_stroke)
    } else if focused {
        (ui.color(Role::Primary), geometry.focus_stroke)
    } else if hovered {
        (ui.color(Role::OnSurface), geometry.stroke)
    } else {
        (
            match variant {
                FieldVariant::Filled => ui.color(Role::OnSurfaceVariant),
                FieldVariant::Outlined => ui.color(Role::Outline),
            },
            geometry.stroke,
        )
    };

    Palette {
        fill: match variant {
            FieldVariant::Filled => Some(ui.color(Role::SurfaceContainerHighest)),
            FieldVariant::Outlined => None,
        },
        line,
        stroke,
        label: if error {
            ui.color(Role::Error)
        } else if focused {
            ui.color(Role::Primary)
        } else {
            ui.color(Role::OnSurfaceVariant)
        },
        input: ui.color(Role::OnSurface),
        icon: if error {
            ui.color(Role::Error)
        } else {
            ui.color(Role::OnSurfaceVariant)
        },
        supporting: if error {
            ui.color(Role::Error)
        } else {
            ui.color(Role::OnSurfaceVariant)
        },
        placeholder: ui.color(Role::OnSurfaceVariant),
    }
}
