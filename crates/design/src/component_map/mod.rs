//! 组件 → 档位映射表：哪个组件用哪档形状 / 高程 / 字体 / 动效。
//!
//! 目的不是「规定 M3 官方长什么样」（官方只给组件规格，不给启动器里的档位表），而是
//! **给 iced 页面线一个不需要再谈判的默认值**：同一个组件在五个页面里长得一样，
//! 才有用户要的「UI 整体性」。
//!
//! 口径：
//! - 形状档位见 [`crate::shape::ShapeScale`]，高程见 [`crate::elevation::Elevation`]；
//! - `text` 是主要文字角色，`support` 是辅助文字（没有就 `None`）；
//! - `motion` + `duration` 是状态过渡（hover/press/展开），不是页面切换；
//! - `morph` 是按下/选中时的形状形变（Expressive 特征），不需要就 `None`；
//! - 圆角分两层（D32）：容器级查 [`crate::shape::ShapeScale`]，≤24dp 的微控件查
//!   [`crate::shape::ComponentRadius`]（落在 [`ComponentSpec::micro_radius`]）。
//!
//! 档位（形状/高程/字级/动效）之外，**控件内部的微几何**（20×20、描边 2、状态层 Ø40 …）在
//! [`ComponentMetrics`] 里，入口是 [`Component::metrics`]——两者分工见该项的文档
//! （`component_map::metrics` 模块）。
//!
//! **改这里的值 = 改全站观感**，必须同步 `DevDocs/SubAgentsTalk/design-令牌约定.md`；
//! 页面线不许在页面里覆盖这些档位。

mod metrics;
mod table;

pub use metrics::ComponentMetrics;

use crate::elevation::Elevation;
use crate::motion::{DurationToken, Easing};
use crate::shape::{ComponentRadius, ShapeMorph, ShapeScale};
use crate::typography::TypeRole;

/// 组件的排版 + 形状 + 动效档位。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComponentSpec {
    /// 该档位属于哪个组件（表自描述，便于测试与文档生成）。
    pub component: Component,
    /// 圆角档位（容器级，[`ShapeScale`] 的严格刻度）。
    pub shape: ShapeScale,
    /// 微控件圆角（D32 第二层）。
    ///
    /// `Some` 时**本字段才是半径真源**，此时 [`ComponentSpec::shape`] 必须是
    /// [`ShapeScale::None`]（占位、不参与渲染）——因为 2dp 不在 `ShapeScale` 刻度里，
    /// 硬塞进 `shape` 会破坏「圆角只取刻度值」。这层配对由测试钉死。
    pub micro_radius: Option<ComponentRadius>,
    /// 高程级别（决定 surface container 角色与是否画阴影）。
    pub elevation: Elevation,
    /// 主要文字角色。**无文字的组件（如 `Divider`）这里是占位值**，消费方不得据此排版。
    pub text: TypeRole,
    /// 辅助文字角色。
    pub support: Option<TypeRole>,
    /// 状态过渡缓动。
    ///
    /// 该字段不是 `Option`，所以**没有动效语义的组件（如 `Divider`）这里是占位值**——
    /// 读之前先确认组件本身有状态过渡，无过渡的组件不得据此排动效。
    pub motion: Easing,
    /// 状态过渡时长。
    pub duration: DurationToken,
    /// 按下/选中时的形状形变。
    pub morph: Option<ShapeMorph>,
}

/// 需要固定档位的组件清单（M53 页面骨架 + M69 组件线补的 5 个）。
///
/// **冻结为只可末尾追加**：判别值当索引用（`table::SPECS[self as usize]`），改名/换位等于
/// 让整张表错行。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Component {
    /// 顶部标题栏（第一层 chrome）。
    TopAppBar,
    /// 导航项（侧栏一行）。
    NavigationItem,
    /// 主操作按钮（实心强调色，每屏最多一个）。
    PrimaryButton,
    /// 次操作按钮（tonal）。
    SecondaryButton,
    /// 文本按钮（低权重）。
    TextButton,
    /// 图标按钮。
    IconButton,
    /// 描边卡片。
    OutlinedCard,
    /// 填充卡片（层级高于描边卡）。
    FilledCard,
    /// 列表行。
    ListItem,
    /// 文本输入框。
    TextField,
    /// 分段按钮（磁贴/列表切换、小中大三档）。
    SegmentedButton,
    /// 贴片/徽标。
    Chip,
    /// 开关。
    Switch,
    /// 滑块。
    Slider,
    /// 进度条。
    ProgressIndicator,
    /// 对话框。
    Dialog,
    /// 下拉菜单。
    Menu,
    /// 底部提示条。
    Snackbar,
    /// 悬浮提示。
    Tooltip,
    /// 空态（图标 + 标题 + 说明 + 标准按钮）。
    EmptyState,
    /// 复选框（安装选项 / mod 启用 / 日志过滤 / Java 管理勾选）。
    ///
    /// 圆角 **2dp** 不在 `ShapeScale` 刻度里 → 走 [`ComponentSpec::micro_radius`]（D32 第二层）；
    /// 20×20 / 描边 2 / 状态层 Ø40 见 [`ComponentMetrics::CHECKBOX`]。
    Checkbox,
    /// 单选框（Java 版本单选 / 安装模式选择）。
    ///
    /// 真圆（`ShapeScale::Full`）；20 / 环线 2 / 内点 10 / 状态层 Ø40 见 [`ComponentMetrics::RADIO`]。
    Radio,
    /// 搜索条（实例 / 版本 / mod 搜索，页级入口）。
    ///
    /// 高 56 是骨架级（`LayoutMetrics::search_bar_height`），圆角 `Full`（56 → 28）；
    /// 图标 24 + 偏移 4 见 [`ComponentMetrics::SEARCH_BAR`]。高程取 **L0**（源码矛盾已登记）。
    Search,
    /// 导航抽屉（左侧常驻导航的展开形态，D33）。
    ///
    /// 形状 `CornerLargeEnd`（只有靠内容那侧两角 16），宽 240 = `LayoutMetrics::nav_drawer_width`；
    /// 项内几何见 [`ComponentMetrics::NAVIGATION_DRAWER`]。
    NavigationDrawer,
    /// 分隔线（列表行分隔 / 区块分隔）。
    ///
    /// **无高程、无圆角**；厚度是一像素线（`LayoutMetrics::divider_thickness` = `stroke::HAIRLINE`），
    /// 颜色 `OutlineVariant`（列表内）或 `Outline`（search view 头部分隔，见 §20.8）。
    Divider,
}

impl Component {
    /// 全部组件。
    pub const ALL: [Component; 25] = [
        Component::TopAppBar,
        Component::NavigationItem,
        Component::PrimaryButton,
        Component::SecondaryButton,
        Component::TextButton,
        Component::IconButton,
        Component::OutlinedCard,
        Component::FilledCard,
        Component::ListItem,
        Component::TextField,
        Component::SegmentedButton,
        Component::Chip,
        Component::Switch,
        Component::Slider,
        Component::ProgressIndicator,
        Component::Dialog,
        Component::Menu,
        Component::Snackbar,
        Component::Tooltip,
        Component::EmptyState,
        // M69 G9：末尾追加的 5 个（前 20 个次序不许动）。
        Component::Checkbox,
        Component::Radio,
        Component::Search,
        Component::NavigationDrawer,
        Component::Divider,
    ];

    /// 该组件的档位。
    pub fn spec(self) -> ComponentSpec {
        table::SPECS[self as usize]
    }

    /// 该组件的**控件内部**微度量；`None` = 只有骨架值（去 `LayoutMetrics` 拿）。
    ///
    /// 这一条是 G10 的消费入口：组件线拿到 `Component::Checkbox.metrics()` 就有 20×20 / 圆角 2 /
    /// 描边 2 / 状态层 Ø40，**不许再在实现里写这些字面量**。
    pub fn metrics(self) -> Option<ComponentMetrics> {
        metrics::metrics_of(self)
    }

    /// 稳定字符串名（kebab-case）。
    pub const fn name(self) -> &'static str {
        match self {
            Component::TopAppBar => "top-app-bar",
            Component::NavigationItem => "navigation-item",
            Component::PrimaryButton => "primary-button",
            Component::SecondaryButton => "secondary-button",
            Component::TextButton => "text-button",
            Component::IconButton => "icon-button",
            Component::OutlinedCard => "outlined-card",
            Component::FilledCard => "filled-card",
            Component::ListItem => "list-item",
            Component::TextField => "text-field",
            Component::SegmentedButton => "segmented-button",
            Component::Chip => "chip",
            Component::Switch => "switch",
            Component::Slider => "slider",
            Component::ProgressIndicator => "progress-indicator",
            Component::Dialog => "dialog",
            Component::Menu => "menu",
            Component::Snackbar => "snackbar",
            Component::Tooltip => "tooltip",
            Component::EmptyState => "empty-state",
            // M69 G9：末尾追加（`name()` 是持久化字符串 id，同样只许末尾追加）。
            Component::Checkbox => "checkbox",
            Component::Radio => "radio",
            Component::Search => "search",
            Component::NavigationDrawer => "navigation-drawer",
            Component::Divider => "divider",
        }
    }
}

#[cfg(test)]
mod tests;
