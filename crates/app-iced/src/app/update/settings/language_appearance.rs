//! 语言 + 外观两组消息的落点（M101 `SETTINGSMSG`）：**只归 language+appearance 线**。
//!
//! 消息口在 `pages/settings/language_appearance.rs`（`pages::settings::language_appearance::Message`）。
//! 为什么两组共用一个消息口：它俩本就是同一条线的两个分组（General 的语言 / 主题 / 启动页，
//! Appearance 的密度 / 圆角 / 动效 / 文字缩放），`GroupMessage` 只给这条线留一个变体。
//!
//! 首版是空枚举上的 `match message {}`：这是**穷尽**的写法（不是兜底），加了变体不接线就报 E0004。

use super::super::*;

impl App {
    /// 语言 / 外观线的一条消息。首版没有可处理的东西（空枚举）。
    pub(super) fn on_settings_language_appearance(
        &mut self,
        message: pages::settings::language_appearance::Message,
    ) -> Task<Message> {
        match message {}
    }
}
