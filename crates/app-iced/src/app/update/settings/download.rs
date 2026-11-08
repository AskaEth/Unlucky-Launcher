//! Downloads 分组消息的落点（M101 `SETTINGSMSG`）：**只归 download 线**。
//!
//! 消息口在 `pages/settings/download.rs`（`pages::settings::download::Message`）。与下载页
//! （`pages/download*` 与 `app/update/download.rs`）无关：那是「装哪个版本」，这一组是「怎么下」。
//!
//! 首版是空枚举上的 `match message {}`：这是**穷尽**的写法（不是兜底），加了变体不接线就报 E0004。

use super::super::*;

impl App {
    /// Downloads 分组的一条消息。首版没有可处理的东西（空枚举）。
    pub(super) fn on_settings_download(
        &mut self,
        message: pages::settings::download::Message,
    ) -> Task<Message> {
        match message {}
    }
}
