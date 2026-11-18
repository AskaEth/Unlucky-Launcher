//! 导航类家族的**几何**：能取令牌的取令牌，取不到的在此登记 G 编号（不散落在组件里）。
//!
//! ## 参考值 → 落点（`[源]` = A1，`material3-android:1.4.0`；本轮逐条用 `javap` 读本机 AAR 复核）
//!
//! ```text
//! javap -p -c androidx.compose.material3.tokens.ListTokens          → 56/72/88、16/16、图标 24
//! javap -p -c androidx.compose.material3.tokens.NavigationDrawerTokens
//!      → ActiveIndicatorHeight 56、ActiveIndicatorWidth 336、ContainerWidth 360、CornerLargeEnd
//! javap -p -c androidx.compose.material3.NavigationDrawerItemDefaults
//!      → ItemPadding = 12（bipush 12 → PaddingValues(12f, 0f)）
//! javap -p -c androidx.compose.material3.tokens.PrimaryNavigationTabTokens
//!      → ContainerHeight 48、IconAndLabelTextContainerHeight 64、ActiveIndicatorHeight 3、图标 24
//! javap -p -c androidx.compose.material3.tokens.SecondaryNavigationTabTokens → ContainerHeight 48
//! javap -p -c androidx.compose.material3.tokens.AppBarSmallTokens   → ContainerHeight 64
//! ```
//!
//! | # | 量 | 参考值（A1） | 本项目 | 来源 | 落点 |
//! |---|---|---|---|---|---|
//! | 4.1 | ListItem 一行高 | **56** | **44** = `LM.row_height`（登记偏离 −12） | `[源]` `ListItemOneLineContainerHeight` | [`ListItemGeometry::heights`] |
//! | 4.2 | ListItem 两/三行高 | **72 / 88** | **60 / 76**（每多一行 +16，与官方同 delta） | `[源]` | 同上 |
//! | 4.3 | ListItem 左右内边距 | **16 / 16** | 同 | `[源]` `ListItemLeading/TrailingSpace` | `pad_start` / `pad_end` |
//! | 4.4 | ListItem leading 图标 | **24** | 同 | `[源]` `ListItemLeadingIconSize` | `icon`（`IconSize::Toolbar`） |
//! | 4.4b | 图标 ↔ 文本间距 | 无 token | **16** | `[估]`（Compose 由 ListItem 内部排版决定） | `gap`（`Space::X4`） |
//! | 6.1 | Tabs 容器高 | **48**（带图标 **64**） | 同 | `[源]` | [`TABS_HEIGHT`]（**G22**：`LM` 无此档） |
//! | 6.2 | 指示条 | 高 **3**、圆角 **3**、`Primary` | 同 | `[源]` `ActiveIndicatorHeight` + `RoundedCornerShape(3dp)` | [`TABS_INDICATOR`]（**G23**） |
//! | 6.6 | Secondary 分隔线 | **1dp `SurfaceVariant`** | 同 | `[源]` `DividerHeight` / `DividerColor` | [`TabsGeometry::secondary_bar`] |
//! | 21.4 | drawer 项高 | **56**（`heightIn(min = ActiveIndicatorHeight)`） | **56**（`LM.nav_item_height` 是 44，登记偏离 +12） | `[源]` | [`DRAWER_ITEM_HEIGHT`]（**G24**） |
//! | 21.5 | drawer 指示器 | 宽 = 容器宽 − **24**（360 → 336）；高 = 项高；圆角 full | 240 − 24 = **216** | `[源]` `ItemPadding = 12` | [`NavPaneGeometry::pill_width`] |
//! | 21.6 | drawer 项内 | 项外 **12**；内 start **16** / end **24**；图标 24；图标↔label **12**；label `LabelLarge` | 同 | `[源]` `ItemPadding` + `navigation/15 §21.6` | [`ComponentMetrics::NAVIGATION_DRAWER`] |
//! | 21.2 | drawer 形状 | **`CornerLargeEnd`**（只有靠内容那侧两角 16，贴边侧直角） | 同（右两角 16） | `[源]` `ContainerShape` | [`NavPaneGeometry::radius`] |
//! | 16.1 | AppBar small 高 | **64** | **56** = `LM.page_header_height`（登记偏离 −8） | `[源]` `AppBarSmallTokens.ContainerHeight` | [`AppBarGeometry::height`] |
//! | 16.5/16.6 | AppBar 图标 / 左右内边距 | **24** / **4** | 同 | `[源]` `AppBarTokens.IconSize/LeadingSpace/TrailingSpace` | [`AppBarGeometry`] |
//!
//! **G 编号**（续 `research/15` §25 的 G13；G16~G20 已被 selection/inputs 用掉）：
//!
//! - **G22** Tabs 容器高 48 / 带图标 64：`LayoutMetrics` 无此档，收在本文件的 [`TABS_HEIGHT`]；
//! - **G23** Tabs 指示条高 3 / 最小项宽 90：同上，收在本文件 [`TABS_INDICATOR`]，
//!   而**最小项宽 90（`TabMinWidth`）本轮不施加**——iced 无 min-width，tab 按内容 hug，
//!   指示条因此严格等于「该 tab 的内容宽 + 2×16」，见 [`TabsGeometry::pad`] 的说明；
//! - **G24** drawer 项高 56：令牌 `LM.nav_item_height = 44` 是**列表行**的密集档，drawer 项按 A1 取 56。
//!
//! ## 两个「同一个容器两个宽度」（D33）
//!
//! [`NavPaneGeometry::of`] 是**唯一**一处同时知道 88 与 240 的地方：宽 → 药丸宽、项高、内边距
//! 全部由 `layout` 派生。Q8 的历史事故（药丸 56×32 被 `center_x(Fill)` 撑成 88×32）就出在
//! 「药丸宽度不是从容器宽派生」——这里把派生写成函数，组件层拿不到手写数字。

use iced::border::Radius;

use unlucky_design::component_map::{Component, ComponentMetrics};
use unlucky_design::icons::IconSize;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;

use crate::theme::Ui;

/// Tabs 容器高（A1 `PrimaryNavigationTabTokens.ContainerHeight`，**G22**）。
pub const TABS_HEIGHT: f32 = 48.0;
/// 带图标的 Tabs 容器高（A1 `IconAndLabelTextContainerHeight`，**G22**；本轮只做文字档）。
pub const TABS_ICON_HEIGHT: f32 = 64.0;
/// Tabs 指示条高（A1 `PrimaryNavigationTabTokens.ActiveIndicatorHeight`，**G23**）。
pub const TABS_INDICATOR: f32 = 3.0;
/// drawer 项高（A1 `NavigationDrawerItemDefaults` 的 `heightIn(min = ActiveIndicatorHeight)`，**G24**）。
pub const DRAWER_ITEM_HEIGHT: f32 = 56.0;

/// 导航容器的两种宽度（D33：**同一个容器**的折叠 / 展开，不是两套组件）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NavLayout {
    /// 折叠：宽 **88**（`LayoutMetrics::nav_rail_width`），项内「药丸在上、标签在下」。
    Rail,
    /// 展开：宽 **240**（`LayoutMetrics::nav_drawer_width`），项内「图标在左、标签在右」。
    Drawer,
}

impl NavLayout {
    /// 两种宽度（gallery）与测试用。
    pub const ALL: [NavLayout; 2] = [NavLayout::Rail, NavLayout::Drawer];

    /// 稳定名字（截图文件名 / 图例用）。
    pub const fn name(self) -> &'static str {
        match self {
            NavLayout::Rail => "rail",
            NavLayout::Drawer => "drawer",
        }
    }
}

/// ListItem 的几何（`research/12b` §4）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ListItemGeometry {
    /// 一 / 两 / 三行行高（44 / 60 / 76；A1 是 56 / 72 / 88，本项目登记偏离 −12）。
    pub heights: [f32; 3],
    /// 内容起始侧内边距（16，A1 `ListItemLeadingSpace`）。
    pub pad_start: f32,
    /// 内容结束侧内边距（16，A1 `ListItemTrailingSpace`）。
    pub pad_end: f32,
    /// leading / trailing 图标边长（24，A1 `ListItemLeading/TrailingIconSize`）。
    pub icon: f32,
    /// 图标 ↔ 文本间距（16，`[估]`）。
    pub gap: f32,
}

impl ListItemGeometry {
    /// 从令牌求值。
    pub fn of(ui: &Ui) -> Self {
        let one = ui.metrics().row_height;
        let step = ui.space(Space::X4);
        Self {
            // 每多一行 +16：与官方 56→72→88 的 delta 一致，差的只是基准值（−12）。
            heights: [one, one + step, one + step * 2.0],
            pad_start: ui.space(Space::X4),
            pad_end: ui.space(Space::X4),
            icon: IconSize::Toolbar.px(),
            gap: ui.space(Space::X4),
        }
    }

    /// `lines` 行的行高（1/2/3 之外按就近夹取，不 panic——调用方传入的是枚举值）。
    pub fn height(&self, lines: usize) -> f32 {
        self.heights[lines.clamp(1, 3) - 1]
    }

    /// `lines` 行的文本块高（行高 − 文本块 = 上下各一半的内边距）。
    pub fn text_block(&self, ui: &Ui, lines: usize) -> f32 {
        let headline = ui.style(typography_headline()).line_height_px;
        match lines.clamp(1, 3) {
            // 一行：主文本（BodyLarge 24）
            1 => headline,
            // 两行：主 + 辅助（BodyLarge 24 + BodyMedium 20）
            2 => headline + ui.style(typography_supporting()).line_height_px,
            // 三行：overline（LabelSmall 16）+ 主 + 辅助
            _ => {
                ui.style(typography_overline()).line_height_px
                    + headline
                    + ui.style(typography_supporting()).line_height_px
            }
        }
    }

    /// 垂直内边距 = (行高 − 文本块) / 2（一行 44 → 10，两行 60 → 8，三行 76 → 8）。
    pub fn pad_y(&self, ui: &Ui, lines: usize) -> f32 {
        (self.height(lines) - self.text_block(ui, lines)) / 2.0
    }
}

/// ListItem 主文本字级（12b 4.5：`BodyLarge 16/24` `[源] ListTokens.ListItemLabelTextFont`）。
pub const fn typography_headline() -> unlucky_design::typography::TypeRole {
    unlucky_design::typography::TypeRole::BodyLarge
}

/// ListItem 辅助文本字级（12b 4.5：`BodyMedium 14/20`）。
pub const fn typography_supporting() -> unlucky_design::typography::TypeRole {
    unlucky_design::typography::TypeRole::BodyMedium
}

/// ListItem overline 字级（12b 4.5：`LabelSmall 11/16`）。
pub const fn typography_overline() -> unlucky_design::typography::TypeRole {
    unlucky_design::typography::TypeRole::LabelSmall
}

/// Tabs 的几何（`research/12b` §6）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabsGeometry {
    /// 容器高（48，**G22**）。
    pub height: f32,
    /// 图标边长（24，A1 `Primary/SecondaryNavigationTabTokens.IconSize`）。
    pub icon: f32,
    /// 标签左右内边距（16，`[估]`：Compose `TabRow` 的 `HorizontalTextPadding`）。
    ///
    /// 这条同时决定「指示条跟随选中项宽度」的**可测形态**：指示条宽 = 标签墨迹宽 + 2×16，
    /// 所以两段文案长度不同的 tab，其指示条宽度必然不同（渲染层断言用它）。
    pub pad: f32,
    /// Primary 指示条高（3，**G23**）。
    pub primary_bar: f32,
    /// Secondary 分隔线厚度（1 = `LM.divider_thickness`）。
    pub secondary_bar: f32,
}

impl TabsGeometry {
    /// 从令牌求值。
    pub fn of(ui: &Ui) -> Self {
        Self {
            height: TABS_HEIGHT,
            icon: IconSize::Toolbar.px(),
            pad: ui.space(Space::X4),
            primary_bar: TABS_INDICATOR,
            secondary_bar: ui.metrics().divider_thickness,
        }
    }

    /// 指示条 / 分隔线高度：Primary 取 3px，Secondary 取 1px。
    pub fn bar(&self, primary: bool) -> f32 {
        if primary {
            self.primary_bar
        } else {
            self.secondary_bar
        }
    }

    /// Primary 指示条圆角：`RoundedCornerShape(3dp)` 落在 3px 高的条上，与 `Full`（=1.5 半径）渲染结果相同。
    ///
    /// Secondary 的 1px 分隔线**没有圆角**：A1 是一条直线；早先给 0.5 圆角，相邻 tab 的分段之间
    /// 因此留下 1px 亮缝（视觉 r1 P2-2 实测 `25..101 | 103..191 | 193..265`）。
    pub fn indicator_radius(&self) -> Radius {
        Radius::new(self.primary_bar / 2.0)
    }
}

/// 导航容器（rail ⇄ drawer）的几何（`research/15` §21 + D33）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavPaneGeometry {
    /// 哪种宽度（88 / 240）。
    pub layout: NavLayout,
    /// 容器宽（88 / 240）。
    pub width: f32,
    /// 项高（rail 52 = 药丸 32 + 4 + 标签 16；drawer 56，**G24**）。
    pub item_height: f32,
    /// 选中药丸宽（rail 56 = 88 − 2×16；drawer 216 = 240 − 2×12）。
    pub pill_width: f32,
    /// 选中药丸高（rail 32；drawer 56 = 项高）。
    pub pill_height: f32,
    /// 药丸离容器左右各缩多少（rail 16；drawer 12 = `ItemPadding`）。
    pub pill_inset: f32,
    /// 内容起始侧内边距（drawer 16；rail 由药丸居中决定）。
    pub pad_start: f32,
    /// 内容结束侧内边距（drawer 24）。
    pub pad_end: f32,
    /// 图标边长（24）。
    pub icon: f32,
    /// 图标 ↔ 标签间距（rail 是纵向 4；drawer 是横向 12）。
    pub gap: f32,
    /// 容器圆角（`CornerLargeEnd`：贴边侧直角、靠内容侧 16）。
    pub radius: Radius,
}

impl NavPaneGeometry {
    /// 从令牌求值：**88 与 240 只在这里分叉**，其余全部由宽度派生。
    pub fn of(ui: &Ui, layout: NavLayout) -> Self {
        let metrics = ui.metrics();
        let item = drawer_metrics();
        // CornerLargeEnd：只有靠内容那一侧（右侧）两角 16，贴边侧直角。
        let corner = shape_radius();
        let radius = Radius {
            top_left: 0.0,
            top_right: corner,
            bottom_right: corner,
            bottom_left: 0.0,
        };

        match layout {
            NavLayout::Rail => {
                let width = metrics.nav_rail_width;
                let pill_inset = ui.space(Space::X4);
                let pill_height = metrics.control_height - ui.space(Space::X2);
                Self {
                    layout,
                    width,
                    // 52 = 药丸 32 + 4 + 标签行高 16（与 `shell/nav.rs` 的项高同源）
                    item_height: pill_height
                        + ui.space(Space::X1)
                        + ui.style(unlucky_design::typography::TypeRole::LabelMedium)
                            .line_height_px,
                    pill_width: width - pill_inset * 2.0,
                    pill_height,
                    pill_inset,
                    pad_start: pill_inset,
                    pad_end: pill_inset,
                    icon: IconSize::Toolbar.px(),
                    gap: ui.space(Space::X1),
                    radius,
                }
            }
            NavLayout::Drawer => {
                let width = metrics.nav_drawer_width;
                Self {
                    layout,
                    width,
                    item_height: DRAWER_ITEM_HEIGHT,
                    pill_width: item.indicator_width(width),
                    pill_height: DRAWER_ITEM_HEIGHT,
                    pill_inset: item.inset,
                    pad_start: item.pad_start,
                    pad_end: item.pad_end,
                    icon: item.icon,
                    gap: item.gap,
                    radius,
                }
            }
        }
    }
}

/// 导航容器的组件级微度量（`ComponentMetrics::NAVIGATION_DRAWER`；缺令牌会直接炸，不静默回退）。
fn drawer_metrics() -> ComponentMetrics {
    Component::NavigationDrawer
        .metrics()
        .expect("NavigationDrawer 的 ComponentMetrics 是 G10 的落点")
}

/// 容器圆角（`Component::NavigationDrawer.spec().shape = ShapeScale::Large` → 16）。
fn shape_radius() -> f32 {
    let shape = Component::NavigationDrawer.spec().shape;
    debug_assert_eq!(shape, ShapeScale::Large, "ContainerShape=CornerLargeEnd");
    shape.dp()
}

/// AppBar（small）的几何（`research/12b` §16）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AppBarGeometry {
    /// 页头高（56 = `LM.page_header_height`；A1 是 64，登记偏离 −8）。
    pub height: f32,
    /// 图标边长（24，A1 `AppBarTokens.IconSize`）。
    pub icon: f32,
    /// 左右内边距（4，A1 `AppBarTokens.Leading/TrailingSpace`）。
    pub edge: f32,
    /// 头像边长（32，A1 `AppBarTokens.AvatarSize`）。
    pub avatar: f32,
}

impl AppBarGeometry {
    /// 从令牌求值。
    pub fn of(ui: &Ui) -> Self {
        Self {
            height: ui.metrics().page_header_height,
            icon: IconSize::Toolbar.px(),
            edge: ui.space(Space::X1),
            avatar: IconSize::Empty.px(),
        }
    }
}

#[cfg(test)]
mod tests;
