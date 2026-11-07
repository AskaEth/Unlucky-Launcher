//! 输入类家族的**内部几何**：能取令牌的一律取令牌，取不到的登记 G 编号后集中在这里。
//!
//! ## 为什么这个文件存在
//!
//! `components::gate` 的 `t2_no_size_literals_in_components` 扫 `.width(40.0)` 这类**调用点字面量**；
//! 但「TextField 高 56」这种 A 级参考值在 `LayoutMetrics` / `ComponentMetrics` 里确实没有落点
//! （令牌层是另一条 mission 的 scope，不许我改）。所以按契约 §6.2：**登记 G 编号 + 只在这个文件里
//! 定义一次**，组件实现一个数字都不写。
//!
//! ## 参考值表（`[源]` = A1，本机 `material3-android:1.4.0` 的 token 源码；`[估]` = 团队口径）
//!
//! | G | 名称 | 参考值 | 出处 | 本文件落点 |
//! |---|---|---|---|---|
//! | G16 | TextField 可见高（带标签） | **56** | `TextFieldDefaults.MinHeight` / `OutlinedTextFieldTokens.ContainerHeight` | [`FIELD_HEIGHT`] |
//! | G17 | 聚焦/错误描边 | **2** | `OutlinedTextFieldTokens.FocusOutlineWidth`、`FilledTextFieldTokens.FocusActiveIndicatorHeight` | [`FOCUS_STROKE`] |
//! | G18 | Slider 轨道 / 竖条 / 停靠点 | **16 / 4×44 / 4** | `SliderTokens.ActiveTrackHeight` / `HandleWidth`+`HandleHeight` / `StopIndicatorSize` | [`TRACK_HEIGHT`] 等 |
//! | G19 | 菜单项高 | **48** | `Menu.kt:524 MenuListItemContainerHeight` | [`MENU_ITEM_HEIGHT`] |
//! | G20 | Search 宽高 | **min 360 / max 720 / 面板最小 240** | `SearchBar.kt:2712-2713`、`DockedExpandedTableMinHeight` | [`SEARCH_MIN_WIDTH`] 等 |
//!
//! **已经能取令牌、不许在这里重复定义的**（同一数值只许一份，见 `ComponentMetrics` 模块文档）：
//! 搜索条高 56 = `LayoutMetrics::search_bar_height`；无标签单行 40 = `LayoutMetrics::control_height`；
//! 命中区 48 = `LayoutMetrics::touch_target`；水平/上下内边距与图标间距 = `Space::{X2,X3,X4}`；
//! 图标 24 = `IconSize::Toolbar`；圆角 = `ComponentSpec::shape`（TextField/Menu = 4，Search = Full，
//! SearchView = ExtraLarge 28）；一像素线 = `stroke::HAIRLINE`。
//!
//! ## 与文档的两处口径冲突（按「代码优先」处理，报告里有偏离登记）
//!
//! 1. `12b §8.1` 说菜单圆角「本项目 12（偏离官方 4）」；令牌表 `Component::Menu.spec().shape`
//!    是 `ExtraSmall`（**4**）。令牌层是 M69 定案的真源，本实现取 **4**，不取 12。
//! 2. `12b §9.1` 说「无标签单行 40」是登记偏离——40 正好是 `LM.control_height`，
//!    所以那条偏离在实现上是「用骨架令牌」而不是「用字面量」。

use iced::Padding;

use unlucky_design::component_map::Component;
use unlucky_design::icons::{IconKind, IconSize};
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;

use crate::theme::{Ui, STROKE_PX};

/// G16：文本字段可见高（带浮动标签）。
pub const FIELD_HEIGHT: f32 = 56.0;

/// G17：聚焦/错误态的描边与指示线宽（官方 2dp；令牌层的 `stroke::HAIRLINE` 只有 1dp）。
pub const FOCUS_STROKE: f32 = 2.0;

/// G18：Slider 轨道高（激活/未激活同值）。
pub const TRACK_HEIGHT: f32 = 16.0;

/// G18：Slider 竖条 handle 宽（静息 / hover）。
pub const HANDLE_WIDTH: f32 = 4.0;

/// G18：Slider 竖条 handle 宽（focus / pressed）——A1 在这两态收窄到 2。
pub const HANDLE_WIDTH_ACTIVE: f32 = 2.0;

/// G18：Slider 竖条 handle 高（纵向溢出轨道，上下各 14）。
pub const HANDLE_HEIGHT: f32 = 44.0;

/// G18：离散停靠点直径。
pub const STOP_SIZE: f32 = 4.0;

/// G18：停靠点与轨道段之间的空隙（官方 `StopIndicatorTrailingSpace = 6`；6 不在 4px 栅格上）。
pub const STOP_GAP: f32 = 6.0;

/// G19：菜单项高。
pub const MENU_ITEM_HEIGHT: f32 = 48.0;

/// G20：搜索条最小宽（列宽不足时**不许压窄**，降级为 40 高 TextField 或 icon 入口）。
pub const SEARCH_MIN_WIDTH: f32 = 360.0;

/// G20：搜索条最大宽。
pub const SEARCH_MAX_WIDTH: f32 = 720.0;

/// G20：展开面板最小高（空结果也不许矮于它）。
pub const SEARCH_VIEW_MIN_HEIGHT: f32 = 240.0;

/// 文本字段的几何（Filled / Outlined 共用；差别只在颜色与形状）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldGeometry {
    /// 可见高：带标签 [`FIELD_HEIGHT`]，无标签 `LM.control_height`。
    pub height: f32,
    /// 内容内边距（无图标 16/16；有前置图标 12/16，图标与文本间距另算）。
    pub padding: Padding,
    /// 前置图标可视左边距（无偏移，就是内边距 12）。
    pub icon_inset: f32,
    /// 图标 ↔ 文本间距（12：12 + 24 + 12 = 48 起文本，与官方 48 的图标槽一致）。
    pub icon_gap: f32,
    /// 图标边长（24）。
    pub icon: f32,
    /// 静息描边（一像素线）。
    pub stroke: f32,
    /// 聚焦/错误描边（2）。
    pub focus_stroke: f32,
    /// 圆角档位（`Component::TextField` = ExtraSmall）。
    pub shape: ShapeScale,
}

impl FieldGeometry {
    /// 求值；`labeled` = 带浮动标签（56），否则单行 40；`leading` = 有没有前置图标。
    pub fn of(ui: &Ui, labeled: bool, leading: Option<IconKind>) -> Self {
        let icon_inset = ui.space(Space::X3);
        let pad_end = ui.space(Space::X4);
        let padding = if leading.is_some() {
            Padding {
                top: 0.0,
                right: pad_end,
                bottom: 0.0,
                left: icon_inset,
            }
        } else {
            Padding {
                top: 0.0,
                right: pad_end,
                bottom: 0.0,
                left: pad_end,
            }
        };

        Self {
            height: if labeled {
                FIELD_HEIGHT
            } else {
                ui.metrics().control_height
            },
            padding,
            icon_inset,
            icon_gap: ui.space(Space::X3),
            icon: IconSize::Toolbar.px(),
            stroke: STROKE_PX,
            focus_stroke: FOCUS_STROKE,
            shape: Component::TextField.spec().shape,
        }
    }
}

/// Slider 的几何（轨道 / 竖条 / 停靠点 / 命中区）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SliderGeometry {
    /// 轨道高（16）。
    pub track: f32,
    /// 竖条宽（静息 4 / focus·pressed 2）。
    pub handle: f32,
    /// 竖条高（44）。
    pub handle_tall: f32,
    /// 停靠点直径（4）。
    pub stop: f32,
    /// 控件高 = 命中区（`LM.touch_target` 48：竖条 44 + 上下各 2）。
    pub height: f32,
}

impl SliderGeometry {
    /// 从令牌求值。
    pub fn of(ui: &Ui) -> Self {
        Self {
            track: TRACK_HEIGHT,
            handle: HANDLE_WIDTH,
            handle_tall: HANDLE_HEIGHT,
            stop: STOP_SIZE,
            height: ui.metrics().touch_target,
        }
    }
}

/// 菜单（Menu / DropdownMenuItem）的几何。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MenuGeometry {
    /// 项高（48）。
    pub item_height: f32,
    /// 水平内边距（12，来自 `Space::X3`）。
    pub pad_h: f32,
    /// 上下留白（8，来自 `Space::X2`）。
    pub pad_v: f32,
    /// 槽位图标边长（24）。
    pub icon: f32,
    /// 图标 ↔ 文本间距（12）。
    pub gap: f32,
    /// 容器圆角档位（`Component::Menu` = ExtraSmall 4）。
    pub shape: ShapeScale,
}

impl MenuGeometry {
    /// 从令牌求值。
    pub fn of(ui: &Ui) -> Self {
        Self {
            item_height: MENU_ITEM_HEIGHT,
            pad_h: ui.space(Space::X3),
            pad_v: ui.space(Space::X2),
            icon: IconSize::Toolbar.px(),
            gap: ui.space(Space::X3),
            shape: Component::Menu.spec().shape,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unlucky_design::color::SchemeFlavor;
    use unlucky_design::typography::Platform;

    use crate::theme::Families;

    const FAMILIES: Families = Families {
        text: "Segoe UI",
        display: "Segoe UI",
        mono: "Consolas",
    };

    fn ui() -> Ui {
        Ui::with_platform(SchemeFlavor::Light, Platform::Windows, FAMILIES)
    }

    /// G16 / G17：两个 2dp 与 56dp 的参考值钉在这里（改参考表会让这条红）。
    #[test]
    fn field_geometry_matches_the_reference_table() {
        let ui = ui();
        let labeled = FieldGeometry::of(&ui, true, None);
        assert_eq!(labeled.height, 56.0, "G16 TextFieldDefaults.MinHeight");
        assert_eq!(labeled.stroke, 1.0, "静息一像素线");
        assert_eq!(labeled.focus_stroke, 2.0, "G17 FocusOutlineWidth");
        assert_eq!(labeled.shape.dp(), 4.0, "CornerExtraSmall");

        // 无标签单行 40 = 骨架令牌，不是字面量
        let compact = FieldGeometry::of(&ui, false, None);
        assert_eq!(compact.height, ui.metrics().control_height);
        assert_eq!(compact.height, 40.0);

        // 内边距：无图标 16/16
        assert_eq!(compact.padding.left, 16.0);
        assert_eq!(compact.padding.right, 16.0);
        assert_eq!(compact.padding.left, compact.padding.right);
    }

    /// 前置图标：图标可视边 12、文本从 48 起（= 12 + 24 + 12，官方 48 的图标槽）。
    #[test]
    fn field_icon_slot_matches_the_official_48() {
        let ui = ui();
        let leading = FieldGeometry::of(&ui, true, Some(IconKind::Search));
        assert_eq!(leading.icon, 24.0, "LeadingIconSize");
        assert_eq!(leading.icon_inset, 12.0, "textFieldHorizontalIconPadding");
        // 文本起点 = 图标可视边 12 + 图标 24 + 间距 12 = 48（官方图标槽）
        assert_eq!(leading.icon_inset + leading.icon + leading.icon_gap, 48.0);
        // 右侧仍是 16（官方内边距不因图标变）
        assert_eq!(leading.padding.right, 16.0);
    }

    /// G18：轨道 16 / 竖条 4×44 / 停靠点 4 / 命中 48；竖条比轨道高 28 ⇒ 上下各溢出 14。
    #[test]
    fn slider_geometry_matches_the_reference_table() {
        let ui = ui();
        let slider = SliderGeometry::of(&ui);
        assert_eq!(slider.track, 16.0, "ActiveTrackHeight");
        assert_eq!(slider.handle, 4.0, "HandleWidth");
        assert_eq!(slider.handle_tall, 44.0, "HandleHeight");
        assert_eq!(slider.stop, 4.0, "StopIndicatorSize");
        assert_eq!(
            (slider.handle_tall - slider.track) / 2.0,
            14.0,
            "上下各溢出"
        );
        assert_eq!(slider.height, 48.0, "命中下限 LM.touch_target");
        assert!(slider.height >= slider.handle_tall, "命中区必须容得下竖条");
        // focus / pressed 收窄到 2（A1 FocusHandleWidth / PressedHandleWidth）
        assert_eq!(HANDLE_WIDTH_ACTIVE, 2.0);
        assert!(HANDLE_WIDTH_ACTIVE < slider.handle);
    }

    /// G19：项高 48、水平 12、上下 8、图标 24、圆角按令牌表（4，**不是** 12b 说的 12）。
    #[test]
    fn menu_geometry_matches_the_reference_table() {
        let ui = ui();
        let menu = MenuGeometry::of(&ui);
        assert_eq!(menu.item_height, 48.0, "MenuListItemContainerHeight");
        assert_eq!(menu.pad_h, 12.0, "DropdownMenuItemHorizontalPadding");
        assert_eq!(menu.pad_v, 8.0, "DropdownMenuVerticalPadding");
        assert_eq!(menu.icon, 24.0);
        assert_eq!(
            menu.shape.dp(),
            4.0,
            "MenuTokens.ContainerShape = CornerExtraSmall"
        );
        assert_eq!(
            menu.shape,
            Component::Menu.spec().shape,
            "圆角只能来自令牌表"
        );
        assert_eq!(menu.gap, ui.space(Space::X3), "图标 ↔ 文本 12 走栅格令牌");
    }

    /// G20：搜索条 min 360 / max 720；面板最小高 240；条高取骨架令牌 56（**不在这里重复定义**）。
    #[test]
    fn search_constants_do_not_duplicate_layout_metrics() {
        let ui = ui();
        assert_eq!(SEARCH_MIN_WIDTH, 360.0);
        assert_eq!(SEARCH_MAX_WIDTH, 720.0);
        assert_eq!(SEARCH_VIEW_MIN_HEIGHT, 240.0);
        assert_eq!(ui.metrics().search_bar_height, 56.0, "条高来自 LM");
        assert_eq!(
            ShapeScale::Full.radius_px(ui.metrics().search_bar_height),
            28.0
        );
        assert_eq!(Component::Search.spec().shape, ShapeScale::Full);
        // 图标可视边 = pad + offset（M69 的 SEARCH_BAR 不变量，本文件不重复定义 12/4）
        let bar = Component::Search.metrics().expect("Search 有组件级度量");
        assert_eq!(bar.pad_start + bar.icon_offset, 16.0);
        assert_eq!(bar.icon, 24.0);
    }
}
