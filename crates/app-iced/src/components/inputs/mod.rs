//! **输入类家族**（D39 §1 的 `inputs/`）：§9 TextField / §20 Search / §11 Slider / §8 Menu。
//!
//! 四个组件的共同纪律（与本目录外的契约一一对应）：
//!
//! - **对消息类型泛型**（`M: Clone`，不 import `crate::app::Message`）——组件库要能脱离业务验收；
//! - **`&'a Ui` 传入**，组件内部不 `Ui::new`（深浅热切靠外部重建整树）；
//! - **不持有 `String`**：文本框与搜索条是受控组件（借值 + `on_input`/`on_change` 回传），
//!   滑块也只借一个比例；
//! - **hover/pressed/focus/disabled 一律走 [`super::interaction`]**（α 只有一处定义）；
//! - **精确尺寸走 [`super::layout`]**（Q8/Q9 的两个固定解法），图标一律 `layout::fixed`；
//! - **零尺寸字面量**：能取令牌的取令牌，取不到的集中登记在 [`geometry`] 的 G16~G20。
//!
//! ## 文件分工
//!
//! | 文件 | 内容 |
//! |---|---|
//! | [`geometry`] | 内部几何 + G 编号常量（`FieldGeometry` / `SliderGeometry` / `MenuGeometry`） |
//! | [`field`] | [`text_field`]（Filled / Outlined） |
//! | [`search`] | [`search_bar`]（收起）+ [`search_view`]（展开 docked） |
//! | [`slider`] | [`slider`]（自绘 `Widget`：轨道 16 + 竖条 4×44） |
//! | [`menu`] | [`menu`]（`Menu` + `MenuItem` + 分组分隔线） |
//!
//! ## 与契约 §2 示例签名的关系
//!
//! 契约示例是 `filled(ui, label, on_press)`；本家族的四个组件都比三个参数多（变体 × 五态 ×
//! 前后槽位 × 错误/辅助文本），所以照按钮家族的先例把「部件参数」收进构造器结构体：
//! `text_field(ui, Field::new(value).label("名称"), on_input)`。**泛型纪律一字不改**，
//! 差别只是位置参数变命名参数（按钮家族已登记同一决定，见 `buttons` 模块文档）。

pub mod geometry;

mod field;
mod menu;
#[cfg(test)]
mod probe;
mod search;
mod slider;

pub use field::{text_field, Field, FieldVariant};
pub use menu::{menu, Menu, MenuItem};
pub use search::{search_bar, search_view, SearchBar, SearchView};
pub use slider::{slider, Slider};
