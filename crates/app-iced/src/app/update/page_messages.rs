//! 页面消息的落点（`on_page`）：解 M94 冲突时从 `update.rs` 拆出来。
//!
//! 为什么拆：`main` 上 `update.rs` 已经 **397 行**（M94 自己也是因此把下载页四支拆到
//! `update/download.rs`），日志页再往里塞「剪贴板 + 清日志」两支就顶破 400 硬线。
//! 与 M94 同一种做法：`app/update/**` 属本 mission scope。
//!
//! ## 穷尽性（M94 复审的 p3，本轮收掉）
//!
//! 这里是全 crate **唯一**对 [`pages::Message`] 的穷尽匹配点，所以**没有** `_ => {}` 兜底：
//! 谁往枚举里加变体却忘了接线，编译器立刻报 E0004。原来是 `_ => {}` + 注释「做穷尽性」——
//! 方向正好写反了，它**取消**穷尽性，把「漏接线」变成「点了没反应」。
//!
//! 代价：由前置分支（`on_download` / 本文件里的剪贴板分支）先消费的变体，在 match 里**仍要
//! 显式列出**。那是保住穷尽性的必要代价，也是这条守卫抓得住东西的原因。
//!
//! ## 边界（抓得住什么 / 抓不住什么）
//!
//! - 抓得住：**加一个 `pages::Message` 变体却没接线** → 编译失败（E0004），不会静默；
//! - 抓不住：加一个 `pages::Action` 变体 —— 它落进 `Action(action)` 的占位分支（打印一行
//!   `[page] 占位动作 …`），编译期不会报。这是占位期的既有设计，不是本文件能守的。

use unlucky_i18n::{Lang, LangChoice};

use crate::pages;
use crate::ui::lang::LangState;

use super::*;

// --------------------------------------------------------------- 界面语言（M106）
//
// 这三个方法住在本文件而不是 `app/state.rs`：`state.rs` 贴着 400 行硬线（M104 的 `agent`、
// M105 的 `download_draft` 都往那儿长），而 `set_language` 的唯一调用点就是本文件的
// `pages::Message::Language` 分支——落点与调用点放一起，读的人不用来回跳。
// 与 M93 把 appearance 那批 `App` 方法放进 `app/appearance.rs` 是同一手法。

impl App {
    /// 页面消息：导航走路由栈，真业务命令派发，纯视图态改本地呈现。
    pub(super) fn on_page(&mut self, message: pages::Message) -> Task<Message> {
        // 下载页的四支（含「刷新下载源」）在 `update/download.rs`：`update.rs` 早已贴 400 行
        if self.on_download(&message) {
            return Task::none();
        }

        match message {
            pages::Message::Navigate(route) => {
                // M109：进内容库时补一次检索（首个动作不该是「再点一下搜索」）。
                // 判据在 `ensure_content_loaded` 里（已有结果/请求在飞就不重复打网络）。
                let entering_content = route == Route::Content;
                self.state.navigate(route);
                self.view_dirty = true;
                self.refresh_view();
                if entering_content {
                    self.ensure_content_loaded();
                }
            }
            pages::Message::Launch(id) => self.dispatch(AppCommand::LaunchInstance(id)),
            pages::Message::Stop(id) => self.dispatch(AppCommand::StopInstance(id)),
            pages::Message::Appearance(change) => self.on_appearance(change),
            // M101：设置页的四条填页线共用一个投递口；组内变体在 `update/settings.rs` 里穷尽分发
            // （那一层没有 `_ =>`，见本文件末尾与 `update/settings/tests.rs` 的两条守门）
            pages::Message::Settings(group) => return self.on_settings(group),
            // M106：换界面语言（落盘 + 重建视图模型；档位不是业务命令，不派 `AppCommand`）。
            // 这是**唯一**一处绕开 M101 投递口的页面消息：语言档位不是「某一组的状态」，
            // 它要落盘到 `ui.language` 并重建整棵视图模型（设置页的语言行只是它的第一个出口）。
            pages::Message::Language(choice) => self.set_language(choice),
            // M109：内容库那一套意图在自己的文件里穷尽分发（`update/content.rs`）
            pages::Message::Content(message) => return self.on_content(message),
            // M95：摘要落剪贴板（文本由页面造，这里只搬）
            pages::Message::CopySummary(text) => return iced::clipboard::write(text),
            // 下面四支已由 `on_download` 提前消化；显式列出而不是 `_ => {}`，见模块文档
            pages::Message::SelectVersion(_)
            | pages::Message::InstallVersion(_)
            | pages::Message::CancelInstall
            // M105 的三支 + M107 的四支 + 分类：全部由 `on_download` 提前消化（显式列出保住穷尽性）
            | pages::Message::SetLoader(_)
            | pages::Message::SetLoaderVersion(_)
            | pages::Message::SetInstanceName(_)
            | pages::Message::SetCategory(_)
            // M107 的四支同属下载页（查询 / 清空 / 主版本桶 / 折叠类型栏）
            | pages::Message::SetVersionQuery(_)
            | pages::Message::ClearVersionQuery
            | pages::Message::SelectMajor(_)
            | pages::Message::ToggleTypeRail => debug_assert!(false, "应由 on_download 消费"),
            // M95：清空日志是**呈现层**操作（`log_lines` 是 presenter 的公开快照字段，
            // 不派业务命令、不写持久状态）
            pages::Message::Action(pages::Action::ClearLogs) => {
                self.state.log_lines.clear();
                self.view_dirty = true;
                self.refresh_view();
            }
            // M114：账户页（三轨登录 / 列表动作 / 表单输入）
            pages::Message::Account(message) => return self.on_account(message),
            // 还没接后端的占位动作：只打日志（哪几个、为什么，见 pages 模块文档的偏离登记）
            pages::Message::Action(action) => {
                eprintln!("[page] 占位动作 {action:?} pressed（没有对应 AppCommand）");
            }
        }

        Task::none()
    }
}

impl App {
    /// 当前生效的界面语言（= 目录选择；页面文案一律由它派生）。
    pub(in crate::app) fn lang(&self) -> Lang {
        self.lang.lang()
    }

    /// 设置页语言行要的两件：档位（画选中态）+ 生效语言（画回显）。
    pub(in crate::app) fn language(&self) -> pages::Language {
        self.lang.for_settings()
    }

    /// 换界面语言（设置页语言行 → `pages::Message::Language`）。
    ///
    /// 三件事缺一不可：①档位与结论换掉 ②视图模型重建（`view_dirty` + `refresh_view`，目录是
    /// 视图模型的一部分）③落盘（重启后保持所选语言）。字体不在这里重扫：中文档缺字体时的
    /// 判据/信号归 `i18n::font::decide` + `fonts/cjk.rs`（见报告「没验的部分」）。
    pub(in crate::app) fn set_language(&mut self, choice: LangChoice) {
        if choice == self.lang.choice() {
            return;
        }
        self.lang = LangState::of(choice);
        self.lang.log();
        if !self.bridge.persist_ui_language(choice.setting_value()) {
            eprintln!(
                "[i18n] 语言偏好没落盘（配置只读或数据根解析不出来）：本次生效，重启会回到旧值"
            );
        }
        self.lang.log_cjk();
        self.view_dirty = true;
        self.refresh_view();
    }
}

#[cfg(test)]
mod tests {
    use unlucky_design::color::SchemeFlavor;
    use unlucky_i18n::LangChoice;
    use unlucky_presenter::mock::MockBackend;
    use unlucky_presenter::settings_bridge::SettingsBridge;

    use crate::app::state::App;
    use crate::cli::Options;
    use crate::pages;
    use crate::theme::Families;

    const FAMILIES: Families = Families {
        text: "Segoe UI",
        display: "Segoe UI",
        mono: "Consolas",
    };

    /// 不碰磁盘的 App（mock 后端 + 内存设置桥），与 `app/tests.rs` 的 `headless` 同型。
    fn headless() -> App {
        App::with_presenter(
            Options::default(),
            FAMILIES,
            SchemeFlavor::Light,
            Box::new(MockBackend::new()),
            SettingsBridge::memory_only(),
        )
    }

    /// M106：设置页语言行点一下 → 生效语言与视图模型都跟着换。
    ///
    /// 抓得住：①`pages::Message::Language` 没接进 `on_page`（点了没反应，编译器不会报——它接得住，
    /// 但接错了分支就静默）；②`set_language` 只改字段不重建视图（界面要等下一次刷新才变）；
    /// ③档位与生效语言各说各话（设置页会画错选中态）。
    /// 抓不住：界面真的画成中文了没有（那要截图，见 `DevDocs/artifacts/m106/`）。
    #[test]
    fn the_language_message_switches_the_catalog_and_rebuilds_the_view() {
        let mut app = headless();
        assert_eq!(
            app.view.lang,
            app.lang(),
            "首帧的视图模型要已经按生效语言建好"
        );

        let _ = app.update(crate::app::message::Message::Page(
            pages::Message::Language(LangChoice::Fixed(unlucky_i18n::Lang::ZhCn)),
        ));
        assert_eq!(app.lang(), unlucky_i18n::Lang::ZhCn);
        assert_eq!(app.view.lang, unlucky_i18n::Lang::ZhCn, "目录要跟着换");
        assert_eq!(
            app.language(),
            pages::Language {
                choice: LangChoice::Fixed(unlucky_i18n::Lang::ZhCn),
                effective: unlucky_i18n::Lang::ZhCn,
            }
        );

        let refreshes = app.refreshes;
        let _ = app.update(crate::app::message::Message::Page(
            pages::Message::Language(LangChoice::Fixed(unlucky_i18n::Lang::En)),
        ));
        assert_eq!(app.view.lang, unlucky_i18n::Lang::En);
        assert_eq!(app.refreshes, refreshes + 1, "换语言要重建一次视图模型");

        // 同一档再点一次：什么都不该发生（不白重建）
        let refreshes = app.refreshes;
        let _ = app.update(crate::app::message::Message::Page(
            pages::Message::Language(LangChoice::Fixed(unlucky_i18n::Lang::En)),
        ));
        assert_eq!(app.refreshes, refreshes, "同一档不该再重建");
    }

    /// 抓得住：有人给 `on_page` 的 match 重新加 `_ => {}` 兜底 —— 那正是 M94 复审的 p3：
    /// 兜底让「加了变体忘了接线」从**编译错误**退化成「点了没反应」，这条测试把它拽回来。
    /// 抓不住：别的兜底写法（把 match 拆成 if-else 链、给子枚举加通配之后再漏变体），
    /// 也抓不住「加了变体但接错分支」——那两条只有编译器和人读得出来。
    #[test]
    fn the_page_match_has_no_wildcard_fallback() {
        let source = include_str!("page_messages.rs");
        let wildcards: Vec<(usize, &str)> = source
            .lines()
            .enumerate()
            .filter(|(_, line)| {
                let trimmed = line.trim();
                trimmed.starts_with("_ =>") || trimmed.starts_with("_=>")
            })
            .map(|(index, line)| (index + 1, line))
            .collect();

        assert!(
            wildcards.is_empty(),
            "on_page 里出现了 `_ =>` 兜底（行 {:?}）：它会取消对 pages::Message 的穷尽性，\
             新变体漏接线时编译器不再报 E0004",
            wildcards
        );
    }
}
