//! 组件级微度量（`ComponentMetrics`）：**单个控件内部的几何**，不是窗口骨架。
//!
//! 为什么要有这个结构（M69 / G10）：Checkbox 的 20×20、圆角 2、描边 2、状态层 Ø40 这类值，
//! `ShapeScale`（刻度）装不了、`LayoutMetrics`（骨架）也不该装——M66 对账时它们只能写在组件
//! 实现里，而 D30 禁止组件层自定义数值。这里给它们一个**唯一落点**。
//!
//! ## 与 `LayoutMetrics` 的分工（两把尺子，量不同的东西）
//!
//! | | [`crate::spacing::LayoutMetrics`] | `ComponentMetrics`（本模块） |
//! |---|---|---|
//! | 管什么 | 窗口骨架与布局节奏：标题栏/页头/行高、dock 宽度、命中区 | 单个控件方框内的几何：外框、微圆角、描边/环线宽、状态层、内点、图标偏移 |
//! | 随什么变 | 窗口尺寸、列宽、用户折叠导航 | 只随组件自身规格变 |
//! | 例子 | `nav_drawer_width = 240`、`search_bar_height = 56` | checkbox 20×20、描边 2、状态层 Ø40 |
//! | 判据 | 决定「一块面板/一行占多宽多高」 | 决定「画进这个方框里的那几个像素」 |
//!
//! 冲突时的规则是**同一个量只许有一份**：搜索条高 56 属骨架级，所以
//! [`ComponentMetrics::SEARCH_BAR`] 的 `height` 留 `0.0` 并在文档里指回 LM；drawer 宽同理。
//! 测试 `component_metrics_never_duplicate_layout_metrics` 把这条钉死。
//!
//! 数值来源：`androidx.compose.material3:material3-android:1.4.0` 源码（A 级，M66 逐行核对，
//! 见 `research/15` §18~§23）。逐字段的出处写在下面各组件的关联常量上；**团队口径**（官方没有
//! token 的）单独注明。

use super::Component;
use crate::icons::IconSize;
use crate::shape::ComponentRadius;
use crate::spacing::Space;

/// 单个控件的内部几何。
///
/// `0.0` 表示**该控件没有这一项**（不是「默认值 0」）——拿 `0.0` 反推数值一定错。
/// 骨架级尺寸（条高、面板宽）不在这里，去 [`crate::spacing::LayoutMetrics`] 拿。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComponentMetrics {
    /// 可见外框宽（dp；`0.0` = 由内容或骨架决定）。
    pub width: f32,
    /// 可见外框高（dp；`0.0` = 由内容或骨架决定）。
    pub height: f32,
    /// 微控件圆角（D32 第二层）。`None` = 该控件的圆角走 `ShapeScale`（见 `ComponentSpec::shape`）。
    pub radius: Option<ComponentRadius>,
    /// 描边 / 环线宽度（dp）。**禁用态也不许变细**——官方 disabled 只改透明度。
    pub stroke: f32,
    /// 状态层（hover / focus / pressed）圆的直径（dp；`0.0` = 不画状态层）。
    pub state_layer: f32,
    /// 内点直径（dp）：radio 选中时的实心点，其余控件 `0.0`。
    pub inner_dot: f32,
    /// 控件相对容器的外距（dp）：drawer 项 = 12，决定选中指示器左右各内缩 12。
    pub inset: f32,
    /// 内容起始侧内边距（dp）。**图标不算在内**：图标可视边 = `pad_start + icon_offset`。
    pub pad_start: f32,
    /// 内容结束侧内边距（dp）。同起始侧，图标可视边 = `pad_end + icon_offset`。
    pub pad_end: f32,
    /// 图标边长（dp；`0.0` = 无图标）。
    pub icon: f32,
    /// 图标相对内边距的**额外**偏移（dp）。图标可视边 = 内边距 + 本值（search bar：12 + 4 = 16），
    /// **不要**把内边距本身当图标边用，也不要再手抄那个 12 / 16。
    pub icon_offset: f32,
    /// 图标与文字（或相邻元素）的间距（dp）。
    pub gap: f32,
}

impl ComponentMetrics {
    /// Checkbox（`research/15` §18 / §23）。
    ///
    /// A 级出处：`Checkbox.kt:737 CheckboxSize = 20.dp`、`:738 RadiusSize = 2.dp`（圆角走 D32 的
    /// [`ComponentRadius::Micro`]）、`CheckboxTokens.UnselectedOutlineWidth = 2`、
    /// `StateLayerSize = 40` + `StateLayerShape = CornerFull`。
    /// 注意 `CheckboxTokens.ContainerWidth/Height = 18` 是**死值**（全仓 0 引用），Compose 实际画
    /// 20（§26.2 矛盾 ①）——判据取 20。
    /// 勾选控件 ↔ 标签间距 8 是**团队口径**：官方没有「框 + 标签」组合的 token（§18.9，估）。
    pub const CHECKBOX: Self = Self {
        width: 20.0,
        height: 20.0,
        radius: Some(ComponentRadius::Micro),
        stroke: 2.0,
        state_layer: 40.0,
        inner_dot: 0.0,
        inset: 0.0,
        pad_start: 0.0,
        pad_end: 0.0,
        icon: 0.0,
        icon_offset: 0.0,
        gap: Space::X2.dp(),
    };

    /// Radio（`research/15` §19 / §23）。
    ///
    /// A 级出处：`RadioButtonTokens.IconSize = 20`、`RadioStrokeWidth = 2.dp`；内点名义
    /// `RadioButtonDotSize = 12`，但绘制半径 = 12/2 − stroke/2 = 5 → **实际内点直径 10**
    /// （§19.2：抄 12 会明显偏大）。圆角走 `ShapeScale::Full`（真圆），不是微半径。
    /// 标签间距 8 同 Checkbox（团队口径）。
    pub const RADIO: Self = Self {
        width: 20.0,
        height: 20.0,
        radius: None,
        stroke: 2.0,
        state_layer: 40.0,
        inner_dot: 10.0,
        inset: 0.0,
        pad_start: 0.0,
        pad_end: 0.0,
        icon: 0.0,
        icon_offset: 0.0,
        gap: Space::X2.dp(),
    };

    /// Search bar（`research/15` §20A / §23）。
    ///
    /// 高度与宽度**不在这里**：条高 56 是骨架级（`LayoutMetrics::search_bar_height`），
    /// 最小宽 360 同样属骨架级（尚未进 LM，M69 报告里登记为待办）。这里只装画进 bar 里的几何：
    /// 图标 24（`IconSize::Toolbar`）、**图标可视边 = `pad_start + icon_offset`** = 12 + 4 = **16**
    /// （右侧同式为 16，§20.4）、圆角走 `ShapeScale::Full`（56 → 28 = 官方 `SearchBarCornerRadius`），
    /// 所以 `radius` 是 `None`。
    ///
    /// 为什么要拆成 12 + 4 而不是直接写 16：官方 = 文本框内容内边距 **12**（`TextFieldImpl` 默认）
    /// 加上 `SearchBarIconOffsetX = 4`。拆开才能让「文本从 12 起、图标从 16 起」都被消费方拿到；
    /// 只写 16 的话文本内边距没有落点，消费方只能自己写 `12.0`——正是 G10 要消灭的字面量。
    /// 图标位置一律按 `pad + icon_offset` 求和，不要单独把 `pad` 当图标边用。
    pub const SEARCH_BAR: Self = Self {
        width: 0.0,
        height: 0.0,
        radius: None,
        stroke: 0.0,
        state_layer: 0.0,
        inner_dot: 0.0,
        inset: 0.0,
        pad_start: Space::X3.dp(),
        pad_end: Space::X3.dp(),
        icon: IconSize::Toolbar.px(),
        icon_offset: 4.0,
        // 图标 ↔ 文本 8 是**团队口径 [估]**：`research/15` §20.4 只给了图标尺寸/位置与 48 命中框，
        // 没有「图标 ↔ 文字」的 token（Compose 由 `TextFieldImpl` 内部排版决定）。
        gap: Space::X2.dp(),
    };

    /// Navigation drawer（`research/15` §21 / §23）。
    ///
    /// 宽度与项高**不在这里**：drawer 240 / rail 88 在 `LayoutMetrics`，项高 44 =
    /// `LayoutMetrics::nav_item_height`。这里装项内几何（§21.6，A 级：
    /// `NavigationDrawerItemDefaults.ItemPadding = 12`、项内 start 16 / end 24、
    /// `IconSize = 24`、图标 ↔ label 12）。
    /// 容器形状 `CornerLargeEnd`（只有靠内容那一侧两角 16）由 `ComponentSpec::shape` 的
    /// `Large` + 组件实现按侧位决定哪两个角，度量层不表达「部分角」。
    pub const NAVIGATION_DRAWER: Self = Self {
        width: 0.0,
        height: 0.0,
        radius: None,
        stroke: 0.0,
        state_layer: 0.0,
        inner_dot: 0.0,
        inset: 12.0,
        pad_start: Space::X4.dp(),
        pad_end: Space::X6.dp(),
        icon: IconSize::Toolbar.px(),
        icon_offset: 0.0,
        gap: Space::X3.dp(),
    };

    /// drawer 选中指示器宽度：容器宽 − 2 × [`ComponentMetrics::inset`]。
    ///
    /// 240 − 24 = **216**；官方 360 − 24 = 336 是同一个公式（§21.5）——写死 336 就是忘了推导。
    pub fn indicator_width(&self, container_width: f32) -> f32 {
        (container_width - self.inset * 2.0).max(0.0)
    }
}

/// 组件 → 组件级微度量。`None` = 该组件没有控件内部几何（只有骨架值，去 LM 拿）。
///
/// Divider 是 `None` 的显式例子：它的唯一几何量是厚度，而厚度是**一像素线**
/// （`LayoutMetrics::divider_thickness` = `stroke::HAIRLINE`），不是「控件内部」的东西。
pub(super) const fn metrics_of(component: Component) -> Option<ComponentMetrics> {
    match component {
        Component::Checkbox => Some(ComponentMetrics::CHECKBOX),
        Component::Radio => Some(ComponentMetrics::RADIO),
        Component::Search => Some(ComponentMetrics::SEARCH_BAR),
        Component::NavigationDrawer => Some(ComponentMetrics::NAVIGATION_DRAWER),
        _ => None,
    }
}
