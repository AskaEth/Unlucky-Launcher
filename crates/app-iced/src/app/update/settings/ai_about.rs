//! AI / About 分组消息的落点（M101 `SETTINGSMSG`）：**只归 ai+about 线**。
//!
//! 消息口在 `pages/settings/ai_about.rs`（`pages::settings::ai_about::Message`）。
//!
//! 首版是空枚举上的 `match message {}`：这是**穷尽**的写法（不是兜底），加了变体不接线就报 E0004。

use super::super::*;

impl App {
    /// AI / About 分组的一条消息。首版没有可处理的东西（空枚举）。
    pub(super) fn on_settings_ai_about(
        &mut self,
        message: pages::settings::ai_about::Message,
    ) -> Task<Message> {
        match message {}
    }
}
