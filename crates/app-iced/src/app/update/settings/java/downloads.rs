//! 「下载一个 JDK」的 App 层落点（M197）。
//!
//! 三件事：
//!
//! | 函数 | 谁调 | 干什么 |
//! |---|---|---|
//! | [`App::start_java_download`] | `Download(major)` 消息 | 本地判据（目录/承接方/落点/是否已在跑）→ 起后台作业 |
//! | [`App::hydrate_java_downloads`] | 心跳那一拍（经 `hydrate_java_scan`） | 把作业快照灌成视图差量；**刚成功**时重扫 + 按 `java.auto_install` 决定要不要设为默认 |
//!
//! ## 为什么轮询挂在 `hydrate_java_scan` 上
//!
//! M175 的扫描已经是「后台线程 + 快照 + 心跳比对」的形状，`app/update/clock.rs` 每拍只调
//! `hydrate_java_scan()` 这一个入口。下载作业是同一族东西（后台线程 + 快照），所以复用那一拍，
//! `clock.rs` 一个字节都不用改——代价是**下载期间进度按 1 Hz 刷新**（帧时钟的判据在
//! `app/subscription.rs` 的 `needs_frames`，那是本 mission scope 之外的文件）。百分比一秒动一次
//! 对一次 100 MB 级的下载是够的；要紧的是它**会动**，而不是每秒动 60 次。
//!
//! ## 装完之后为什么要重扫
//!
//! 列表里那一行「已安装：…」来自**真扫盘的结论**（M175 的 `ScanSlot`），不是另存的安装记录。
//! 装完不重扫，界面上就会出现「下载成功」与「列表里没有它」并存——正是本仓最忌讳的那种自相矛盾。

use iced::Task;

use crate::app::message::Message;
use crate::app::state::App;
use crate::pages;

use unlucky_core::game::java::catalog;

use crate::pages::settings::java::{prefs_store, store};

impl App {
    /// 下载并安装某个大版本（`Download(major)` 的落点）。
    ///
    /// 四道本地判据，任何一道不过都在下载区顶上写清原因（点了没反应是禁止的）：
    /// ① 目录里有这个大版本；② 没有别的作业在跑；③ 有可写的设置文件与能解析的落点；
    /// ④ 这一档在这台机器上真有一家发行版承接（纯判据，不联网）。
    pub(super) fn start_java_download(&mut self, major: u32) {
        use pages::settings::java::downloads::{gh_proxy_of_config, Refusal, Request};

        self.settings.java.download_notice = None;
        self.settings.java.notice = None;
        self.view_dirty = true;

        if catalog::find(major).is_none() {
            self.settings.java.download_notice = Some(Refusal::UnknownMajor);
            return;
        }
        if self.settings.java.downloads.busy() {
            self.settings.java.download_notice = Some(Refusal::Busy);
            return;
        }
        let Some(config) = store::config_of(&self.bridge) else {
            self.settings.java.download_notice = Some(Refusal::NoStorage);
            return;
        };
        let priority = self.settings.java.priority_vendors();
        let domestic = self.settings.java.effective.domestic;
        if catalog::vendors_for(major, &priority, domestic).is_empty() {
            self.settings.java.download_notice = Some(Refusal::NoVendor);
            return;
        }
        let saved_root = self.settings.java.prefs.saved_install_root.clone();
        let Some(cache_root) = prefs_store::cache_root(&config) else {
            self.settings.java.download_notice = Some(Refusal::NoStorage);
            return;
        };
        let Some(install_root) = prefs_store::install_root(&config, &saved_root) else {
            // 有设置文件但解析不出落点（standard 模式读不到系统目录）：说这一句，
            // 而不是含糊的「没有可用的设置文件」。
            self.settings.java.download_notice = Some(Refusal::RootUnresolved);
            return;
        };
        let started = self.settings.java.downloads.start(Request {
            major,
            cache_root,
            install_root,
            priority,
            // `download.gh_proxy`（M197 的任务书那句「默认值可改」）：空 = 跟随内置默认，
            // `off` = 关，其余 = 覆盖前缀。Java 页不画这一项的 UI（归 M193 的下载设置页）。
            gh_proxy: gh_proxy_of_config(&config),
        });
        if !started {
            self.settings.java.download_notice = Some(Refusal::Busy);
        }
    }

    /// 把作业快照灌成视图差量（心跳那一拍调）。返回是否变了。
    pub(in crate::app) fn hydrate_java_downloads(&mut self) -> bool {
        let report = self.settings.java.downloads.report();
        if report == self.settings.java.download_seen {
            return false;
        }
        let fresh = report
            .success
            .clone()
            .filter(|success| self.settings.java.download_seen.success.as_ref() != Some(success));
        self.settings.java.download_seen = report;
        if let Some(success) = fresh {
            // 新装上的一份：先重扫（列表要出现它），再按 `java.auto_install` 决定默认值。
            let _ = self.start_java_scan();
            if self.settings.java.prefs.saved_auto_install
                && self.settings.java.selected.trim().is_empty()
            {
                self.select_java_default(&success.binary);
            }
        }
        self.view_dirty = true;
        true
    }
}

/// 消息→落点的那一层（`java.rs` 的 match 里一行转发）。
pub(super) fn on_download(app: &mut App, major: u32) -> Task<Message> {
    app.start_java_download(major);
    Task::none()
}
