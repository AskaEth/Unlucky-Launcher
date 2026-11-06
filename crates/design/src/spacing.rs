//! 形状 / 高程 / 动效之外的「尺子」：4px 栅格、骨架尺寸、一像素线。
//!
//! 源流有两条：`DevDocs/ops/slint-reference/crates/app/ui/theme/metrics.slint`（M52 定稿的骨架节奏），
//! 以及 `ui-integration-contract.md` §3「间距只允许 4/8/12/16/24/32」。M3 同样用 4dp 栅格，
//! 所以这层**不用改口径**，只是把值搬进唯一真源。
//!
//! ## 职责边界：`LayoutMetrics` vs `ComponentMetrics`（M69 / G10）
//!
//! | 结构 | 管什么 | 例子 |
//! |---|---|---|
//! | [`LayoutMetrics`] | **窗口骨架与布局节奏**：标题栏/页头/行的节奏、dock 宽度（导航、AI 面板）、命中区 | [`LayoutMetrics::nav_drawer_width`] / [`LayoutMetrics::search_bar_height`] / `touch_target` |
//! | `component_map::ComponentMetrics` | **单个控件内部几何**：外框尺寸、微圆角、描边/环线宽、状态层直径、内点、图标偏移 | checkbox 20×20 + 描边 2 + 状态层 Ø40 |
//! | [`stroke::HAIRLINE`] | **一像素线**的唯一真源：分隔线厚度、容器描边、聚焦指示 | [`LayoutMetrics::divider_thickness`] |
//!
//! 判据：一个值若随窗口/列宽变化、或决定一块面板占多宽，归 LM；若它只在某个控件自己的方框
//! 内起作用（画进去的那几个像素），归 CM。**同一个值不许两边各写一份**。

/// 一像素线（G12）：全站 1px 线的**唯一真源**。
///
/// 为什么要单独开一个命名空间：`1.0` 在项目里同时承担三种语义——分隔线厚度、容器描边宽度、
/// 聚焦指示描边宽度。各自抄字面量就会漂成 1.0 / 1.5 / 2.0 混排（M66 的 G12 正是这么来的）。
/// 从这里取，语义和数值只有一个落点。
///
/// iced 侧怎么落地（适配层照此抄，别再自己定 `STROKE_PX`）：
/// - **容器描边**：`Border { width: unlucky_design::stroke::HAIRLINE, .. }`（卡片/输入框/chip/菜单）；
/// - **分隔线**：[`LayoutMetrics::divider_thickness`]（数值就是 [`stroke::HAIRLINE`]）——
///   横向分隔线是容器高度、纵向是宽度，都取它；
/// - **聚焦环**：内描边与外描边**都用**它（M3 的聚焦指示是 1dp，做成 2dp 就是加粗）；
/// - **不用于**：elevation 的 blur / offset（那是 [`crate::elevation`] 的量）、
///   图标描边（图标走 SVG 的 viewBox 单位，见 [`crate::icons::stroke_width_for`]）。
pub mod stroke {
    /// 一像素线宽度（逻辑 px @100% DPI = 1dp；DividerTokens.Thickness）。
    pub const HAIRLINE: f32 = 1.0;
}

/// 栅格基数（dp）。
pub const GRID: f32 = 4.0;

/// 间距档位（M3/M2 通用 4px 栅格）。禁止 6/10/14 这类非整倍数值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Space {
    /// 4dp：图标与文字之间。
    X1,
    /// 8dp：控件内边距、行内元素间距。
    X2,
    /// 12dp：控件间距。
    X3,
    /// 16dp：区块内行距、卡片内边距下限。
    X4,
    /// 24dp：区块间距、页面内边距。
    X6,
    /// 32dp：页级大留白。
    X8,
}

impl Space {
    /// 间距值（dp）。
    pub const fn dp(self) -> f32 {
        match self {
            Space::X1 => 4.0,
            Space::X2 => 8.0,
            Space::X3 => 12.0,
            Space::X4 => 16.0,
            Space::X6 => 24.0,
            Space::X8 => 32.0,
        }
    }

    /// 全部档位，由小到大。
    pub const ALL: [Space; 6] = [
        Space::X1,
        Space::X2,
        Space::X3,
        Space::X4,
        Space::X6,
        Space::X8,
    ];
}

/// 把任意数值对齐到 4px 栅格（最近档）。
pub fn snap_to_grid(value: f32) -> f32 {
    (value / GRID).round() * GRID
}

/// 骨架尺寸：窗口结构（标题栏、导航、页头、行高）的单一来源。
///
/// 这些值不是 M3 规范值（M3 只规定组件内部节奏），而是本启动器的**布局契约**，
/// 由页面/组件两条线共同消费——任何页面自己写 `56px` 都是重复定义。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutMetrics {
    /// 标题栏高度。
    pub titlebar_height: f32,
    /// 标题栏按钮宽度（关闭/最大化/最小化）。
    pub titlebar_button_width: f32,
    /// 展开态导航（drawer 形态）宽度。
    ///
    /// **G11 语义修正（M69 / D33）**：D16「左侧固定、允许折叠」落地为**同一个常驻容器**的两种
    /// 宽度——展开 240（drawer）、折叠 88（rail）。旧字段 `nav_expanded_width = 88` 的名字与值
    /// 相反（88 其实是 rail），已删除，**不留别名**：两个名字指同一个量等于把选择权还给组件层。
    ///
    /// 官方 `NavigationDrawerTokens.ContainerWidth = 360`、`MinimumDrawerWidth = 240`；
    /// 本项目取官方下限 240——360 + AI 面板 320 = 680 = 1180 窗宽的 58%，内容区只剩 500 放不下两列卡片。
    pub nav_drawer_width: f32,
    /// 折叠态导航（rail 形态）宽度。
    ///
    /// 88 = 12b §5.1 对官方 rail 96 的登记偏离（−8）：指示器宽 88 − 2×16 = 56 仍成立，
    /// 再窄就装不下 56 宽的选中指示器。
    pub nav_rail_width: f32,
    /// 搜索条高度（G11）。
    ///
    /// 官方 `SearchBarTokens.ContainerHeight = 56`。**不走 [`LayoutMetrics::control_height`]（40）**：
    /// 搜索条是页级入口而不是密集行控件；56 也顺带 ≥ 命中下限 48。圆角由 `ShapeScale::Full`
    /// 按边长折算（56 → 28，正好等于官方 `SearchBarCornerRadius`）。
    pub search_bar_height: f32,
    /// 分隔线厚度（G11 + G12）。
    ///
    /// 官方 `DividerTokens.Thickness = 1.0.dp`。值就是 [`stroke::HAIRLINE`]——**一像素线不受 4px
    /// 栅格约束**，所以它不在栅格测试的表里（测试里显式豁免并写明原因）。
    pub divider_thickness: f32,
    /// 导航项高度。
    pub nav_item_height: f32,
    /// 导航项间距。
    pub nav_item_gap: f32,
    /// 单行页头高度。
    pub page_header_height: f32,
    /// 两行页头高度（日志页）。
    pub page_header_tall_height: f32,
    /// 运行中状态条高度。
    pub running_strip_height: f32,
    /// 列表行高（桌面密集列表；见模块文档的「触控目标」说明）。
    pub row_height: f32,
    /// 标准控件可见高度（按钮/输入框）。
    pub control_height: f32,
    /// 最小**命中**区域（M3 Expressive 的无障碍下限）。
    pub touch_target: f32,
    /// 卡片内边距。
    pub card_padding: f32,
    /// AI 面板宽度。
    pub ai_panel_width: f32,
}

impl LayoutMetrics {
    /// 桌面基线（1180×760 窗口）。
    ///
    /// **触控目标的换算依据**（reviewer P3-1：不许和 `metrics.slint` 沉默地不一致）：
    /// - 规范：M3 Expressive 要求可点目标 ≥**48dp**（无障碍下限，手机与桌面同标准）。
    /// - 我们取 `touch_target = 48`（照规范），`control_height = 40`（**可见**高度）：
    ///   两者差 8，正好由控件上下各 4px 的内边距补足，于是「看起来 40 高的按钮」有 48 的命中区。
    /// - `row_height = 44` / `nav_item_height = 44` 是 M63 §R13 + §1.4 表定的桌面密集档
    ///   （`metrics.slint` 的导航 56 是 Fluent 2 时代的旧节奏，D25 已终止）：可见高度 44，
    ///   点击命中也按 `touch_target`（48）判定，不靠行高本身。
    /// - `titlebar_height = 48` 取自 M63 §1.4 的 S2 面板表（旧值 40 是 Fluent 口径）。
    pub const DESKTOP: Self = Self {
        titlebar_height: 48.0,
        titlebar_button_width: 46.0,
        // G11 / D33：展开 = drawer 240，折叠 = rail 88（旧 `nav_expanded_width=88` /
        // `nav_collapsed_width=48` 的名字与值相反，已删）。
        nav_drawer_width: 240.0,
        nav_rail_width: 88.0,
        // G11：官方 SearchBarTokens.ContainerHeight = 56。
        search_bar_height: 56.0,
        // G11/G12：官方 DividerTokens.Thickness = 1.0dp → 一像素线的唯一真源。
        divider_thickness: stroke::HAIRLINE,
        nav_item_height: 44.0,
        nav_item_gap: 4.0,
        page_header_height: 56.0,
        page_header_tall_height: 84.0,
        running_strip_height: 32.0,
        row_height: 44.0,
        control_height: 40.0,
        touch_target: 48.0,
        card_padding: 20.0,
        ai_panel_width: 320.0,
    };
}

impl LayoutMetrics {
    /// 页头是否两行（日志页），决定主区与 AI 面板分隔线对齐的高度。
    pub fn header_height(&self, two_line: bool) -> f32 {
        if two_line {
            self.page_header_tall_height
        } else {
            self.page_header_height
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_space_is_on_the_grid() {
        for space in Space::ALL {
            assert_eq!(space.dp() % GRID, 0.0, "{space:?} 不在 4px 栅格上");
        }
    }

    #[test]
    fn layout_values_are_on_the_grid() {
        let m = LayoutMetrics::DESKTOP;
        // 受 4px 栅格约束的字段（间距/高度/宽度类）。
        // `titlebar_button_width` 刻意不在表内：46 对齐 Windows 11 原生 caption 按钮宽度，
        // 属 OS 惯例值，单列一条断言钉住，不让它静默漂移。
        // `divider_thickness` 同理不在表内：它是**一像素线**（1 不是 4 的倍数），
        // 归 `stroke::HAIRLINE` 那条规则管，下面单独断言。
        for (name, value) in [
            ("titlebar_height", m.titlebar_height),
            ("nav_drawer_width", m.nav_drawer_width),
            ("nav_rail_width", m.nav_rail_width),
            ("search_bar_height", m.search_bar_height),
            ("nav_item_height", m.nav_item_height),
            ("nav_item_gap", m.nav_item_gap),
            ("page_header_height", m.page_header_height),
            ("page_header_tall_height", m.page_header_tall_height),
            ("running_strip_height", m.running_strip_height),
            ("row_height", m.row_height),
            ("control_height", m.control_height),
            ("touch_target", m.touch_target),
            ("card_padding", m.card_padding),
            ("ai_panel_width", m.ai_panel_width),
        ] {
            assert_eq!(value % GRID, 0.0, "{name} = {value} 不在 4px 栅格上");
        }
        assert_eq!(
            m.titlebar_button_width, 46.0,
            "标题栏按钮宽度应对齐 Win11 原生 caption"
        );
        // 一像素线的显式豁免（不是「忘了加进表」，是它本来就不受栅格约束）。
        assert_eq!(m.divider_thickness, stroke::HAIRLINE);
        assert_ne!(m.divider_thickness % GRID, 0.0);
    }

    #[test]
    fn nav_widths_follow_the_two_form_decision() {
        // D33：左侧导航是同一容器的两种宽度，展开（drawer）必须比折叠（rail）宽。
        let m = LayoutMetrics::DESKTOP;
        assert_eq!(m.nav_drawer_width, 240.0, "展开 drawer 取官方下限 240");
        assert_eq!(m.nav_rail_width, 88.0, "折叠 rail 为 88（登记偏离官方 96）");
        assert!(m.nav_rail_width < m.nav_drawer_width);
        // rail 里的选中指示器 = 容器宽 − 2×16；88 − 32 = 56，再窄就装不下 56 宽指示器。
        assert_eq!(m.nav_rail_width - Space::X4.dp() * 2.0, 56.0);
        // 内容区下限：1180 窗宽 − drawer − AI 面板 仍 ≥600（够放两列卡片）。
        assert!(1180.0 - m.nav_drawer_width - m.ai_panel_width >= 600.0);
    }

    #[test]
    fn search_bar_height_is_a_page_level_entry() {
        let m = LayoutMetrics::DESKTOP;
        assert_eq!(m.search_bar_height, 56.0, "官方 ContainerHeight = 56");
        assert!(
            m.search_bar_height >= m.touch_target,
            "页级入口必须 ≥ 命中下限"
        );
        // 圆角走 ShapeScale::Full，56 → 28，正好是官方 SearchBarCornerRadius。
        assert_eq!(
            crate::shape::ShapeScale::Full.radius_px(m.search_bar_height),
            28.0
        );
    }

    #[test]
    fn touch_target_meets_the_expressive_48dp_minimum() {
        let m = LayoutMetrics::DESKTOP;
        // M3 Expressive 无障碍下限：可点目标 ≥48dp。
        assert_eq!(m.touch_target, 48.0);
        // 可见控件高 + 上下各 4px 内边距 = 命中区。
        assert_eq!(m.control_height + 2.0 * Space::X1.dp(), m.touch_target);
        // 密集行（M63 §1.4/R13 的桌面档）可见高度 44，命中仍按 touch_target。
        assert_eq!(m.row_height, 44.0);
        assert_eq!(m.nav_item_height, 44.0);
        assert!(m.row_height <= m.touch_target);
        // M63 §1.4 的 S2 面板表：标题栏 48。
        assert_eq!(m.titlebar_height, 48.0);
    }

    #[test]
    fn snap_rounds_to_nearest_grid_step() {
        assert_eq!(snap_to_grid(5.0), 4.0);
        assert_eq!(snap_to_grid(6.5), 8.0);
        assert_eq!(snap_to_grid(0.0), 0.0);
    }

    #[test]
    fn header_height_follows_line_count() {
        let m = LayoutMetrics::DESKTOP;
        assert_eq!(m.header_height(false), 56.0);
        assert_eq!(m.header_height(true), 84.0);
    }
}
