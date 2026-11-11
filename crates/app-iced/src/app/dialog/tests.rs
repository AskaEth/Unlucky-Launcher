//! 弹窗的三条守卫：**队列真的被消费**、**运行时中文真的进了正文**、**两种失败说两句话**。
//!
//! ## 这些用例抓得住什么 / 抓不住什么（说在明处）
//!
//! | 用例 | 抓得住 | 抓不住 |
//! |---|---|---|
//! | [`queue_drives_the_modal_through_refresh_view`] | 队列里有请求 ⇒ `refresh_view` 之后 `view()` 拿到的模型里就有它（把消费点删掉即红） | 像素：`view()` 是否真的把控件塞进 iced 树（由 `DevDocs/artifacts/m187/` 的截图守） |
//! | [`runtime_chinese_reaches_the_body`] | 运行时拼出来的中文（`render` / `format!` 的 owned 串）确实在正文里 | 字形是否栅格化出来（同上，wgpu / tiny-skia 并排图） |
//! | [`two_failure_shapes_say_two_different_things`] | 没有 crash-report 时**不会**说成「有崩溃报告」，而且必须贴出捕获行 | 那句话读起来顺不顺（review） |
//!
//! 「owned 文字在 wgpu 下丢字」这条 M190 的怀疑在本 branch 上**没有复现**（A/B 图见
//! `DevDocs/artifacts/m187/ab-wgpu.png` 与 `ab-tinyskia.png`）：同一串中文走 `text_role` 与
//! `text_role_owned` 在两种后端下逐字相同。所以这里不编一条「为了变绿而改渲染路径」的断言。

use std::path::PathBuf;

use unlucky_core::launch::crash_report::{self, ArtifactFact, CrashReport};
use unlucky_i18n::catalog::Key;
use unlucky_i18n::Lang;
use unlucky_presenter::dialogs::{DialogKind, DialogQueue, DialogSource};
use unlucky_presenter::dto::{LaunchFinished, LogOutcome};

use super::{current, path_rows, CrashCard, DialogRequestView};

use crate::app::state::App;
use crate::cli::Options;
use crate::theme::Families;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

fn headless() -> App {
    use unlucky_presenter::mock::MockBackend;
    use unlucky_presenter::settings_bridge::SettingsBridge;
    App::with_presenter(
        Options::default(),
        FAMILIES,
        unlucky_design::color::SchemeFlavor::Light,
        Box::new(MockBackend::new()),
        SettingsBridge::memory_only(),
    )
}

fn report(with_crash_report: bool, tail: Vec<String>) -> CrashReport {
    let run_dir = PathBuf::from("/instances/survival");
    let artifacts = if with_crash_report {
        vec![ArtifactFact {
            source: "crash_report".to_string(),
            path: run_dir.join("crash-reports/crash-2026-11-08.txt"),
            excerpt: Some("Description: Initializing game".to_string()),
        }]
    } else {
        Vec::new()
    };
    CrashReport {
        instance: "survival".to_string(),
        name: "生存".to_string(),
        version: "1.20.4-fabric".to_string(),
        session: "9f2c".to_string(),
        session_log: Some(PathBuf::from("/sessions/game/9f2c.jsonl")),
        version_json: Some(PathBuf::from("/game/versions/1.20.4/1.20.4.json")),
        launcher_logs: Some(PathBuf::from("/logs")),
        run_dir: Some(run_dir),
        artifacts,
        exit_code: Some(1),
        exit_type: "application_error".to_string(),
        uptime_ms: 1_000,
        command: "javaw -Xmx4096m".to_string(),
        java: "javaw.exe (21)".to_string(),
        system: "os=windows".to_string(),
        stage: None,
        stage_detail: None,
        stderr_tail: tail,
    }
}

/// 守卫①：队列消费。请求进队列 ⇒ `refresh_view` 之后模型里有它；空队列 ⇒ 没有。
///
/// **变异证明**：把 `refresh_view` 末尾那行 `self.dialog_model = self.dialog_modal();` 删掉，
/// 本用例第二段立刻红（`dialog_model` 恒 `None`）。
#[test]
fn queue_drives_the_modal_through_refresh_view() {
    let mut app = headless();
    app.refresh_view();
    assert!(
        app.dialog_model.is_none(),
        "空队列不该画任何弹窗（现状：队列零引用，弹窗从不渲染）"
    );

    let title = app.lang().catalog().text(Key::CrashDialogTitleFailed);
    app.state.dialogs.push(
        DialogSource::UserAction,
        title,
        "正文一行",
        DialogKind::Confirm {
            primary: "主要".to_string(),
            secondary: "次要".to_string(),
        },
    );
    app.view_dirty = true;
    app.refresh_view();

    let modal = app.dialog_model.as_ref().expect("队列里有请求就必须有模型");
    assert_eq!(modal.title, title);
    assert_eq!(modal.primary, "主要");
    assert_eq!(modal.secondary.as_deref(), Some("次要"));
}

/// 守卫②：**运行时拼出来的中文**必须真的进正文（owned 串走 `Catalog::render` / `format!`）。
///
/// **变异证明**：把 `failure_body` 里的 `catalog.render(Key::CrashDialogExitCode, …)` 换成固定
/// 英文，本用例第一处断言立刻红。
#[test]
fn runtime_chinese_reaches_the_body() {
    let lang = Lang::ZhCn;
    let card = CrashCard {
        report: report(false, vec!["java.lang.RuntimeException: boom".to_string()]),
        expanded: true,
        dialog: 1,
        outcome: None,
    };
    let mut queue = DialogQueue::new();
    queue.push(
        DialogSource::UserAction,
        "游戏崩溃",
        "",
        DialogKind::Confirm {
            primary: String::new(),
            secondary: String::new(),
        },
    );
    let request = queue.current().expect("当前条");
    let view = DialogRequestView::of(request);
    let modal = current(lang, &view, Some(&card));
    let body = modal.body_text();

    // 退出码那句是 `render`（带参数）拼出来的：必须带中文与数字。
    assert!(body.contains("退出码 1"), "{body}");
    assert!(body.contains("游戏进程起来了"), "{body}");
    // 没有 crash-report 时必须说「没生成」+「下面是捕获的行」。
    assert!(body.contains("没有生成崩溃报告"), "{body}");
    assert!(body.contains("捕获"), "{body}");
    // 英文界面里同样的三句不许出现中文（反向守卫：i18n 不是只往一个方向写）。
    let en = current(Lang::En, &view, Some(&card));
    assert!(!en.body_text().contains('崩'), "{}", en.body_text());
}

/// 守卫③：两种失败**说两句不同的话**，而且都可以导出（事实齐全）。
#[test]
fn two_failure_shapes_say_two_different_things() {
    let lang = Lang::ZhCn;
    let with = report(true, vec!["stack line".to_string()]);
    let without = report(false, vec!["Error: could not find java.dll".to_string()]);

    assert_ne!(with.evidence(), without.evidence());
    let with_card = CrashCard {
        report: with.clone(),
        expanded: true,
        dialog: 1,
        outcome: None,
    };
    let without_card = CrashCard {
        report: without.clone(),
        expanded: true,
        dialog: 1,
        outcome: None,
    };
    let mut queue = DialogQueue::new();
    queue.push(
        DialogSource::UserAction,
        "t",
        "",
        DialogKind::Error {
            primary: "p".to_string(),
            hint: None,
        },
    );
    let request = queue.current().unwrap();
    let view = DialogRequestView::of(request);

    let crash = current(lang, &view, Some(&with_card));
    let no_log = current(lang, &view, Some(&without_card));
    assert_ne!(crash.body_text(), no_log.body_text());
    assert!(crash.body_text().contains("写出了崩溃报告"));
    assert!(!no_log.body_text().contains("写出了崩溃报告"));
    // 没有 crash-report 的那一态仍然有可展开的细节（贴的是捕获行）与四条路径。
    assert!(no_log.card.as_ref().expect("卡片").detail_title.is_some());
    assert_eq!(path_rows(&lang.catalog(), &without).len(), 4);
    assert!(path_rows(&lang.catalog(), &without)
        .iter()
        .any(|(_, path)| path.ends_with("latest.log")));
}

/// 弹窗开/关与卡片的绑定：关掉之后两样都要空（否则下一次失败会读到上一次的现场）。
#[test]
fn closing_the_dialog_clears_the_card() {
    let mut app = headless();
    crash_report::publish(report(false, vec!["line".to_string()]));
    let finished = LaunchFinished {
        id: "survival".to_string(),
        session: "9f2c".to_string(),
        outcome: LogOutcome::Crashed,
        exit_code: Some(1),
        uptime_ms: 1_000,
        failure: None,
    };
    app.open_failure_dialog(&finished);
    assert!(app.state.dialogs.current().is_some(), "失败必须弹框");
    assert!(app.dialog_card.is_some());

    app.close_dialog();
    assert!(app.state.dialogs.current().is_none());
    assert!(app.dialog_card.is_none());
}

/// 成功收尾不打扰用户。
#[test]
fn a_successful_launch_does_not_pop_anything() {
    let mut app = headless();
    crash_report::publish(report(false, Vec::new()));
    let finished = LaunchFinished {
        id: "survival".to_string(),
        session: "9f2c".to_string(),
        outcome: LogOutcome::Ok,
        exit_code: Some(0),
        uptime_ms: 90_000,
        failure: None,
    };
    app.open_failure_dialog(&finished);
    assert!(app.state.dialogs.current().is_none());
}

/// P2-1：**卡片那一层只能是卡片大**。
///
/// 抓得住什么：r1 复审实测的那个 bug —— `mouse_area(整窗居中容器)` 的命中区是整窗，
/// 卡片外任何一次点击都先被它捕获，遮罩永远收不到（「点遮罩关闭」形同虚设）。
/// `Widget::size_hint()` 不需要渲染器就能问，所以这条能在单测里跑：卡片层若声明
/// `Fill`/`Fill`，就等于「命中区 = 整窗」。
///
/// **抓不住什么**：端到端指针派发（本仓没有事件 harness，`Element` 的 `layout`/`update`
/// 需要真渲染器）。所以这条守的是**结构属性**，不是「点一下真的关了」。
#[test]
fn the_card_layer_is_card_sized_so_the_backdrop_can_be_clicked() {
    use iced::Length;

    let mut app = headless();
    app.state.dialogs.push(
        DialogSource::UserAction,
        "t",
        "b",
        DialogKind::Confirm {
            primary: "p".to_string(),
            secondary: "s".to_string(),
        },
    );
    let modal = {
        let request = app.state.dialogs.current().expect("当前条");
        let view = DialogRequestView::of(request);
        super::current(app.lang(), &view, None)
    };
    let plane = super::view::card_plane(app.ui(), &modal);
    let hint = iced::advanced::Widget::size_hint(plane.as_widget());
    // 任一维是 `Fill` 就意味着「跟随父级 = 整窗」（外层是 Fill 的居中容器），
    // 命中区于是变成整窗 —— 那正是 r1 抓到的原实现。
    assert_ne!(
        hint.width,
        Length::Fill,
        "卡片层宽度不能 Fill（会撑成整窗）"
    );
    assert_ne!(
        hint.height,
        Length::Fill,
        "卡片层高度不能 Fill（会撑成整窗）"
    );
}

/// P1-1：遮罩必须是**不透明预合成**，而且两档都**压暗**（深色档尤其）。
///
/// 抓得住 r1 实测的那个 p1：`Role::OnSurface @ 32%` 在深色档是「近白 @32%」⇒ 遮罩把整窗
/// 提亮（实测 (9,21,28) → (129,138,145)）。这里用亮度直接判方向：遮罩必须比画布暗。
///
/// 抓不住：具体色值对不对（那是令牌层的事）。
#[test]
fn the_backdrop_is_an_opaque_precomposite_and_darkens_both_flavors() {
    use unlucky_design::color::{Role, SchemeFlavor};
    use unlucky_design::elevation::MaterialLayer;
    use unlucky_design::typography::Platform;

    let luma = |color: iced::Color| 0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b;

    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let ui = crate::theme::Ui::with_platform(flavor, Platform::Windows, FAMILIES);
        // 断言**视图真正用的那一份**（不是组件层的默认值）：改回「OnSurface @32%」这里就红。
        let backdrop = super::view::backdrop_color(&ui);
        assert_eq!(
            backdrop.a, 1.0,
            "{flavor:?}：遮罩必须是不透明预合成（D27：CPU 档零半透明）"
        );

        let canvas = crate::theme::to_iced(
            unlucky_design::color::Scheme::baseline(flavor)
                .get(MaterialLayer::Canvas.surface_role()),
        );
        assert!(
            luma(backdrop) < luma(canvas),
            "{flavor:?}：遮罩必须比画布暗（当前 lane {:.3} vs 画布 {:.3}）——              用 Role::OnSurface 当遮罩会在深色档把整窗提亮",
            luma(backdrop),
            luma(canvas)
        );
        assert_eq!(Role::Surface, MaterialLayer::Canvas.surface_role());
    }
}

/// P2-4：启动那一拍的**事实**确实进了启动器日志（登记目录后一次刷出）。
///
/// 抓得住：记录函数被拆掉/改空（缓冲里就什么都没有）。
/// 抓不住：`on_boot` 有没有调它 —— 那靠这条用例旁边的调用点与 `host/launch.rs` 的 `set_dir`，
/// 真机第一次崩溃才是最终验收闸门（复审与我的一致口径）。
#[test]
fn boot_facts_are_recorded_into_the_launcher_log() {
    // 与 `app/diag.rs` 的滚动闸串行：两条用例都动同一个进程级单例目录。
    let _guard = crate::app::diag::LOG_STATE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let dir = std::env::temp_dir().join(format!("ull-m187-bootlog-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("建目录");
    unlucky_core::launch::launcher_log::set_dir(&dir);

    let app = headless();
    crate::app::update::boot_env::record_boot_log(&app);

    let path = dir.join(unlucky_core::launch::launcher_log::FILE_NAME);
    let text = std::fs::read_to_string(&path).expect("launcher.log");
    assert!(text.contains("[boot] app boot"), "{text}");
    assert!(text.contains("theme="), "{text}");
    assert!(text.contains("window="), "{text}");
}

#[path = "tests/export.rs"]
mod export;
