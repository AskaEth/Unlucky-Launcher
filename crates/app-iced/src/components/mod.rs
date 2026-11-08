//! 组件库（D39 定案）：五家族并行，**目录所有权切分**，接口见
//! `DevDocs/SubAgentsTalk/app-iced-组件库契约.md`。
//!
//! ## 模块地图与归属（谁能写哪里）
//!
//! ```text
//! components/
//!   mod.rs          ← 本文件（COMPBASE 独占）：只声明子模块，不含实现
//!   interaction.rs  ← COMPBASE 独占：hover/focus/pressed/disabled 的唯一实现
//!   layout.rs       ← COMPBASE 独占：Q8/Q9 两个布局陷阱的固定解法
//!   buttons/        ← COMPBASE（12b §1 按钮家族 + §2 IconButton）
//!   divider.rs      ← COMPBASE（15 §22）
//!   selection/      ← 本文件只放桩；后续 mission 在自己的目录里填（§18/§19/§10/§13）
//!   inputs/         ← 同上（§9/§20/§11/§8）
//!   feedback/       ← 同上（§12/§14/§7/§15）
//!   navigation/     ← 同上（§4/§6/§21/§16）
//!   gallery/        ← 视觉验收 harness（`--screen=gallery`）
//! ```
//!
//! **后续 mission 只写自己目录内的文件，不改本文件**：四个家族桩已经在这里声明好，
//! 每个家族文件自带一个 `demo()`（占位 → 真组件），gallery 侧对应文件也在
//! `gallery/` 里预置了同构的桩。加组件 = 只改自家目录里的文件。
//!
//! ## 组件签名纪律（契约 §2）
//!
//! 组件**对消息类型泛型**，不 import `crate::app::Message`：
//!
//! ```text
//! pub fn filled<'a, M: Clone + 'a>(ui: &'a Ui, ...) -> Element<'a, M>
//! ```
//!
//! `Ui` 一律 `&'a Ui` 传入，组件内部**不许** `Ui::new`；深浅热切靠外部重建整树。
//!
//! ## 令牌纪律
//!
//! 组件里**零**色值 / 字号 / 半径 / 间距 / α 字面量：颜色走 [`crate::theme::Ui`]，
//! 状态层走 [`interaction`]，尺寸走 `unlucky_design` 的 `LayoutMetrics` / `Space` /
//! `IconSize` / `ShapeScale`。缺令牌 → 登记 G 编号（续 15 §25 的 G13），**不发明**。
//! [`gate`] 里的测试会扫本目录源码执行这条纪律。
//!
//! ## 为什么 `components` 由 `app.rs` 声明
//!
//! `src/lib.rs` 是五条组件线共用的文件（并行线改它必然冲突）。本模块挂在
//! `app.rs` 的 `#[path]` 声明下，路径仍是 `src/components/`，这样 COMPBASE 的
//! 全部改动都落在自己 scope 内。

pub mod buttons;
pub mod divider;
pub mod feedback;
pub mod gallery;
pub mod inputs;
pub mod interaction;
pub mod layout;
pub mod navigation;
pub mod selection;

#[cfg(test)]
mod gate;
