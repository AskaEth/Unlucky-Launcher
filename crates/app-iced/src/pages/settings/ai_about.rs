//! 设置页「AI / About」分组：**预留**（页面上还没有这个分组，左列 4 项里没有它）。
//!
//! 为什么先留一个空文件：设置页正文接下来由四条线并行接（java / download / language+appearance /
//! ai+about）。文件位置先占好，四条线就不会在同一天抢同一个新文件名；本文件现在是**只有文档**的
//! 空模块（不产生任何代码，`-D warnings` 不会红）。
//!
//! ## 接线步骤（ai+about 线照做）
//!
//! 1. 在本文件写 `pub(super) const GROUP: Group = Group { … }`，形状抄 `java.rs`；
//! 2. 到 `settings/mod.rs` 的 `GROUPS` 里加一行 `ai_about::GROUP,`，并把数组长度 `4` 改成 `5`；
//! 3. 文案 key 写进 `crates/i18n/src/catalog/settings/ai.rs`（那边已经接进装配链，同样只改一个文件）。
//!
//! 界面上「多了一组」这件事本身是行为变化，所以**只在真有那一组内容时才做**，不要为占位先加。
//!
//! ## M101 起本文件还装本线的状态与消息口
//!
//! - 状态：[`State`]（`Groups.ai_about` 的载荷，加字段只改本文件）；
//! - 消息：[`Message`]（落点 `app/update/settings/ai_about.rs`）。
//!
//! 上第 5 组时**额外**要动的共享文件（按上面的步骤 2 + 测试装配）：
//! - `settings/mod.rs` 的 `GROUPS`（加一行，长度 +1）；
//! - `settings/tests/mod.rs` 顶部加一行 `mod ai_section;`（新测试文件不挂 `mod` 就不编译），
//!   并且左列顺序那条字面量断言会红——那是刻意的（页面上多一组就是观感变了，要人确认）。
//!
//! 注意 i18n 那边是按**分组**命名的，文案文件叫 `ai.rs` 而不是 `ai_about.rs`。

/// AI / About 分组的运行期状态（M101 r2：**只归 ai+about 线**）。
///
/// 加字段只改本文件：`Groups` 只认 `Groups.ai_about` 这一个字段。
#[derive(Debug, Default)]
pub struct State {}

/// AI / About 线的消息（**只归本线**）。
///
/// 首版是空枚举：这一条线的分组还没上页面。空枚举的好处是
/// `app/update/settings/ai_about.rs` 里的 `match message {}` 天然穷尽——加了变体不接线就报
/// E0004，不会静默。
#[derive(Debug, Clone)]
pub enum Message {}
