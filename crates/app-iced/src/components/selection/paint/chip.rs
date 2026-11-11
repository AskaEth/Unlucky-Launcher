//! Chip 的角色映射（`research/12b` §13.5 / §13.6 / §13.7）。

use unlucky_design::color::Role;

use crate::theme::{Ui, STROKE_PX};

use super::super::super::interaction::{Base, Interaction};
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
    base: Base,
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
        let ink = disabled_ink(ui, base, Role::OnSurface);
        return ChipPaint {
            frame: Frame {
                fill: selected.then(|| disabled_container(ui, base)),
                outline: (!selected).then(|| disabled_container(ui, base)),
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
        state_layer(ui, base, state, label_role)
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

/// 「**当前项**」标记的外观（M206）：**就是选中 chip 的那一套**，一个像素都不另起一套。
///
/// 为什么要一个具名入口而不是让调用点自己拼 `chip(.., Filter, true, Enabled, ..)`：
///
/// 1. 这个语义（「这行就是当前生效的那个」）此前被画成了 [`super::super::Chip::badge`] 的
///    **禁用**配色（`OnSurface`@38% 叠 surface，M204 实测 **1.84:1 浅 / 2.28:1 深**）——那是
///    「暂不可用」的语汇，而当前项不是不可用，它是**已经选中的那个**。修法不是给禁用配色调亮，
///    而是让当前项改用**选中**的语汇；
/// 2. 与左侧导航里相邻的「下载」组的选中 chip（`Chip::filter(.., true, ..)`，容器
///    `SecondaryContainer`、文字 `OnSecondaryContainer`，实测 **7.21:1 浅 / 7.21:1 深**）
///    **同族**：两页表达「选中」只用一套颜色，用户不用在两页之间切换心智模型。身份由
///    `pages::settings::java::tests::marker` 的
///    `the_current_default_marker_speaks_the_download_groups_language` 钉住——谁把其中一页
///    改成另一套配色，那条立刻红。具体的 8 bit 色值不写在这里：`tests/no_hardcoded_tokens.rs`
///    连注释里的十六进制色值也拦，色值只该在报告与令牌表里出现。
///
/// 与选中 chip 的唯一差别**不在颜色**：标记没有消息（`on_press == None`），所以它不吃 hover /
/// pressed / disabled 五态（见 `Chip::current` 的文档）。对比度下限是 [`crate::components::selection::paint`]
/// 同一条正文口径（WCAG AA 4.5:1，`unlucky_design::palette::wcag::AA_TEXT`）。
///
/// 与选中 chip 的唯一差别**不在颜色**：标记没有消息（`on_press == None`），所以它不吃 hover /
/// pressed / disabled 五态（见 `Chip::current` 的文档）。对比度下限是 [`crate::components::selection::paint`]
/// 同一条正文口径（WCAG AA 4.5:1，`unlucky_design::palette::wcag::AA_TEXT`）。
pub fn current_item(ui: &Ui, radius: f32) -> ChipPaint {
    chip(
        ui,
        Base::CANVAS,
        ChipVariant::Filter,
        true,
        Interaction::Enabled,
        radius,
    )
}
