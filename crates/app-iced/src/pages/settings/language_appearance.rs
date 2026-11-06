//! 设置页 language+appearance 线的**消息口 + 状态口**（M101 `SETTINGSMSG`）。
//!
//! 为什么另开一个文件而不是塞进 `general.rs`：这一条线管**两个分组**（General 的语言 / 主题 /
//! 启动页 / 关闭按钮，Appearance 的密度 / 圆角 / 动效 / 文字缩放）。两个分组各有自己的正文文件
//! （`settings/general.rs`、`settings/appearance.rs`），消息与状态却只能有一个类型挂到
//! [`GroupMessage::LanguageAppearance`](super::GroupMessage::LanguageAppearance) /
//! [`Groups::language_appearance`](super::Groups) 上——否则 `GroupMessage` 与 `Groups` 会多出第五个
//! 槽位，本线就要改 `settings/message.rs` / `settings/state.rs`（共享文件）。
//!
//! ## 本线的文件集合（只归本线，四条线两两不相交）
//!
//! | 文件 | 作用 |
//! |---|---|
//! | 本文件 | 消息枚举 + 状态结构（加一支消息 / 一个字段只改这里） |
//! | `settings/general.rs` / `settings/appearance.rs` | 两个分组各自的 `GROUP`（名字 / 图标 / 行 / body） |
//! | `settings/tests/general_section.rs` / `settings/tests/appearance_section.rs` | 两组的独立断言 |
//! | `crates/i18n/src/catalog/settings/general.rs` / `appearance.rs` | 两组的文案 key |
//! | `crates/app-iced/src/app/update/settings/language_appearance.rs` | 消息的落点 |
//!
//! 真控件那半边在 `pages/appearance/section.rs`（`appearance.rs` 只负责把 `body` 挂上去）。

/// 语言 + 外观线的状态（**只归本线**）。
///
/// 首版是空结构：这一条线还没有自己的运行期状态（外观那份状态是 `pages::appearance::State`，
/// 归 M93，走 `States::appearance` 那条既有通道，不在这里）。加字段只改本文件。
#[derive(Debug, Default)]
pub struct State {}

/// 语言 + 外观线的消息（**只归本线**）。
///
/// 首版是空枚举：这一条线还没有接真下拉 / 真控件之前，没有可点的东西。空枚举的好处是
/// `app/update/settings/language_appearance.rs` 里的 `match message {}` 天然穷尽——加了变体
/// 不接线就报 E0004，不会静默。
#[derive(Debug, Clone)]
pub enum Message {}
