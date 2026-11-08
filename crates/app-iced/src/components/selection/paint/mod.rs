//! 选择类家族的**角色映射**：`Interaction × 选中 × 错误 → 颜色`（公共件 + 各组件分文件）。
//!
//! ## 出处（全部 A1：`material3-android:1.4.0` 的 token 类，本机 AAR `javap` 复核，见报告）
//!
//! | 组件 | 静息 | hover / focus / pressed | 禁用 |
//! |---|---|---|---|
//! | Checkbox 未选中 | 描边 `OnSurfaceVariant` 2px | 描边提到 **`OnSurface`**（宽度仍 2） | 描边 `OnSurface`@38% |
//! | Checkbox 选中 | 填 `Primary` + 勾 `OnPrimary` | 填 `Primary`（不变） | 填 `OnSurface`@38% + 勾 `Surface` |
//! | Radio | 环 `OnSurfaceVariant` / 选中环+点 `Primary` | 未选中环提到 **`OnSurface`**；**选中态仍是 `Primary`** | 环+点 `OnSurface`@38% |
//! | Switch | 轨 `Primary`（选中）/`SurfaceContainerHighest`+2px `Outline`；滑块 `OnPrimary`/`Outline` | 滑块 `PrimaryContainer`/`OnSurfaceVariant` | 轨 `OnSurface`@12%；滑块 `Surface`/@38% |
//! | Chip | 未选中：透明+1px `OutlineVariant`，文字 `OnSurfaceVariant`；选中：`SecondaryContainer` **无描边** | 描边不变，状态层成为底色 | 描边 `OnSurface`@12%；文字/图标 @38% |
//!
//! **A1 改过参考表的三处**：`UnselectedHover/Focus/PressedOutlineColor` 与 Radio 的
//! `UnselectedHover/Focus/PressedIconColor` 都是 `OnSurface`（不是 `OnSurfaceVariant`）；
//! `SelectedDisabledIconColor` 是 `Surface`。α 一律从 [`interaction`] 取。
//!
//! ## 为什么拆成目录（复审 P2-5）
//!
//! 原来一个文件正好 400 行、零余量：再加一个态就得动它。按组件拆成
//! `check.rs` / `radio.rs` / `switch.rs` / `chip.rs`，公共件（[`Frame`]、状态层、禁用色）留在本文件，
//! **对外 API 一字不改**（`selection::paint::*` 的路径全部保持）。

use iced::widget::container;
use iced::{Background, Border, Color};

use unlucky_design::color::Role;

use crate::theme::Ui;

use super::super::interaction::{self, Interaction};

mod check;
mod chip;
mod radio;
mod switch;

pub use check::{checkbox, checkbox_layer_role};
pub use chip::{chip, ChipVariant};
pub use radio::{radio, radio_layer_role};
pub use switch::{switch, switch_layer};

/// 一个「方框」的外观：填色 / 描边 / 线宽 / 半径。
///
/// Checkbox 的框、Radio 的环、Switch 的轨道、Chip 的容器都用它——差别只在颜色与线宽。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    /// 填充色；`None` = 透明（未选中的 Checkbox / Radio 外圈、Chip 的 flat 底）。
    pub fill: Option<Color>,
    /// 描边色；`None` = 不画描边。
    pub outline: Option<Color>,
    /// 描边宽度（Checkbox/Radio 2，Chip 1，Switch 未选中 2）。
    pub stroke: f32,
    /// 圆角半径（Checkbox 2 / Radio 10 / Switch 16 / Chip 8）。
    pub radius: f32,
}

impl Frame {
    /// 翻成 iced 的 `container::Style`（唯一把 Frame 变成像素的地方）。
    pub fn container_style(self) -> container::Style {
        container::Style {
            background: self.fill.map(Background::Color),
            border: Border {
                color: self.outline.unwrap_or(Color::TRANSPARENT),
                width: self.outline.map_or(0.0, |_| self.stroke),
                radius: iced::border::Radius::new(self.radius),
            },
            ..container::Style::default()
        }
    }
}

/// Checkbox 的外观：框 + 勾的颜色。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CheckboxPaint {
    /// 框（20×20）。
    pub frame: Frame,
    /// 勾 / 短横。
    pub glyph: Color,
}

/// Radio 的外观：环 + 内点。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadioPaint {
    /// 外圈（20×20，环线向内收 → 外缘恒 20）。
    pub ring: Frame,
    /// 内点（Ø10）；`None` = 未选中（不画点）。
    pub dot: Option<Color>,
}

/// Switch 的外观：轨道 + 滑块 + 滑块内图标。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwitchPaint {
    /// 轨道（52×32）。
    pub track: Frame,
    /// 滑块。
    pub thumb: Color,
    /// 滑块内图标（16）；`None` = 不画（未选中时官方色与轨道同色，画了也看不见）。
    pub icon: Option<Color>,
}

/// Chip 的外观：容器 + 文字/图标。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChipPaint {
    /// 容器。
    pub frame: Frame,
    /// 文字。
    pub label: Color,
    /// 前/后置图标。
    pub icon: Color,
}

/// 状态层颜色：`content` 角色 @ 该态的 α 叠在 `surface` 上（不透明；α 的唯一实现点是
/// [`interaction::overlay_color`]，本目录零 α 字面量）。
pub fn state_layer(ui: &Ui, state: Interaction, content: Role) -> Option<Color> {
    interaction::overlay_color(ui, state, content, Role::Surface)
}

/// `OnSurface`@12% 叠 surface（**容器**禁用色：Switch 轨道、Chip 未选中描边）。
///
/// 注意：Checkbox「选中且禁用」的**填充**不是这个 12%，A1 的
/// `SelectedDisabledContainerOpacity = 0.38` → 见 [`disabled_ink`]（复审 P2-3）。
pub(super) fn disabled_container(ui: &Ui) -> Color {
    interaction::disabled_colors(ui, Role::Surface).0
}

/// 内容禁用色：`role` @38% 叠在 surface 上（`on_container = false`）。
pub(super) fn disabled_ink(ui: &Ui, role: Role) -> Color {
    interaction::disabled_content_over_role(ui, role, false)
}

/// hover / focus / pressed（「抬起来」的三态）。
pub(super) fn lifted(state: Interaction) -> bool {
    matches!(
        state,
        Interaction::Hovered | Interaction::Focused | Interaction::Pressed
    )
}

/// 控件旁的标签色：`OnSurface`；禁用 → `OnSurface`@38%。
///
/// A1 里 checkbox/radio 都没有「标签色随 hover 变」的字段（标签不属于控件本体），
/// 所以这里只看禁用与否。
pub fn label(ui: &Ui, state: Interaction) -> Color {
    if state.is_disabled() {
        disabled_ink(ui, Role::OnSurface)
    } else {
        ui.color(Role::OnSurface)
    }
}
