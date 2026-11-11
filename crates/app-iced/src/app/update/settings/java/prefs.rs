//! 三个设置键的 App 层落点（M197）：`java.installRoot` / `java.vendorPriority` / `java.auto_install`。
//!
//! ## 为什么这三项单独一个「保存」
//!
//! M175 的「保存」只写内存 + JVM 参数（`java.default_memory_mb` / `java.extra_args`），那是**运行时
//! 参数**；这三项是**装到哪、按什么顺序找、缺 JDK 时怎么准备**——改了它们的副作用是「下一次下载 /
//! 下一次扫描的落点变了」，与内存那两项混在一个按钮里，用户就不清楚自己刚保存的到底是哪一半。
//! 于是两段各有自己的脏标记与保存按钮，各回读各的键。
//!
//! ## 保存之后为什么立刻重扫
//!
//! `java.installRoot` 改了 ⇒ 托管根的集合变了 ⇒ 「已检测到的 JDK」那份列表可能整份不同。
//! 保存成功就重扫一次，界面不会出现「落点写着新目录，列表还是旧目录里的 JDK」。
//!
//! ## 回读校验
//!
//! `prefs_store::save` 返回的是**落盘后从磁盘读回来**的真值，[`Prefs::adopt`] 把草稿与
//! 「落盘真值」两半都对齐到它：用户看到的就是磁盘上那份（要是 schema 把值规整过，界面显示的是
//! 规整后的结果，而不是他刚敲的那串——这一条正是「改完回读验证已落盘」）。

use iced::Task;

use crate::app::message::Message;
use crate::app::state::App;
use crate::pages;

use super::super::java::write_notice;

impl App {
    /// `java.installRoot` 草稿（只改草稿，不写盘）。
    pub(super) fn edit_java_install_root(&mut self, value: String) {
        self.settings.java.prefs.install_root = value;
        self.settings.java.notice = None;
        self.settings.java.download_notice = None;
        self.view_dirty = true;
    }

    /// `java.auto_install` 开关（只改草稿）。
    pub(super) fn toggle_java_auto_install(&mut self, on: bool) {
        self.settings.java.prefs.auto_install = on;
        self.settings.java.notice = None;
        self.view_dirty = true;
    }

    /// 发行版顺序上移 / 下移一行。
    pub(super) fn move_java_vendor(&mut self, index: usize, up: bool) {
        let domestic = self.settings.java.effective.domestic;
        if up {
            self.settings.java.prefs.move_up(index, domestic);
        } else {
            self.settings.java.prefs.move_down(index, domestic);
        }
        self.settings.java.notice = None;
        self.view_dirty = true;
    }

    /// 恢复地区默认序（= 把这个键写回空，让地区那一层重新生效）。
    pub(super) fn reset_java_priority(&mut self) {
        self.settings.java.prefs.reset_priority();
        self.settings.java.notice = None;
        self.view_dirty = true;
    }

    /// 保存三个键：本地校验 → 落盘 → **用读回的真值**刷新草稿与生效值 → 重扫一次。
    pub(super) fn save_java_prefs(&mut self) {
        use pages::settings::java::Notice;

        self.view_dirty = true;
        if let Err(refusal) = self.settings.java.prefs.validate() {
            self.settings.java.notice = Some(Notice::Refused(refusal));
            return;
        }
        let Some(config) = pages::settings::java::store::config_of(&self.bridge) else {
            self.settings.java.notice = Some(Notice::NoStorage);
            return;
        };
        match pages::settings::java::prefs_store::save(&config, &self.settings.java.prefs) {
            Ok(snapshot) => {
                self.settings.java.prefs.adopt(snapshot);
                self.settings.java.effective = pages::settings::java::prefs_store::effective(
                    &config,
                    &self.settings.java.prefs,
                );
                // 落点 / 顺序变了：托管根的集合可能整份不同，立刻重扫一次。
                // 顺序要紧：`start_java_scan` 会清 `notice`，所以回执写在它之后。
                let _ = self.start_java_scan();
                self.settings.java.notice = Some(Notice::PrefsSaved);
                // M204：三键也已落盘（`java.installRoot` / `java.auto_install` / `java.vendorPriority`
                // 一次 `store.save()`），请 host 重发快照——否则 `AppState::settings_groups` 里
                // java 组那三行停在旧值，要等下一次任何别的写入才跟上。
                self.request_settings_refresh();
            }
            Err(error) => self.settings.java.notice = Some(write_notice(error)),
        }
    }
}

/// 消息→落点的那一层（`java.rs` 的 match 里一行转发）。
pub(super) fn on_edit_install_root(app: &mut App, value: String) -> Task<Message> {
    app.edit_java_install_root(value);
    Task::none()
}

/// `ToggleAutoInstall` 的转发。
pub(super) fn on_toggle_auto_install(app: &mut App, on: bool) -> Task<Message> {
    app.toggle_java_auto_install(on);
    Task::none()
}

/// `MoveVendor` 的转发。
pub(super) fn on_move_vendor(app: &mut App, index: usize, up: bool) -> Task<Message> {
    app.move_java_vendor(index, up);
    Task::none()
}

/// `ResetPriority` 的转发。
pub(super) fn on_reset_priority(app: &mut App) -> Task<Message> {
    app.reset_java_priority();
    Task::none()
}

/// `SavePrefs` 的转发。
pub(super) fn on_save_prefs(app: &mut App) -> Task<Message> {
    app.save_java_prefs();
    Task::none()
}
