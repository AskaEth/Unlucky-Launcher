//! M334：Java 不符框的**第三个出口**（「下载 Java {major}」）在界面这一侧的状态机。
//!
//! | 用例 | 抓得住 |
//! |---|---|
//! | [`the_dialog_offers_a_download_exit_named_by_the_major`] | 第三个出口没接线（框里只有「去设置」与「仍要启动」）、或按钮不带大版本号（用户不知道要等哪一个） |
//! | [`clicking_download_dispatches_the_install_and_shows_progress`] | 点了不派 `InstallJavaFor`（用户点了没反应）、或派了却不在正文里给进展（用户盯着一个不动的框） |
//! | [`the_failure_line_names_the_vendor_from_the_catalog`] | 失败不给原因、或把内部 id（`temurin`）直接上屏 |
//! | [`the_done_state_offers_a_retry_that_rethrows_the_gate`] | 装好了却只说「已装好」不给下一步、或「现在启动」派错了命令（派 `LaunchInstanceAnyway` = 绕过闸门那次一次性放行） |
//!
//! 抓不住：像素排版（按钮会不会挤成两行由出图守）；也抓不住**真的**把 JDK 装到盘上——
//! 输入是一份构造出来的 `JavaPageView`，产品侧那一段由 `presenter::host::launch::m334_tests` 断。

use std::cell::RefCell;
use std::rc::Rc;

use unlucky_i18n::catalog::Key;
use unlucky_i18n::Lang;
use unlucky_presenter::dto::{
    AppCommand, AppEvent, Backend, CoreEvent, JavaChainOutcome, JavaChainStep, JavaGuardDirection,
    JavaPageView, JavaRuntimeView, LaunchBlockedView,
};
use unlucky_presenter::settings_bridge::SettingsBridge;

use crate::app::dialog::{current, DialogRequestView, GuardInstall, JavaGuardCard};
use crate::app::state::App;
use crate::cli::Options;
use crate::theme::Families;

pub(super) const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

/// 「1.12.2 只能用 Java 8，托管里只有 17」这一次拦下的事实。
pub(super) fn blocked() -> LaunchBlockedView {
    LaunchBlockedView {
        id: "1.12.2".to_string(),
        name: "better-mc-1122".to_string(),
        version_id: "1.12.2".to_string(),
        direction: JavaGuardDirection::TooNew,
        required_min: 8,
        required_max: Some(8),
        actual: 17,
        install_major: Some(8),
        evidence: None,
    }
}

/// 记下 `dispatch` 的后端（只观察，不回事件）。
#[derive(Default)]
struct RecorderBackend {
    dispatched: Rc<RefCell<Vec<AppCommand>>>,
}

impl Backend for RecorderBackend {
    fn drain_events(&mut self) -> Vec<AppEvent> {
        Vec::new()
    }

    fn dispatch(&mut self, command: AppCommand) {
        self.dispatched.borrow_mut().push(command);
    }
}

pub(super) fn recording() -> (App, Rc<RefCell<Vec<AppCommand>>>) {
    let dispatched = Rc::new(RefCell::new(Vec::new()));
    let backend = RecorderBackend {
        dispatched: Rc::clone(&dispatched),
    };
    let app = App::with_presenter(
        Options::default(),
        FAMILIES,
        unlucky_design::color::SchemeFlavor::Light,
        Box::new(backend),
        SettingsBridge::memory_only(),
    );
    (app, dispatched)
}

/// 把一次拦下事件喂进产品路径（`AppState::apply` + `refresh_view`）。
pub(super) fn deliver(app: &mut App, view: &LaunchBlockedView) {
    app.state
        .apply(AppEvent::Core(CoreEvent::LaunchBlocked(Box::new(
            view.clone(),
        ))));
    app.refresh_view();
}

/// 喂一份 Java 页快照（与 host 经 `CoreEvent::JavaChanged` 送来的同一形状）。
pub(super) fn java_page(
    app: &mut App,
    busy: bool,
    majors: &[u32],
    chain: Vec<JavaChainStep>,
    message: &str,
) {
    let mut page = JavaPageView::new();
    page.busy = busy;
    page.message = message.to_string();
    page.chain = chain;
    page.runtimes = majors
        .iter()
        .map(|major| JavaRuntimeView {
            binary: format!("D:/jdks/{major}/bin/java.exe"),
            version: major.to_string(),
            major: *major,
            vendor: "Temurin".to_string(),
            origin: "managed".to_string(),
        })
        .collect();
    app.state
        .apply(AppEvent::Core(CoreEvent::JavaChanged(page)));
    app.refresh_view();
}

pub(super) fn hit(vendor: &str) -> JavaChainStep {
    JavaChainStep {
        vendor: vendor.to_string(),
        outcome: JavaChainOutcome::Hit,
        detail: "命中 Adoptium 8".to_string(),
    }
}

pub(super) fn failed(vendor: &str, detail: &str) -> JavaChainStep {
    JavaChainStep {
        vendor: vendor.to_string(),
        outcome: JavaChainOutcome::Failed,
        detail: detail.to_string(),
    }
}

/// 第三枚按钮的文字（`Modal` 的字段在 `current` 之后才定，所以只能从模型上读）。
pub(super) fn tertiary(app: &mut App, lang: Lang) -> String {
    let request = app.state.dialogs.current().expect("框还在").clone();
    let card = app.java_guard.clone();
    let modal = current(lang, &DialogRequestView::of(&request), None, card.as_ref());
    modal.tertiary.unwrap_or_default()
}

pub(super) fn body(app: &mut App, lang: Lang) -> String {
    let request = app.state.dialogs.current().expect("框还在").clone();
    let card = app.java_guard.clone();
    current(lang, &DialogRequestView::of(&request), None, card.as_ref())
        .body
        .join("\n")
}

/// 框里必须有第三个出口，而且它带着**大版本号**（中英各一次）。
#[test]
fn the_dialog_offers_a_download_exit_named_by_the_major() {
    let (mut app, _recorded) = recording();
    deliver(&mut app, &blocked());
    for lang in [Lang::En, Lang::ZhCn] {
        let shown = tertiary(&mut app, lang);
        let expected = lang
            .catalog()
            .render(Key::JavaGuardDownload, &[("major", "8")]);
        assert_eq!(shown, expected, "{}：第三个出口要带大版本号", lang.tag());
        assert!(shown.contains('8'), "{shown}");
    }
    assert_ne!(
        Lang::En.catalog().text(Key::JavaGuardDownload),
        Lang::ZhCn.catalog().text(Key::JavaGuardDownload),
        "两套译文必须真的不同（漏译会被这条抓住）"
    );
}

/// 点「下载 Java 8」⇒ 派 `InstallJavaFor { major: 8 }`，正文立刻出现进展那一句。
#[test]
fn clicking_download_dispatches_the_install_and_shows_progress() {
    let (mut app, recorded) = recording();
    deliver(&mut app, &blocked());
    let _ = app.update(crate::app::Message::Dialog(
        crate::app::dialog::Message::Tertiary,
    ));
    assert_eq!(
        *recorded.borrow(),
        vec![AppCommand::InstallJavaFor { major: 8 }],
        "第三个出口必须派**按大版本**装的那条命令（不是设置页那条按第一条实例挑的）"
    );
    assert!(
        app.state.dialogs.current().is_some(),
        "点了下载不许把框关掉"
    );

    // 还没收到快照：按钮写「正在下载…」，正文说**另一句**（不复读，复审 r1 nit）。
    let lang = app.lang();
    assert_eq!(
        tertiary(&mut app, lang),
        lang.catalog()
            .render(Key::JavaGuardDownloading, &[("major", "8")])
    );
    assert!(
        body(&mut app, lang).contains(lang.catalog().text(Key::JavaGuardDownloadingHint)),
        "正文必须有进展那一句：{}",
        body(&mut app, lang)
    );

    // 下载中（host 那份 `busy = true` 的快照到了）：按钮照旧「正在下载…」，正文换成**另一句**
    // （复审 r1 nit：两行一模一样的话等于没说）。
    java_page(&mut app, true, &[17], Vec::new(), "");
    assert_eq!(
        tertiary(&mut app, lang),
        lang.catalog()
            .render(Key::JavaGuardDownloading, &[("major", "8")])
    );
    assert!(
        body(&mut app, lang).contains(lang.catalog().text(Key::JavaGuardDownloadingHint)),
        "正文该说「这一步要联网下载」那句：{}",
        body(&mut app, lang)
    );

    // 再点一次不会重复派（一个框只该有一次安装在跑）。
    let _ = app.update(crate::app::Message::Dialog(
        crate::app::dialog::Message::Tertiary,
    ));
    assert_eq!(recorded.borrow().len(), 1, "下载中点第二下不该再派一条");
}

/// 装失败：正文说「Java 8 没能装上（<发行版>: <core 原文>）」，**发行版名走目录**。
#[test]
fn the_failure_line_names_the_vendor_from_the_catalog() {
    let (mut app, _recorded) = recording();
    deliver(&mut app, &blocked());
    let _ = app.update(crate::app::Message::Dialog(
        crate::app::dialog::Message::Tertiary,
    ));
    java_page(
        &mut app,
        false,
        &[17],
        vec![failed("temurin", "Adoptium API 不可达（HTTP 503）")],
        "",
    );
    let lang = app.lang();
    let shown = body(&mut app, lang);
    assert!(
        shown.contains(lang.catalog().text(Key::JavaVendorTemurin)),
        "失败原因必须指名发行版（目录文案）：{shown}"
    );
    assert!(
        shown.contains("HTTP 503"),
        "core 的原文要原样留着（取证材料）：{shown}"
    );
    assert!(!shown.contains("temurin"), "内部 id 不许直接上屏：{shown}");
    // 失败之后按钮回到「下载 Java 8」：用户可以再点一次。
    assert_eq!(
        tertiary(&mut app, lang),
        lang.catalog()
            .render(Key::JavaGuardDownload, &[("major", "8")])
    );
    assert!(matches!(
        app.java_guard.as_ref().map(|card| &card.install),
        Some(GuardInstall::Failed { .. })
    ));
}

/// 装好了：「现在启动」派 **`LaunchInstance`**（重新过闸），而不是「仍要启动」那次一次性放行。
#[test]
fn the_done_state_offers_a_retry_that_rethrows_the_gate() {
    let (mut app, recorded) = recording();
    deliver(&mut app, &blocked());
    let _ = app.update(crate::app::Message::Dialog(
        crate::app::dialog::Message::Tertiary,
    ));
    // 托管目录里出现了 Java 8（host 那份终态快照）：终态就是「装好了」。
    java_page(&mut app, false, &[8, 17], vec![hit("temurin")], "");
    assert_eq!(
        app.java_guard.as_ref().map(|card| card.install.clone()),
        Some(GuardInstall::Done)
    );
    let lang = app.lang();
    assert_eq!(
        tertiary(&mut app, lang),
        lang.catalog().text(Key::JavaGuardRetryLaunch),
        "装好了按钮要换成「现在启动」"
    );
    assert!(
        body(&mut app, lang).contains(
            &lang
                .catalog()
                .render(Key::JavaGuardDownloadDone, &[("major", "8")])
        ),
        "正文要说「Java 8 已装好」：{}",
        body(&mut app, lang)
    );

    let _ = app.update(crate::app::Message::Dialog(
        crate::app::dialog::Message::Tertiary,
    ));
    assert_eq!(
        *recorded.borrow(),
        vec![
            AppCommand::InstallJavaFor { major: 8 },
            AppCommand::LaunchInstance("1.12.2".to_string()),
        ],
        "「现在启动」必须走 `LaunchInstance`（照旧过闸），不许用「仍要启动」绕过判据"
    );
    assert!(app.state.dialogs.is_empty(), "点了启动框要关掉");
}

/// 卡片夹具的形状（防止有人给 `JavaGuardCard` 加字段时漏掉 `install` 的默认态）。
#[test]
fn a_fresh_card_starts_idle() {
    let card = JavaGuardCard {
        view: blocked(),
        dialog: 1,
        install: GuardInstall::default(),
        consumed: None,
    };
    assert_eq!(card.install, GuardInstall::Idle);
    assert!(card.consumed.is_none());
}
