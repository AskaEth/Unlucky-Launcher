//! **选择类家族**（D39 §1 的 ownership 切片）：Checkbox / Radio / Switch / Chip。
//!
//! | 组件 | 参考章节 | 尺寸一句话 |
//! |---|---|---|
//! | [`Checkbox`] | `research/15` §18 | 20×20 / 圆角 2 / 描边 2（向内收）/ 勾 12dp 方头 / Ø40 状态层 / 命中 48 |
//! | [`Radio`] | `research/15` §19 | 外圈 20 / 环线 2 / 内点 **10**（不是 12）/ Ø40 状态层 |
//! | [`Switch`] | `research/12b` §10 | 轨道 52×32 / 滑块 24·16 / Ø40 状态层 |
//! | [`Chip`] | `research/12b` §13 | 高 32 / 圆角 8 / 内边距 8 / Assist·Filter·Input |
//!
//! ## 分层（照抄按钮家族，审起来只需看四处）
//!
//! ```text
//! geometry.rs  令牌 → 像素（唯一把 ComponentMetrics / Space / ShapeScale 读成数字的地方）
//! paint.rs     Interaction × 选中 × 错误 → 角色 → 颜色（唯一写角色名的地方）
//! shell.rs     button 四态 → 内部自绘样式的帧内通道 + 勾形 SVG
//! <component>.rs  布局与装配（一行一个 Element）
//! ```
//!
//! ## 纪律（D39 契约 §2 / §3）
//!
//! - 全部组件**对消息类型泛型**（`M: Clone`），不 import `crate::app::Message`；
//! - hover / pressed / focus / disabled 的 α 只从 [`super::interaction`] 取（本目录零 α 字面量）；
//! - 精确尺寸只从 [`super::layout`]（`centered_fixed` / `fixed`）与 `geometry` 取，不许手写；
//! - 图标一律 `icons::view_at(kind, color, geometry.icon)` + 固定尺寸容器（Q9）：本目录的盒是
//!   **18（chip）与 16（switch 滑块）**，都不是 `icons::RENDER_PX`(24)，所以必须按盒烘焙——用
//!   `icons::view`（24 烘焙）会被布局缩到 1.1px 视觉线宽（复审 r1 P2-2）。圆角按 D32 分层
//!   （微控件 2 / 容器刻度 / full）。

mod checkbox;
mod chip;
mod radio;
mod shell;
mod switch;

pub mod geometry;
pub mod matrix;
pub mod paint;

#[cfg(test)]
mod pixel;
#[cfg(test)]
mod probe;
#[cfg(test)]
mod tests;

pub use checkbox::{checkbox, CheckValue, Checkbox};
pub use chip::{chip, Chip};
pub use paint::ChipVariant;
pub use radio::{radio, Radio};
pub use switch::{switch, Switch};
