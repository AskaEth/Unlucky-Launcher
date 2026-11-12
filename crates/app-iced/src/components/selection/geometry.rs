//! 选择类家族的**几何**：值要么来自令牌，要么在此登记 G 编号（不散落在组件里）。
//!
//! ## 参考值 → 落点
//!
//! | # | 量 | 参考值 | 来源 | 落点 |
//! |---|---|---|---|---|
//! | 18.1 | Checkbox 外框 / 圆角 | 官方 **20×20** / **2**；**本项目取规格卡 18×18 / rx4** | `[源]` `Checkbox.kt:737-738` + 规格卡 `rect 18×18 rx4` | `inkcat::metrics::CHECKBOX` |
//! | 18.2 | Checkbox 描边 | **2**（选中时 0） | `[源]` `UnselectedOutlineWidth` / 规格卡 `stroke-width=2` | `inkcat::metrics::CHECKBOX_STROKE` |
//! | 18.5 | Checkbox 状态层 | **Ø40** | `[源]` `StateLayerSize` | `ComponentMetrics::CHECKBOX.state_layer` |
//! | 18.9 | 命中区 / 标签间距 | **48** / **8** | `[源]` `minimumInteractiveComponentSize` / 间距 `[估]` | `LayoutMetrics::touch_target` / `metrics.gap` |
//! | 19.1 | Radio 外圈 / 环线 | 官方 **20** / **2**；**本项目取规格卡 18 Ø** / 2 | `[源]` `IconSize` / `RadioStrokeWidth` + 规格卡 `r=9` | `inkcat::metrics::RADIO` |
//! | 19.2 | Radio 内点 | **10**（不是 12） | `[源]` `DotSize=12`，绘制半径 12/2−2/2=5；规格卡 `r=5` 同为 10 Ø | `inkcat::metrics::RADIO_DOT` |
//! | 10.1 | Switch 轨道 | **52×32**，圆角 full(16) | `[源]` `TrackWidth/TrackHeight/TrackShape` | [`SwitchGeometry`]（**G16**） |
//! | 10.2 | Switch 滑块 | 选中 **24** / 未选 **16** / 按下 **28** | `[源]` `Selected/Unselected/PressedHandleWidth` | [`SwitchGeometry`]（**G16**） |
//! | 13.1 | Chip 高 / 圆角 | **32** / **8** | `[源]` `ContainerHeight` / `ContainerShape=CornerSmall` | `Space::X8` + `Component::Chip.spec().shape` |
//!
//! **G16 登记**（续 `research/15` §25 的 G13）：`ComponentMetrics` 只有
//! `CHECKBOX/RADIO/SEARCH_BAR/NAVIGATION_DRAWER` 四组，**Switch 与 Chip 没有落点**。
//! 本文件把它们的量收成命名常量（每个都带 A1 出处），等令牌层补
//! `ComponentMetrics::SWITCH/CHIP` 后整段删除、改走 `Component::Switch.metrics()`。
//!
//! **G14 已收口（M86）**：Chip 图标取 `IconSize::Button` = **18**（AAR `AssistChipTokens.IconSize =
//! 18.0d`，与 12b 13.4 同值），不再是偏离。Switch 滑块内的图标是**另一个值 16**
//! （`SwitchTokens.SelectedIconSize / UnselectedIconSize = 16.0d`），所以走 `IconSize::SwitchThumb`：
//! 两者在官方就是两个 token，别为了省一个档位把它们合成一个。

use unlucky_design::component_map::{Component, ComponentMetrics};
use unlucky_design::icons::IconSize;
use unlucky_design::inkcat::metrics as inkcat;
use unlucky_design::spacing::Space;

use crate::theme::Ui;

/// Switch 轨道宽（`SwitchTokens.TrackWidth = 52.0`，**G16**：令牌层无此档）。
const SWITCH_TRACK_WIDTH: f32 = 52.0;
/// Switch 轨道描边（`SwitchTokens.TrackOutlineWidth = 2.0`，**G16**）。
const SWITCH_TRACK_STROKE: f32 = 2.0;
/// Switch 选中态滑块（`SwitchTokens.SelectedHandleWidth = 24.0`，**G16**）。
const SWITCH_THUMB_SELECTED: f32 = 24.0;
/// Switch 未选中态滑块（`SwitchTokens.UnselectedHandleWidth = 16.0`，**G16**）。
const SWITCH_THUMB_UNSELECTED: f32 = 16.0;
/// Switch 按下态滑块（`SwitchTokens.PressedHandleWidth = 28.0`，**G16**）。
const SWITCH_THUMB_PRESSED: f32 = 28.0;
/// Switch 状态层（`SwitchTokens.StateLayerSize = 40.0`，**G16**；与 Checkbox 同值）。
const SWITCH_STATE_LAYER: f32 = 40.0;

/// Checkbox / Radio 共用的方框几何（两控件同尺寸、不同形状语言，§18 / §19）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlGeometry {
    /// 可见外框边长（20）。
    pub side: f32,
    /// 描边 / 环线宽度（2；**禁用态不变细**，§18.8）。
    pub stroke: f32,
    /// 状态层圆的直径（40）。
    pub state_layer: f32,
    /// 最小命中区（48，`LayoutMetrics::touch_target`）。
    pub hit: f32,
    /// 框 ↔ 标签间距（8，团队口径 `[估]`）。
    pub gap: f32,
    /// 圆角半径：Checkbox = 2（`ComponentRadius::Micro`），Radio = 10（`ShapeScale::Full` 的真圆）。
    pub radius: f32,
    /// Radio 内点直径（10）；Checkbox 为 0（没有内点）。
    pub inner_dot: f32,
}

impl ControlGeometry {
    /// Checkbox：**18×18** / 圆角 **4** / 描边 2 / 状态层 Ø40（M229 取规格卡口径，见模块表）。
    ///
    /// 与官方 20×20 / 2dp 差 2px 与 2dp：规格卡画的是 `rect 18×18 rx4`，M228 的 `inkcat` 把这条
    /// 差异登记为「换肤时取 `CHECKBOX`」。4 是 `ShapeScale` 的刻度值，所以走容器层不违反 D32。
    pub fn checkbox(ui: &Ui) -> Self {
        let metrics = metrics(Component::Checkbox);
        let spec = inkcat::CHECKBOX;
        Self {
            side: spec.width,
            stroke: inkcat::CHECKBOX_STROKE,
            state_layer: metrics.state_layer,
            hit: ui.metrics().touch_target,
            gap: metrics.gap,
            radius: spec.radius_px(),
            // Checkbox 没有内点（内点是 radio 的语言，§19.6 明令两者不许混用）
            inner_dot: 0.0,
        }
    }

    /// Radio：**18 Ø** / 环线 2 / 内点 **10** / 状态层 Ø40；圆角是**真圆**（`ShapeScale::Full`）。
    pub fn radio(ui: &Ui) -> Self {
        let metrics = metrics(Component::Radio);
        let spec = inkcat::RADIO;
        Self {
            side: spec.width,
            stroke: inkcat::RADIO_RING,
            state_layer: metrics.state_layer,
            hit: ui.metrics().touch_target,
            gap: metrics.gap,
            radius: spec.radius_px(),
            // 内点 **10**（§19.2：12 是含描边前的名义值，绘制半径 = 12/2 − 2/2 = 5）。
            // 规格卡实测同样是 10，所以这里取 `inkcat::RADIO_DOT`。
            inner_dot: inkcat::RADIO_DOT,
        }
    }

    /// 命中区比可见状态层宽出来的部分（左右**各一份**）：`(48 − 40) / 2 = 4`。
    ///
    /// 无标签时行宽只有状态层的 40dp，命中区就少 8dp；A1 的
    /// `minimumInteractiveComponentSize` 是 **48**，所以 view 侧左右各留这段透明内边距
    /// （复审 P2-2）。有标签时行宽本来 ≥48，这段内边距不改变结论，只是保持两处口径一致。
    pub fn hit_padding(&self) -> f32 {
        (self.hit - self.state_layer) / 2.0
    }

    /// 勾形横向端点跨度（20 画布内 0.2→0.8）：`[源]` `Checkbox.kt:496-514` 的路径比例。
    ///
    /// 这是 research/15 §25 明确允许的「组件级常量」（路径比例 0.2/0.4/0.5/0.7/0.8 不进令牌层）。
    /// 只在取证/测试里用（生产路径由 `shell::glyph` 直接按比例构路径）。
    #[cfg(test)]
    pub fn check_span(&self) -> f32 {
        self.side * (0.8 - 0.2)
    }

    /// 方头（`StrokeCap.Square`）渲染后墨迹相对端点跨度的**外扩量**：两侧各 `1/√2 × 线宽`。
    ///
    /// 只用于把「参考值 12 是端点跨度、实测是墨迹跨度」这件事写进断言：勾的两端是 45° 斜线，
    /// 方头沿斜向外扩，投影到 x 轴正好是 `stroke / √2`（每端）。`#[cfg(test)]`：同 `check_span`。
    #[cfg(test)]
    pub fn square_cap_bleed(&self) -> f32 {
        self.stroke * std::f32::consts::FRAC_1_SQRT_2
    }
}

/// Switch 的几何（`research/12b` §10；数值标 **G16** 的部分令牌层暂无档位）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwitchGeometry {
    /// 轨道宽（52）。
    pub track_width: f32,
    /// 轨道高（32 = `Space::X8`）。
    pub track_height: f32,
    /// 轨道描边（2，仅未选中态画）。
    pub track_stroke: f32,
    /// 滑块：选中 / 未选中 / 按下三档（24 / 16 / 28）。
    pub thumb_selected: f32,
    /// 未选中滑块（16）。
    pub thumb_unselected: f32,
    /// 按下滑块（28）。
    pub thumb_pressed: f32,
    /// 滑块内图标（16 = `IconSize::SwitchThumb`；AAR `SwitchTokens.SelectedIconSize /
    /// UnselectedIconSize = 16.0d`。**不是按钮的 18**）。
    pub icon: f32,
    /// 状态层直径（40）。
    pub state_layer: f32,
    /// 最小命中区（48）。
    pub hit: f32,
}

impl SwitchGeometry {
    /// 从令牌 + G16 常量求值。
    pub fn of(ui: &Ui) -> Self {
        Self {
            track_width: SWITCH_TRACK_WIDTH,
            track_height: ui.space(Space::X8),
            track_stroke: SWITCH_TRACK_STROKE,
            thumb_selected: SWITCH_THUMB_SELECTED,
            thumb_unselected: SWITCH_THUMB_UNSELECTED,
            thumb_pressed: SWITCH_THUMB_PRESSED,
            icon: IconSize::SwitchThumb.px(),
            state_layer: SWITCH_STATE_LAYER,
            hit: ui.metrics().touch_target,
        }
    }

    /// 滑块直径：按下态胀到 28，其余按选中与否取 24 / 16（§10.2）。
    pub fn thumb(&self, selected: bool, pressed: bool) -> f32 {
        if pressed {
            self.thumb_pressed
        } else if selected {
            self.thumb_selected
        } else {
            self.thumb_unselected
        }
    }

    /// 滑块离轨道边的内距 = `(轨道高 − 滑块) / 2`（三个档位都成立：32−24=8→4，32−16=16→8，32−28=4→2）。
    pub fn thumb_inset(&self, thumb: f32) -> f32 {
        (self.track_height - thumb) / 2.0
    }

    /// 状态层相对轨道的横向外扩：状态层比轨道高，未选中时滑块又靠左，
    /// 所以状态层会**超出轨道左侧**——绘制区因此要多留这一段（见 [`Self::layer_offset`]）。
    pub fn bleed(&self) -> f32 {
        (self.state_layer - self.track_height) / 2.0
    }

    /// 滑块圆心到状态层左缘的距离（= 状态层半径）。
    pub fn layer_radius(&self) -> f32 {
        self.state_layer / 2.0
    }

    /// 滑块圆心离轨道左缘的距离：未选中 = 内距 + 半滑块；选中 = 轨道宽 − 内距 − 半滑块。
    pub fn thumb_center(&self, selected: bool, thumb: f32) -> f32 {
        let inset = self.thumb_inset(thumb);
        if selected {
            self.track_width - inset - thumb / 2.0
        } else {
            inset + thumb / 2.0
        }
    }

    /// 状态层左缘相对**绘制区**的位置：绘制区左缘 = 轨道左缘 − [`Self::bleed`]。
    pub fn layer_offset(&self, selected: bool, thumb: f32) -> f32 {
        self.thumb_center(selected, thumb) + self.bleed() - self.layer_radius()
    }

    /// 绘制区宽（轨道 + 两侧各一段状态层外扩）。
    pub fn draw_width(&self) -> f32 {
        self.track_width + self.bleed() * 2.0
    }

    /// 绘制区高（= 状态层直径，轨道在中间垂直居中）。
    pub fn draw_height(&self) -> f32 {
        self.state_layer
    }
}

/// Chip 的几何（`research/12b` §13）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChipGeometry {
    /// 容器高（32 = `Space::X8`，`AssistChipTokens.ContainerHeight`）。
    pub height: f32,
    /// 圆角（8 = `Component::Chip.spec().shape = ShapeScale::Small`）。
    pub radius: f32,
    /// 水平内边距（8 = `Space::X2`，`Chip.kt HorizontalElementsPadding`）。
    pub pad: f32,
    /// 元素间距（8，同上）。
    pub gap: f32,
    /// 图标边长（18 = `IconSize::Button`；AAR `AssistChipTokens.IconSize = 18.0d`，12b 13.4 同值）。
    pub icon: f32,
}

impl ChipGeometry {
    /// 从令牌求值（`shape` 档位来自组件表，半径按边长折算）。
    pub fn of(ui: &Ui) -> Self {
        let height = ui.space(Space::X8);
        let shape = Component::Chip.spec().shape;
        Self {
            height,
            radius: shape.radius_px(height),
            pad: ui.space(Space::X2),
            gap: ui.space(Space::X2),
            icon: IconSize::Button.px(),
        }
    }
}

/// 组件表里的微度量；缺组 = 令牌层还没建这一档（G16 的判据）。
fn metrics(component: Component) -> ComponentMetrics {
    component
        .metrics()
        .unwrap_or_else(|| panic!("{component:?} 缺 ComponentMetrics（G10 的消费入口）"))
}
