//! 三件「面」的几何：Snackbar / Dialog / EmptyState（数值与出处见 [`super::geometry`] 的 G17 表）。
//!
//! 为什么与进度分家：进度那两个是**带节奏的图形**（keyframes 与相位），这三件是**静态盒子**
//! （宽高/内距/圆角）。合在一个文件里会顶到 400 行红线（AGENTS.md §3-1）。

use unlucky_design::component_map::Component;
use unlucky_design::inkcat::metrics as inkcat;
use unlucky_design::spacing::Space;

use crate::theme::Ui;

/// Snackbar 的几何（G17-c/d + A1 高/内距/上限）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnackbarGeometry {
    /// 最小宽度（344，`[估]`；iced 无 min-width，见组件文档）。
    pub min_width: f32,
    /// 最大宽度（600，`[源]` `SnackbarKt.ContainerMaxWidth`）。
    pub max_width: f32,
    /// 单行高（48）。
    pub single_height: f32,
    /// 双行高（68）。
    pub two_height: f32,
    /// 距宿主边距（12，`[规格]`）。
    pub margin: f32,
    /// 文本水平内距（16）。
    pub pad_h: f32,
    /// 文本垂直内距（6）。
    pub pad_v: f32,
    /// 文本 ↔ 动作间距（8）。
    pub gap: f32,
    /// 圆角（4）。
    pub radius: f32,
}

impl SnackbarGeometry {
    /// 从令牌求值。
    ///
    /// M229 换肤把**高与圆角**改成规格卡口径（`inkcat::metrics::SNACKBAR`：340×**52**、rx=**16**）：
    /// 官方 `SnackbarTokens` 是 48 高 + `CornerExtraSmall(4)`，规格卡画成 52/16 —— M228 已把这条
    /// 差异登记为「换肤时取 `SNACKBAR`」。双行高仍按官方「每多一行 +20」推成 **72**（52 + 20），
    /// 与单行同 delta；宽上限 600 与边距 12 不动（它们不在规格卡上）。
    pub fn of(ui: &Ui) -> Self {
        let spec = inkcat::SNACKBAR;
        let single_height = spec.height;
        Self {
            min_width: 344.0,
            max_width: 600.0,
            single_height,
            two_height: single_height + 20.0,
            margin: ui.space(Space::X3),
            pad_h: ui.space(Space::X4),
            pad_v: 6.0,
            gap: ui.space(Space::X2),
            radius: spec.radius_px(),
        }
    }

    /// 按行数取高：`lines >= 2` → 68，否则 48（官方 `heightIn(min=...)` 的等价口径）。
    pub fn height_for(&self, lines: u8) -> f32 {
        if lines >= 2 {
            self.two_height
        } else {
            self.single_height
        }
    }
}

/// Dialog 的几何。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DialogGeometry {
    /// 本项目宽度（480，落在官方 280..560 内）。
    pub width: f32,
    /// 官方下限（280）。
    pub min_width: f32,
    /// 官方上限（560）。
    pub max_width: f32,
    /// 圆角（28）。
    pub radius: f32,
    /// 四边内边距（24）。
    pub padding: f32,
    /// 标题下间距（16）。
    pub title_gap: f32,
    /// 正文下间距（24）。
    pub body_gap: f32,
    /// 动作主轴间距（8）。
    pub action_gap: f32,
    /// 动作交叉间距（12）。
    pub action_cross: f32,
}

impl DialogGeometry {
    /// 可用宽 → 实际渲染宽：`min(本项目 480, 可用宽)`，再**尊重官方下限 280**。
    ///
    /// 复审 E：早先 `Dialog::width(200)` 会真的画 200（官方 `DialogMinWidth = 280` 只在测试里）。
    /// 可用宽 < 280 时仍画 280（**溢出宿主**）—— iced 没有 min-width，这条是 G17-d 之二。
    pub fn effective_width(&self, available: f32) -> f32 {
        available.min(self.width).max(self.min_width)
    }

    /// 从令牌求值（内边距与三个间距全部落在 4px 栅格上 ⇒ 走 `Space`）。
    pub fn of(ui: &Ui) -> Self {
        let width = 480.0;
        Self {
            width,
            min_width: 280.0,
            max_width: 560.0,
            radius: Component::Dialog.spec().shape.radius_px(width),
            padding: ui.space(Space::X6),
            title_gap: ui.space(Space::X4),
            body_gap: ui.space(Space::X6),
            action_gap: ui.space(Space::X2),
            action_cross: ui.space(Space::X3),
        }
    }
}

/// 空态图标边长（`IconSize::Empty` = 32）。
///
/// M86 起 `icons::view_at` 会按请求尺寸**重新烘焙 stroke**，所以 32 渲染仍是视觉 1.5px 线宽
/// （早先 `view` 把 svg 钉成 24，32 的盒子只是居中不拉伸，实渲 24 —— 那个欠账已随 M86 消解）。
pub const EMPTY_ICON: f32 = 32.0;

/// 空态的几何（**项目自定**：官方没有这个组件；`[估]` 几何 + `[规格]` 排版 + `[源]` CTA）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EmptyStateGeometry {
    /// 图标块边长（96）。
    pub block: f32,
    /// 图标块圆角（16，`ShapeScale::Large`）。
    pub block_radius: f32,
    /// 块内图标（**32**，`IconSize::Empty`；由 `icons::view_at` 按 32 烘焙，stroke 同步重算）。
    pub icon: f32,
    /// 块 ↔ 标题（16）。
    pub gap_title: f32,
    /// 标题 ↔ 副文案（8）。
    pub gap_body: f32,
    /// 副文案 ↔ 动作（24）。
    pub gap_action: f32,
}

impl EmptyStateGeometry {
    /// 从令牌求值（圆角档位与图标档位是真令牌，96 / 三个间距是项目自定）。
    pub fn of(ui: &Ui) -> Self {
        let block = 96.0;
        Self {
            block,
            block_radius: Component::EmptyState.spec().shape.radius_px(block),
            icon: EMPTY_ICON,
            gap_title: ui.space(Space::X4),
            gap_body: ui.space(Space::X2),
            gap_action: ui.space(Space::X6),
        }
    }
}
