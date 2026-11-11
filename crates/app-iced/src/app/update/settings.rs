//! 设置页分组消息的**二级分发**（M101 `SETTINGSMSG`）：
//! `pages::Message::Settings(GroupMessage)` → 四条填页线各自的 handler。
//!
//! ## 为什么分两级
//!
//! `app/update/page_messages.rs` 对 [`pages::Message`] 做穷尽匹配（没有 `_ =>`，有
//! `the_page_match_has_no_wildcard_fallback` 守门）。设置页的四条填页线若各自往 `pages::Message`
//! 里加变体，就都得改那个文件——四路并行必然冲突。M101 把它们收进一个投递口
//! `pages::Message::Settings(GroupMessage)`，本文件再把 `GroupMessage` 分给四条线：
//!
//! ```text
//! GroupMessage::Java(m)               → settings/java.rs
//! GroupMessage::Download(m)           → settings/download.rs
//! GroupMessage::LanguageAppearance(m) → settings/language_appearance.rs
//! GroupMessage::AiAbout(m)            → settings/ai_about.rs
//! ```
//!
//! ## 穷尽性：抓得住什么 / 抓不住什么
//!
//! - **抓得住**：某条线给 `GroupMessage` 加了载荷变体（= 在自己文件里加了一支消息）却忘了在本
//!   match 里接线 → **E0004**；四个 handler 里对空枚举的 `match message {}` 一旦有人写成
//!   `_ => {}`，`the_group_dispatch_has_no_wildcard_fallback` 会红（兜底会把「漏接线」从编译
//!   错误降级成「点了没反应」，`page_messages.rs` 的同类守卫是这么写的，这里照抄）。
//! - **抓不住**：handler 忽略自己的消息（`let _ = message; Task::none()`）、两个 handler 身体写反
//!   或写成一样——类型对得上就编得过，只有人读能看出来。也因此「接错了组」的一半是编译期兜住的
//!   （载荷类型不同 ⇒ 编不过），另一半（接了个空壳）不是。
//!
//! ## 加一支设置页消息要改哪
//!
//! 自己那条线的两个文件：消息口（`pages/settings/<线>.rs`）+ 落点（本目录 `<线>.rs`）。
//! 本文件的 match 只在 `GroupMessage` **加变体**时才动，而那意味着新增一条线，不是加一支消息。

use super::*;

impl App {
    /// 设置页的分组消息：分给四条线各自的文件，本文件不认识任何一支具体消息。
    pub(super) fn on_settings(&mut self, message: pages::settings::GroupMessage) -> Task<Message> {
        match message {
            pages::settings::GroupMessage::Java(message) => self.on_settings_java(message),
            pages::settings::GroupMessage::Download(message) => self.on_settings_download(message),
            pages::settings::GroupMessage::LanguageAppearance(message) => {
                self.on_settings_language_appearance(message)
            }
            pages::settings::GroupMessage::AiAbout(message) => self.on_settings_ai_about(message),
        }
    }
}

mod ai_about;
mod download;
mod java;
mod language_appearance;

#[cfg(test)]
mod tests;
