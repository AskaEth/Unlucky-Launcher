//! 设置页「助手（AI）」分组（M174）：提供商 / 端点 / 模型 / 密钥 + 「测试连接」。
//!
//! ## 为什么单独一个目录
//!
//! `pages/settings/mod.rs` 里只挂一行 `mod ai;`（与 M173 的版面改动不碰面），本组的状态 / 读写 /
//! 渲染 / 用例都在这几个文件里：[`state`]（草稿 + 探测结论 + 校验）、[`store`]（真读写
//! `config/launcher.json` 的 `ai.*` 六键）、[`view`]（一张卡片）、`seed`（出图种子，M390 从
//! `state.rs` 纯搬移，为守 400 行）与 `tests`。
//!
//! ## 三条纪律（本组的全部安全面）
//!
//! 1. **密钥不进 DTO、不进日志、不进错误文案**：草稿是 [`state::Secret`]（`Debug` 手写成掩码），
//!    落盘走 [`store::save`]（在进程内直接过 `core::setting` 的 schema，**不经过**
//!    `AppCommand::SetSetting` 那条可序列化的指令通道——那条通道会把明文带进事件流/日志）；
//!    界面上默认掩码，只有用户点「显示」才明文。已存密钥的**长度**是唯一允许上屏的数值。
//! 2. **空/非法值不保存**：[`state::State::validate`] 先拦（提供商词表、`http(s)://` 前缀、模型名），
//!    schema 再拦（越界/类型），两次都过才写盘。失败时界面上给的是拒绝原因，不是「已保存」。
//! 3. **不配置就不可能发请求**：「测试连接」按钮在 [`state::State::can_test`] 为假时是禁用的，
//!    真发不发由 `unlucky_cli::commands::agent::probe` 里的 `to_config()` 再拦一次
//!    （假 provider 构造计数 == 0 的守卫在那里）。
//!
//! ## 唯一例外：`ai.yolo`（M390）点了就落盘，不进「保存」那条事务
//!
//! 它是**安全开关**，两条理由让它不能等用户点「保存」：①[`state::State::validate`] 拦的是
//! 端点/模型，一个**跟 YOLO 无关**的草稿错误会把「关掉 YOLO」一起卡住——安全开关不许被别人
//! 锁住；②「保存」写的是整张表单，点一下开关顺手把半截草稿落盘不是用户要的。
//! 落盘仍走本模块 [`store`]（同一个 `SettingStore::set` + `save` + 重读），只是键-scoped 的
//! 一个入口 [`store::set_yolo`]，**没有**第二条写机制。

pub mod outcome;
mod seed;
pub mod state;
pub mod store;
mod toggles;
pub mod view;

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "yolo_tests.rs"]
mod yolo_tests;

pub use outcome::ProbeResult;
pub use state::{Probe, ProviderId, SaveNotice, Secret, State};
