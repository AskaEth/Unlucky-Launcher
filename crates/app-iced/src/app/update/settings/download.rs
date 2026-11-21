//! Downloads 分组消息的落点（M201）：**设置写回**的那一段（M101 `SETTINGSMSG` 的漏斗不变）。
//!
//! 消息口在 `pages/settings/download.rs`（`pages::settings::download::Message`）。本文件只有一件事：
//! 把「用户改了哪个键」翻成 `AppCommand::SetSetting` —— 那是 presenter 已有的写回闭环
//! （`host/dispatch.rs` 的 `SetSetting` 分支：`settings::set` 落盘 → `apply_tuning` → 重新发
//! `CoreEvent::SettingsChanged`）。
//!
//! ## 不做乐观回显
//!
//! 这里**不**改任何「当前值」：界面上的值是 presenter 快照（`pages::Model.settings`）的投影，
//! 只有 host 把 `SettingsChanged` 发回来才会变。理由与下载页的安装/启动同一条纪律——本地改一份、
//! host 回来再改一次，两边打架时界面就会各说一套（尤其是写盘失败：乐观回显会让用户以为改成了）。
//!
//! ## 文本行为什么要「应用」才提交
//!
//! 见 `pages/settings/download.rs` 的文件头：每敲一个键写一次盘会把半截值落进去。所以
//! `Edit` 只改本地草稿（`view_dirty` 都不用翻：草稿由 `States` 直接读），`Commit` 才派发。
//! 提交之后草稿当场丢掉——快照随即成为唯一真值，写失败时行上会回到文件里的值 + presenter 的
//! 错误 Toast 说清原因（比留着一个没生效的草稿更诚实）。

use super::super::*;

use unlucky_presenter::dto::{find_row, AppCommand};

impl App {
    /// Downloads 分组的一条消息。
    pub(super) fn on_settings_download(
        &mut self,
        message: pages::settings::download::Message,
    ) -> Task<Message> {
        use pages::settings::download::Message;

        match message {
            Message::Set { key, value } => {
                self.dispatch(AppCommand::SetSetting { key, value });
                Task::none()
            }
            Message::Edit { key, value } => {
                // 只改草稿：`body` 每一帧都从 `Groups.download` 读它，所以这里不必重建视图模型。
                self.settings.download.edit(&key, value);
                Task::none()
            }
            Message::Commit { key } => {
                let value = self.pending_text(&key);
                self.settings.download.clear(&key);
                self.dispatch(AppCommand::SetSetting { key, value });
                Task::none()
            }
            // 动作行（「重新检测地区」）：键名与动作 id 都由 presenter 的快照给，取值被
            // `settings::set` 忽略（那条分支先认键、再解析值）。
            Message::Action { key } => {
                self.dispatch(AppCommand::SetSetting {
                    key,
                    value: String::new(),
                });
                Task::none()
            }
        }
    }

    /// 这一行要提交的文本：本地草稿优先，否则用快照里的当前值（= 文件里的值）。
    ///
    /// 空串是合法提交（「留空 = 跟随默认」是这一组真有的语义），所以只有**两者都缺**才返回空串。
    fn pending_text(&self, key: &str) -> String {
        self.settings
            .download
            .pending(key)
            .or_else(|| find_row(&self.view.settings, key).map(|row| row.value_text.clone()))
            .unwrap_or_default()
    }
}

#[cfg(test)]
#[path = "download_tests.rs"]
mod tests;
