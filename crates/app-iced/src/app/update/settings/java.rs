//! Java 分组消息的落点（M101 `SETTINGSMSG`）：**只归 java 线**。
//!
//! 消息口在 `pages/settings/java.rs`（`pages::settings::java::Message`）。加一支消息 = 改那两个
//! 文件，`pages/mod.rs` / `page_messages.rs` / `app/update/settings.rs` 都不用动。
//!
//! 消息进来的通道有两条，都不经过共享文件：
//! 1. **页面点击**：`pages::Message::Settings(GroupMessage::Java(..))` → `on_page` → `on_settings`；
//! 2. **异步结果**：本文件自己构造 `Message::Page(pages::Message::Settings(GroupMessage::Java(..)))`
//!    （`app::Message::Page` 是既有通道，不用加新变体）。
//!
//! 穷尽性：这里对 `java::Message` 做**穷尽**匹配（没有 `_ =>`、没有 `other =>`）。加了变体不接线
//! 立刻 E0004；写成兜底会把编译错误降级成「点了没反应」，`app/update/settings/tests.rs` 的
//! `the_group_dispatch_has_no_catch_all_arm` 会红。

use super::super::*;

impl App {
    /// Java 分组的一条消息。
    pub(super) fn on_settings_java(
        &mut self,
        message: pages::settings::java::Message,
    ) -> Task<Message> {
        match message {
            // M101 r2 的**活的端到端证据**：只有测试构造这一支（java 线接第一个真控件时换成真变体）。
            // 它真的改一个 app 状态，所以「消息从外壳走到 handler」这件事有运行时证据，不只是编译期。
            #[cfg(test)]
            pages::settings::java::Message::Ping => {
                self.view_dirty = true;
                Task::none()
            }
        }
    }
}
