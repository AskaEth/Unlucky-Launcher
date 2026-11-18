//! **导航类家族**（`research/12b` §4/§6/§16 + `research/15` §21）：四件组件，一个容器。
//!
//! | 组件 | 入口 | 参考章节 | 关键几何 |
//! |---|---|---|---|
//! | ListItem | [`list_item::list_item`] | 12b §4 | 行高 44 / 60 / 76（A1 56/72/88，登记偏离） |
//! | Tabs | [`tabs::tabs`] | 12b §6 | 容器 48、Primary 指示条 3、Secondary 线 1 |
//! | NavigationDrawer | [`drawer::navigation`] | 15 §21 + D33 | **同一个容器**：rail 88 ⇄ drawer 240 |
//! | AppBar small | [`app_bar::app_bar`] | 12b §16 | 页头 56、标题 TitleLarge、滚动换色 + L2 |
//!
//! ## 三条纪律（与另外四个家族同源）
//!
//! 1. **对消息类型泛型**，不 import `crate::app::Message`（契约 §2）；`Ui` 一律 `&'a Ui` 传入。
//! 2. **状态层只走** [`crate::app::components::interaction`]：α 与禁用色一处求值；
//!    禁用是**修饰器**（`.disabled()` 只清消息，见 `selection/chip.rs` 复审 P2-1）。
//! 3. **精确尺寸只走** [`crate::app::components::layout`]：图标一律 `layout::fixed`（Q9），
//!    居中交给父级 `align_x`（Q8：`center_x(Fill)` 会把 56 宽药丸撑成 88）。
//!
//! ## 禁用内容角色**按变体**（不许抄默认值）
//!
//! | 组件 | 禁用内容角色 | 出处 |
//! |---|---|---|
//! | ListItem | **`OnSurface`**@38%（不是 `OnSurfaceVariant`） | `[源]` `ListTokens.ListItemDisabledLabel/LeadingIcon/TrailingIconColor = OnSurface` + `…Opacity = 0.38f` |
//! | Drawer / Rail 项 | `OnSurface`@38% | `[估]`（A1 无 disabled token，与 ListItem 同表面语境） |
//! | Tabs | `OnSurface`@38% | `[估]`（同上；A1 的 Tabs 两套 token 只有 Active/Inactive） |
//! | AppBar | — （本轮页头不带禁用动作） | — |
//!
//! ## Q21：要给居中就**两轴都显式给**（第三轮 review 的规则化）
//!
//! `Container` 的 `horizontal_alignment` 默认 `Left`、`vertical_alignment` 默认 `Top`
//! （`iced_widget-0.14.2/src/container.rs`）。只给 `align_y` 会让内容贴左缘——AppBar 的 40×40
//! 动作按钮里 24px 图标曾因此左内缩 4 / 右内缩 20（视觉 r1 实测前导墨迹 `38..58`，应 `44..68`），
//! 而且 hover 的圆形状态层会露在图标一侧。
//!
//! 本目录的复查结论（逐处）：
//!
//! | 位置 | 需要 | 写法 |
//! |---|---|---|
//! | `app_bar::action` 命中区里的图标 | 两轴居中 | `align_x` + `align_y` |
//! | `app_bar` 页头内容行 | 垂直居中（水平由 `Fill` 决定） | `.align_y(Center)` |
//! | `tabs::tab_cell` 的内容 | 两轴居中 | `align_x` + `align_y` |
//! | `drawer/item` 的药丸 | 两轴居中 | `align_x` + `align_y` |
//! | `list_item` 的文本块 / `drawer` 的分组标题 | **左上起排**（`Start` 是对的，不需要居中） | 不设对齐，靠 `padding` 定起点 |
//!
//! 回归钉子：`navigation::probe` 的 appbar 断言量「左内缩 == 右内缩 ±2」——`align_x` 再漏一次就红。
//!
//! ## 单行约束（偏离 D-M79-5）
//!
//! 四件的文本一律 `Wrapping::None` + 外层 `clip(true)`：容器高度都是固定值（44/60/76、48/64、56），
//! `Text` 默认 `Wrapping::Word` 会把长实例名折成 2~3 行，顶穿页头、压住 trailing。官方要的是
//! **单行 ellipsis**，iced 没有 ellipsis（`inputs/menu.rs` 的 D19 先例）→ 我们单行 + 超长直接切。
//! 取证：`ULL_NAV_PART=long` 出图 + `navigation::probe::long` 量「行高不变 / 页头仍 56 / 标题不压动作」。
//!
//! ## 令牌缺口（G 编号，续 `research/15` §25 的 G13；G16~G20 已被其他家族占用）
//!
//! - **G22** Tabs 容器高 48（带图标 64）——`LayoutMetrics` 无此档；
//! - **G23** Tabs 指示条高 3 / `TabMinWidth` 90（后者 iced 无法表达，登记为限制）；
//! - **G24** drawer 项高 56——与 `LM.nav_item_height`(44) 的差异按 A1 取 56，见 [`geometry`]；
//! - **G25** ListItem 两/三行高（60 / 76）——`G4` 要求的 `row_height_1/2/3` 仍缺，本文件按
//!   `LM.row_height` + 每行 16 派生（A1 delta 相同）。
//!
//! ## 视觉证据（D30）
//!
//! `gallery/navigation.rs` 把四个组件摆成状态矩阵；`ULL_GALLERY_SECTION=navigation` 只渲染本段，
//! `ULL_NAV_PART=<list|tabs|drawer|appbar|all>` 再切成单块出图（1180×760 一屏装不下四块矩阵）。
//! 渲染层断言在 [`probe`]（读 `ULL_NAV_DIR` 下的真截图量像素），纯函数断言在各组件文件的 `tests`。

pub mod app_bar;
pub mod drawer;
pub mod geometry;
pub mod list_item;
pub mod matrix;
#[cfg(test)]
mod probe;
pub mod tabs;

pub use app_bar::{app_bar, AppBar, AppBarAction};
pub use drawer::{navigation, NavGroup, NavItem, Navigation};
pub use geometry::NavLayout;
pub use list_item::{list_item, ListItem};
pub use tabs::{tabs, Tab, Tabs, TabsVariant};
