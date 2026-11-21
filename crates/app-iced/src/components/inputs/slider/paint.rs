//! 滑块的颜色求值（M234 从 `slider.rs` 的 `draw` 里搬出来，顺手收一个 [`Base`]）。
//!
//! 搬出来的两个理由：① `slider.rs` 已顶到 399 行，加 `.base(..)` 之前必须腾地方；
//! ② 禁用态那三笔颜色原来在 `draw` 里写死 `Role::Surface` 当合成底（M234 的病灶），
//! 搬进纯函数后可以被单测直接钉住「叠在哪个底上」。

use iced::Color;

use unlucky_design::color::Role;

use crate::theme::Ui;

use super::super::super::interaction::{self, Base, Interaction};
use super::super::geometry::HANDLE_WIDTH_ACTIVE;

/// 滑块一笔要的三个颜色 + handle 宽。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Colors {
    /// 激活段（左侧已走部分）。
    pub(super) active: Color,
    /// 未激活段。
    pub(super) inactive: Color,
    /// handle（4×44 竖条）。
    pub(super) handle: Color,
    /// handle 宽（focus / pressed 收窄）。
    pub(super) handle_width: f32,
}

/// 五态 → 三笔颜色（`HANDLE_WIDTH_ACTIVE` 只在 focus/pressed 用）。
///
/// 禁用（11.6）：激活段与 handle `OnSurface`@38%、未激活轨 @12%，三者都叠在 `base` 上
/// ——`base` 是滑块**脚下那一层**（卡片 = `surfaces::elevated::BASE`），不是恒定的画布色。
pub(super) fn colors(ui: &Ui, base: Base, state: Interaction, handle: f32) -> Colors {
    if state.is_disabled() {
        return Colors {
            active: interaction::disabled_content_over_role(ui, base, Role::OnSurface, false),
            inactive: interaction::disabled_colors(ui, base).0,
            handle: interaction::disabled_content_over_role(ui, base, Role::OnSurface, false),
            handle_width: handle,
        };
    }
    // 状态层只叠在 handle 上（D16）：α 全部来自 interaction（hover 8 / focus 10 / pressed 10）
    let handle_color = interaction::overlay_color(ui, state, Role::OnSurface, Role::Primary)
        .unwrap_or_else(|| ui.color(Role::Primary));
    let handle_width = match state {
        Interaction::Focused | Interaction::Pressed => HANDLE_WIDTH_ACTIVE,
        _ => handle,
    };
    Colors {
        active: ui.color(Role::Primary),
        inactive: ui.color(Role::SecondaryContainer),
        handle: handle_color,
        handle_width,
    }
}
