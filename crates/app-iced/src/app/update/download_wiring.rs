//! 下载页（M94）在 app 层的接线测试：**命令出得去、状态不自己变**。
//!
//! 从 `app/update.rs` 外置（那边 348 行 + 这个模块会顶破 400 行硬上限；同一句话的理由见
//! `crates/i18n/src/catalog/tests.rs`）。`use super::*` 依然指向 `crate::app::update`。

use std::cell::RefCell;
use std::rc::Rc;

use unlucky_presenter::dto::{AppEvent, Backend, VersionCategory};
use unlucky_presenter::settings_bridge::SettingsBridge;

use super::*;

/// 只记账、**不回事件**的后端（同 `app/tests/presenter.rs`）：命令出了 UI 层，状态一个字节都不许变。
/// 不用 `MockBackend`：它同步回事件，「本地乐观回显」与「事件回流」终态相同，断言会**假绿**。
struct Recorder(Rc<RefCell<Vec<AppCommand>>>);

impl Backend for Recorder {
    fn drain_events(&mut self) -> Vec<AppEvent> {
        Vec::new()
    }
    fn dispatch(&mut self, command: AppCommand) {
        self.0.borrow_mut().push(command);
    }
}

/// 四条下载页消息：三条派真指令，`SelectVersion` 只改视图模型（P3-2 的不乐观回显）。
/// 抓得住：意图被写成「本地改状态就算完」、或派错指令；抓不住：host 收到后真的开始下载。
#[test]
fn download_intents_dispatch_real_commands() {
    let log: Rc<RefCell<Vec<AppCommand>>> = Rc::new(RefCell::new(Vec::new()));
    let families = crate::theme::Families {
        text: "Segoe UI",
        display: "Segoe UI",
        mono: "Consolas",
    };
    let mut app = App::with_presenter(
        crate::cli::Options::default(),
        families,
        SchemeFlavor::Light,
        Box::new(Recorder(Rc::clone(&log))),
        SettingsBridge::memory_only(),
    );
    // M95 起 `on_page` 返回 `Task`（剪贴板命令要走返回的 Task）：这里只关心派发与视图模型，
    // 任务本身没有需要断言的后效，显式丢弃。
    let _ = app.on_page(pages::Message::InstallVersion("1.21.9".to_string()));
    let _ = app.on_page(pages::Message::CancelInstall);
    let _ = app.on_page(pages::Message::SetCategory(pages::Category::Release));
    let _ = app.on_page(pages::Message::SelectVersion("1.21.8".to_string()));
    // M105：「安装」不再是「立刻装原版」，而是把**草稿里的选项**一起交出去
    // （`CreateInstance` 带加载器 / 加载器版本 / 实例名）——这正是用户报的「没有加载器可选」的修复点。
    let expected = vec![
        AppCommand::CreateInstance(unlucky_presenter::dto::CreateRequest {
            version: "1.21.9".to_string(),
            loader: unlucky_presenter::dto::LoaderKind::Vanilla,
            loader_version: String::new(),
            name: String::new(),
        }),
        AppCommand::CancelInstall,
        AppCommand::ListVersions(VersionCategory::Release),
    ];
    assert_eq!(
        *log.borrow(),
        expected,
        "三条各派一条；`SelectVersion` 不派"
    );
    // M105：选中项住在**草稿**里（`refresh_view` 每次事件都会整棵重建页面模型，
    // 页面自己存不住它）；recorder 不回 `VersionListChanged` ⇒ 列表是空的 ⇒
    // 视图模型里那一行不存在，于是 `selected` 被过滤成 `None`（防「切分类后还留着一个点不动的安装」）。
    assert_eq!(app.download_draft.selected.as_deref(), Some("1.21.8"));
    assert_eq!(
        app.view.download.selected, None,
        "选中的行不在列表里时，视图模型不该拿着它"
    );
    assert_eq!(app.view.download.category, pages::Category::Release);
    assert_eq!(
        app.view.download.rows.len(),
        0,
        "recorder 不回事件 ⇒ 列表不该冒出来"
    );
    // 分类是本地过滤条件：**任何一次视图重建都不许把它打回 `All`**（复审 P2-1 的回归点）。
    // 抓得住：`refresh_view` 忘记把分类带回去（那一行删掉这条就红）。
    // 抓不住：真 host 是否按分类拉清单（那是 presenter 的 `spawn_list`，已报 finding）。
    app.refresh_view();
    assert_eq!(
        app.view.download.category,
        pages::Category::Release,
        "重建视图模型之后分类必须还在"
    );
}

/// **取消之后面板要回到「未安装态」且能立刻再装**（M105 的验收项，CI 可跑的那一半）。
///
/// 现场（用户原话）：「取消下载按钮点了没反应」。真根因是取消生效了又被旧任务的事件盖回去
/// （presenter 侧的代次守门在 `host/cancel_tests.rs`）；这一条钉的是**页面这一侧**：
/// 收到 `InstallProgress(None)` 之后，面板必须回到安装选项区、确认键可用、选中的版本还在
/// ——不需要用户重新点一遍版本。
///
/// 抓得住：取消后仍画着进度面板 / 选项区丢了选中 / 确认键还是禁用。
/// 抓不住：真网络下的取消延迟（那条是 `#[ignore]` 的真网络用例）。
#[test]
fn cancelling_gives_the_panel_back_to_the_install_options() {
    let log: Rc<RefCell<Vec<AppCommand>>> = Rc::new(RefCell::new(Vec::new()));
    let families = crate::theme::Families {
        text: "Segoe UI",
        display: "Segoe UI",
        mono: "Consolas",
    };
    let mut app = App::with_presenter(
        crate::cli::Options::default(),
        families,
        SchemeFlavor::Light,
        Box::new(Recorder(Rc::clone(&log))),
        SettingsBridge::memory_only(),
    );
    app.state.apply(AppEvent::Core(
        unlucky_presenter::dto::CoreEvent::VersionListChanged(vec![
            unlucky_presenter::dto::VersionSummary {
                id: "1.21.9".to_string(),
                kind: unlucky_presenter::dto::VersionKind::Release,
                released: "2026-09-20".to_string(),
                installed: false,
            },
        ]),
    ));
    app.refresh_view();

    // 选中 → 面板给出选项区
    let _ = app.on_page(pages::Message::SelectVersion("1.21.9".to_string()));
    let options = app
        .view
        .download
        .options
        .as_ref()
        .expect("选中版本之后要出现安装选项区");
    assert!(options.can_confirm, "原版 + 列表里的版本 = 可以装");
    assert_eq!(options.version, "1.21.9");

    // 开装：host 回一条「下载中」→ 面板换成进度
    app.state.apply(AppEvent::Core(
        unlucky_presenter::dto::CoreEvent::InstallProgress(Some(
            unlucky_presenter::dto::InstallProgressView {
                version: "1.21.9".to_string(),
                stage: unlucky_presenter::dto::InstallStage::Downloading,
                files_done: 9,
                files_total: 512,
                ..unlucky_presenter::dto::InstallProgressView::new()
            },
        )),
    ));
    app.refresh_view();
    assert!(app.view.download.install.is_some(), "装的时候面板讲进度");
    assert!(app.view.download.options.is_none(), "进度与选项互斥");

    // 取消：host 回 `InstallProgress(None)`（`CancelInstall` 的既有语义）
    let _ = app.on_page(pages::Message::CancelInstall);
    app.state.apply(AppEvent::Core(
        unlucky_presenter::dto::CoreEvent::InstallProgress(None),
    ));
    app.refresh_view();
    assert!(app.view.download.install.is_none(), "取消后面板必须收回");
    let options = app
        .view
        .download
        .options
        .as_ref()
        .expect("取消后要回到安装选项区（未安装态），而不是只剩一句提示");
    assert_eq!(
        options.version, "1.21.9",
        "选中项要还在，用户不必重新点一遍"
    );
    assert!(options.can_confirm, "取消后要能立刻再装一次");
}

/// **取消提示只说一次，且说的是页面语言**（复审 P2-2）。
///
/// 修前：`presenter` 在取消时直接推一条**写死的中文** Toast，而下载页此刻渲染英文 ⇒
/// 选项区英文、取消弹窗中文，同屏两语。现在 host 只发信号（`CoreEvent::InstallCancelled`），
/// 文案由页面层按 `Key::DownloadCancelled` 现求，语言跟页面走。
///
/// 抓得住：谁把中文写回 presenter（类型上已经做不到——它没有 i18n 依赖）、
/// 或者 app 层每次 `refresh_view` 都重复弹（下面的 `items().len() == 1`）。
/// 抓不住：`Lang` 本身接得对不对（M106 起语言来自 `App::lang()`，本测试**显式钉**成英文，
/// 否则这条断言会跟着跑测试那台机器的 locale 变——那是假绿）。
#[test]
fn cancelling_announces_once_in_the_page_language() {
    let log: Rc<RefCell<Vec<AppCommand>>> = Rc::new(RefCell::new(Vec::new()));
    let families = crate::theme::Families {
        text: "Segoe UI",
        display: "Segoe UI",
        mono: "Consolas",
    };
    let mut app = App::with_presenter(
        crate::cli::Options::default(),
        families,
        SchemeFlavor::Light,
        Box::new(Recorder(Rc::clone(&log))),
        SettingsBridge::memory_only(),
    );
    assert!(app.state.toasts.is_empty(), "出厂态不该有 Toast");

    // M106：语言显式钉成英文。不钉的话「页面语言」就是跑测试那台机器的 locale（英文 CI 绿、
    // 中文机器红）——那种测试证明不了任何事。钉住之后期望值仍从目录取，守的是同一条原意：
    // **提示语来自 i18n 目录，不是 presenter 写死的中文**。
    app.set_language(unlucky_i18n::LangChoice::Fixed(unlucky_i18n::Lang::En));

    app.state.apply(AppEvent::Core(
        unlucky_presenter::dto::CoreEvent::InstallCancelled,
    ));
    app.refresh_view();
    let items = app.state.toasts.items();
    assert_eq!(items.len(), 1, "取消要提示一条（且只一条）");
    let text = items[0].text.clone();
    assert_eq!(
        text,
        pages::cancelled_toast(unlucky_i18n::Lang::En),
        "提示语必须来自 i18n 目录（不是 presenter 写死的中文）"
    );
    assert!(
        !text.contains('取') && !text.contains('消'),
        "页面此刻是英文，取消提示不该混中文：{text}"
    );

    // 再重建一次视图：不该重复弹（信号是计数，不是「每拍都算一次」）
    app.refresh_view();
    assert_eq!(app.state.toasts.items().len(), 1, "同一次取消只提示一次");
    let _ = app.state.toasts.len();
}
