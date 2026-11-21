//! M349：**老版本原版 + 更高版本 Java** 的「先问一句」框（说什么 / 两档怎么分 / 三枚出口 / 取消）。
//!
//! | 用例 | 抓得住 |
//! |---|---|
//! | [`the_advisory_sentence_is_the_catalog_sentence_with_core_evidence`] | 正文退回「Java 版本不对」那类空话、over-claim（「已支持 Java 17」）、或界面自己写死证据数字（`1.12.2` / `Java 17`）——**它必须等于目录那条 key 用 core 常量渲染出来的那一句** |
//! | [`the_two_tiers_say_different_things_and_only_the_measured_one_is_soft`] | 两档写成同一句（1.7.10 也敢说「可能能启动」，或实测过的那一档反而更凶） |
//! | [`the_download_exit_is_the_primary_button_and_launch_anyway_is_secondary`] | 主按钮不是推荐动作（复审 P2-3：filled 应该是「下载 Java 8」）/ 动作跟着位置猜（点 filled 跳去设置页）/ 主次之外的第三枚丢了 |
//! | [`dismissing_the_advisory_starts_nothing_and_writes_nothing`] | Esc / 点遮罩顺手启动，或把卡片/队列留成脏状态（复审 P3-2） |
//!
//! 判据的来源：`direction` 与 `evidence` 都由 presenter 给（`CoreEvent::LaunchBlocked`，与命令行
//! 同一个 core 函数 `legacy_advisory_facts` 算出来的）。这里一个字都不重判「是不是老版本」。
//!
//! 抓不住：像素排版与「按钮有没有被切」（由 `DevDocs/artifacts/legacyadvisory/` 的浅深两档图守）。

use std::cell::RefCell;
use std::rc::Rc;

use unlucky_core::game::java::{MEASURED_LEGACY_JAVA, MEASURED_LEGACY_VERSION};
use unlucky_i18n::catalog::{Catalog, Key};
use unlucky_i18n::Lang;
use unlucky_presenter::dto::{
    AdvisoryEvidence, AdvisoryTier, AppCommand, AppEvent, Backend, CoreEvent, JavaGuardDirection,
    LaunchBlockedView,
};
use unlucky_presenter::router::Route;
use unlucky_presenter::settings_bridge::SettingsBridge;

use crate::app::state::App;
use crate::cli::Options;
use crate::theme::Families;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

/// 一条建议事实（`tier` 决定正文那一句）；`evidence` 用 core 常量填，与真实事件同源。
/// `install_major = Some(8)`：**没钉过实例 Java** 的正常那档（可一键装）。
fn advisory(version_id: &str, name: &str, actual: u32, tier: AdvisoryTier) -> LaunchBlockedView {
    LaunchBlockedView {
        id: version_id.to_string(),
        name: name.to_string(),
        version_id: version_id.to_string(),
        direction: JavaGuardDirection::Advisory(tier),
        required_min: 8,
        required_max: None,
        actual,
        install_major: Some(8),
        evidence: Some(AdvisoryEvidence {
            version_id: MEASURED_LEGACY_VERSION.to_string(),
            java: MEASURED_LEGACY_JAVA,
        }),
    }
}

/// **钉过实例 `launch.javaPath`** 的那一档（复审 r2 P2-1）：装机出口落地不了 ⇒ `None`。
fn pinned_advisory(actual: u32) -> LaunchBlockedView {
    LaunchBlockedView {
        install_major: None,
        ..advisory("1.7.10", "怀旧服 1.7.10", actual, AdvisoryTier::Unmeasured)
    }
}

/// 目录里那句建议（用 core 给的数字渲染）——**界面与命令行都该说出这一句**。
fn expected_sentence(lang: Lang, key: Key, view: &LaunchBlockedView) -> String {
    let evidence = view.evidence.as_ref().expect("建议必须带证据");
    let min = view.required_min.to_string();
    let actual = view.actual.to_string();
    let measured_java = evidence.java.to_string();
    Catalog::of(lang).render(
        key,
        &[
            ("version", &view.version_id),
            ("min", &min),
            ("actual", &actual),
            ("measured_version", &evidence.version_id),
            ("measured_java", &measured_java),
        ],
    )
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

fn recording() -> (App, Rc<RefCell<Vec<AppCommand>>>) {
    let dispatched = Rc::new(RefCell::new(Vec::new()));
    let backend = RecorderBackend {
        dispatched: Rc::clone(&dispatched),
    };
    let mut app = App::with_presenter(
        Options::default(),
        FAMILIES,
        unlucky_design::color::SchemeFlavor::Light,
        Box::new(backend),
        SettingsBridge::memory_only(),
    );
    app.pin_language(Lang::ZhCn);
    (app, dispatched)
}

/// 把一条建议喂进产品路径（`AppState::apply` + `refresh_view`，与真 host 同一条）。
fn deliver(app: &mut App, view: &LaunchBlockedView) {
    app.state
        .apply(AppEvent::Core(CoreEvent::LaunchBlocked(Box::new(
            view.clone(),
        ))));
    app.refresh_view();
}

/// 当前这一屏（走产品函数 `dialog_modal`）。
fn shown(app: &mut App) -> crate::app::dialog::Modal {
    app.dialog_modal().expect("建议事件到达后必须有一屏")
}

/// 判据①：正文**就是目录那一句**（用 core 的证据数字渲染），且不许 over-claim。
/// 界面自己拼一句（或把证据数字写死）⇒ 第一条 `contains` 红。
#[test]
fn the_advisory_sentence_is_the_catalog_sentence_with_core_evidence() {
    let view = advisory("1.7.10", "怀旧服 1.7.10", 21, AdvisoryTier::Unmeasured);
    let (mut app, _recorded) = recording();
    deliver(&mut app, &view);
    let modal = shown(&mut app);
    let catalog = Catalog::of(Lang::ZhCn);

    assert_eq!(
        modal.title,
        catalog.text(Key::JavaAdvisoryTitle),
        "标题必须说「这个老版本是按更老的 Java 设计的」，不是「需要另一个 Java」"
    );
    let body = modal.body_text();
    let sentence = expected_sentence(Lang::ZhCn, Key::JavaAdvisoryUnmeasured, &view);
    assert!(
        body.contains(&sentence),
        "正文必须是目录那条 key 用 core 的证据渲染出来的那一句：\n期望：{sentence}\n实际：{body}"
    );
    // 证据来自 core 的常量（不是界面写死的字面）。
    assert!(
        sentence.contains(MEASURED_LEGACY_VERSION),
        "证据版本要写出来：{sentence}"
    );
    assert!(
        sentence.contains(&format!("Java {MEASURED_LEGACY_JAVA}")),
        "证据里的 Java 主版本要写出来：{sentence}"
    );
    assert!(body.contains("1.7.10"), "要点名版本：{body}");
    assert!(
        body.contains("Java 8"),
        "要说清该版本设计上是 Java 8：{body}"
    );
    assert!(body.contains("Java 21"), "要说清现在用的是 21：{body}");
    // 过头话不许出现（`jvm` 那套崩溃规则的存在本身就说明这类组合真会崩）。
    for wrong in ["已支持", "完美兼容", "可以放心"] {
        assert!(!body.contains(wrong), "过头话「{wrong}」：{body}");
    }
    // 内部 code 不上屏。
    assert!(!body.contains("unmeasured"), "{body}");
    assert!(!body.contains("Advisory"), "{body}");

    // 英文档：同一批 key 的英文那份，而且**不是**中文原文（漏译会被这条抓住）。
    let en = expected_sentence(Lang::En, Key::JavaAdvisoryUnmeasured, &view);
    assert!(en.contains("never been measured"), "{en}");
    assert!(en.contains("Java 8") && en.contains("Java 21"), "{en}");
    assert!(!en.contains('原'), "英文档里漏了中文：{en}");
}

/// 判据②：两档**说两句不同的话**，而且只有实测过的那一档敢说「可能能启动」。
#[test]
fn the_two_tiers_say_different_things_and_only_the_measured_one_is_soft() {
    let measured_view = advisory("1.12.2", "生存服 1.12.2", 17, AdvisoryTier::Measured);
    let unmeasured_view = advisory("1.7.10", "怀旧服 1.7.10", 21, AdvisoryTier::Unmeasured);

    let body_of = |view: &LaunchBlockedView| {
        let (mut app, _recorded) = recording();
        deliver(&mut app, view);
        shown(&mut app).body_text()
    };
    let measured = body_of(&measured_view);
    let unmeasured = body_of(&unmeasured_view);

    assert_ne!(measured, unmeasured, "两档必须说两句不同的话");
    assert!(measured.contains("可能能启动"), "{measured}");
    assert!(
        !unmeasured.contains("可能能启动"),
        "没测过的组合不许发安慰：{unmeasured}"
    );
    // 实测那一档也要把证据强度写出来（到主菜单 ≠ 能玩）。
    assert!(measured.contains("进世界未验证"), "{measured}");
    for body in [&measured, &unmeasured] {
        assert!(body.contains("Java 8"), "{body}");
        assert!(body.contains(MEASURED_LEGACY_VERSION), "{body}");
    }
}

/// 判据③：主按钮 = **推荐动作**（「下载 Java 8」filled），次按钮 = 仍要启动，第三枚 = 去设置。
///
/// 抓得住（复审 P2-3）：`filled` 换回「去设置改 Java」⇒ 第一条红；动作按**位置**猜（filled 派
/// `Primary`）⇒ 第三段红（那里断言点 filled 派的是 `InstallJavaFor`，不是换路由）。
#[test]
fn the_download_exit_is_the_primary_button_and_launch_anyway_is_secondary() {
    let catalog = Catalog::of(Lang::ZhCn);
    let view = advisory("1.7.10", "怀旧服 1.7.10", 21, AdvisoryTier::Unmeasured);
    let (mut app, recorded) = recording();
    deliver(&mut app, &view);
    let modal = shown(&mut app);

    let download = catalog.render(Key::JavaGuardDownload, &[("major", "8")]);
    assert_eq!(
        modal.primary, download,
        "filled 主按钮必须是「下载 Java 8」"
    );
    assert_eq!(
        modal.secondary.as_deref(),
        Some(catalog.text(Key::JavaGuardLaunchAnyway)),
        "次按钮（text）仍是「仍要启动」"
    );
    assert_eq!(
        modal.tertiary.as_deref(),
        Some(catalog.text(Key::JavaGuardOpenSettings)),
        "第三枚（outlined）是「去设置改 Java」"
    );
    // 建议框不是崩溃卡片：没有证据区可给。
    assert!(modal.card.is_none(), "启动前没有崩溃现场");

    // 动作跟着按钮走：filled 那格挂的是 `Tertiary`（装机），outlined 那格挂的是 `Primary`（设置页）。
    // 断言这两格，是防「按位置猜动作」——猜的话 filled 会跳去设置页（下面两段就红）。
    assert_eq!(
        modal.primary_action,
        Some(crate::app::dialog::Message::Tertiary),
        "filled 那枚必须挂「装机/装好重试」这条动作"
    );
    assert_eq!(
        modal.tertiary_action,
        Some(crate::app::dialog::Message::Primary),
        "outlined 那枚（去设置改 Java）挂的是 Primary"
    );

    // 点 filled 那一枚 = 装 Java 8（**不是**跳设置页）。
    let filled = modal.primary_action.clone().expect("filled 的动作");
    let _ = app.update(crate::app::Message::Dialog(filled));
    assert_eq!(
        *recorded.borrow(),
        vec![AppCommand::InstallJavaFor { major: 8 }],
        "filled 那枚必须派「装 Java 8」，实际：{:?}",
        recorded.borrow()
    );
    assert_ne!(app.state.route(), &Route::Settings, "不该顺带跳设置页");

    // 点 outlined 那一枚 = 去设置改 Java（不启动、不装机）。
    let (mut app, recorded) = recording();
    deliver(&mut app, &view);
    let outlined = shown(&mut app)
        .tertiary_action
        .clone()
        .expect("outlined 的动作");
    let _ = app.update(crate::app::Message::Dialog(outlined));
    assert_eq!(app.state.route(), &Route::Settings, "第三枚要跳到设置页");
    assert!(
        recorded.borrow().is_empty(),
        "去设置这条路不该顺带启动/装机：{:?}",
        recorded.borrow()
    );

    // 次按钮 = 仍要启动（实例 id 取自建议事实，不靠上下文猜）。
    let (mut app, recorded) = recording();
    deliver(&mut app, &view);
    let _ = app.update(crate::app::Message::Dialog(
        crate::app::dialog::Message::Secondary,
    ));
    assert_eq!(
        *recorded.borrow(),
        vec![AppCommand::LaunchInstanceAnyway("1.7.10".to_string())],
        "「仍要启动」必须派一条放行命令"
    );
    assert!(app.state.dialogs.is_empty(), "点了之后框要关掉");
}

/// 判据⑤（复审 r2 P2-1 **必修**）：**钉过实例 Java** 那一档，留下的两枚出口都能落地。
///
/// 抓得住：① 又画回装机出口（注定被影子掉）⇒ 前两条断言红；② 第三枚改回 `Primary`（全局设置页）
/// ⇒ `Route::Detail` 那条断言红——钉过的实例改全局设置**不生效**；③ 主按钮留装机 ⇒ 放行断言红。
#[test]
fn the_pinned_advisory_has_no_dead_ends() {
    let catalog = Catalog::of(Lang::ZhCn);
    let view = pinned_advisory(21);
    let (mut app, recorded) = recording();
    deliver(&mut app, &view);
    let modal = shown(&mut app);

    // ① 正文说清「为什么没有一键装」——用**探测到的**真实主版本。
    let pinned_line = catalog.render(Key::JavaAdvisoryPinned, &[("java", "21")]);
    assert!(
        modal.body_text().contains(&pinned_line),
        "钉过实例 Java 时必须说清这条实例钉了 Java 21：{pinned_line} / 实际：{}",
        modal.body_text()
    );
    // ② 不给装机出口：没有那枚「下载 Java 8」。
    let download = catalog.render(Key::JavaGuardDownload, &[("major", "8")]);
    assert_ne!(modal.primary, download, "钉过实例 Java ⇒ 不许画装机出口");
    assert_eq!(
        modal.primary,
        catalog.text(Key::JavaGuardLaunchAnyway),
        "主按钮改成「仍要启动」"
    );
    assert_eq!(
        modal.primary_action,
        Some(crate::app::dialog::Message::Secondary),
        "主按钮的动作必须是「仍要启动」，不是装机"
    );
    assert_eq!(
        modal.tertiary.as_deref(),
        Some(catalog.text(Key::JavaGuardInstanceArgs)),
        "第三枚指向**实例**的启动参数"
    );
    assert_eq!(
        modal.tertiary_action,
        Some(crate::app::dialog::Message::InstanceLaunchArgs),
        "第三枚的动作必须是「去实例启动参数」（不是 Primary = 全局设置页）"
    );
    assert!(modal.secondary.is_none(), "这一档只留两枚，不留空的视觉位");

    // ③ 两枚都能落地：filled = 这一次放行；outlined = 跳到**这条实例**的详情页。
    let filled = modal.primary_action.clone().expect("filled 的动作");
    let _ = app.update(crate::app::Message::Dialog(filled));
    assert_eq!(
        *recorded.borrow(),
        vec![AppCommand::LaunchInstanceAnyway("1.7.10".to_string())],
        "「仍要启动」必须派一条放行命令"
    );
    assert!(app.state.dialogs.is_empty(), "点了之后框要关掉");

    let (mut app, recorded) = recording();
    deliver(&mut app, &view);
    let outlined = shown(&mut app)
        .tertiary_action
        .clone()
        .expect("outlined 的动作");
    let _ = app.update(crate::app::Message::Dialog(outlined));
    assert_eq!(
        app.state.route(),
        &Route::Detail("1.7.10".to_string()),
        "第三枚要去**这条实例**的启动参数（详情页），不是全局设置"
    );
    assert_ne!(
        app.state.route(),
        &Route::Settings,
        "钉过的实例改全局设置无效"
    );
    assert!(
        !recorded.borrow().iter().any(|command| matches!(
            command,
            AppCommand::LaunchInstance(_)
                | AppCommand::LaunchInstanceAnyway(_)
                | AppCommand::InstallJavaFor { .. }
        )),
        "去实例参数这条路不该顺带启动/装机：{:?}",
        recorded.borrow()
    );
}

/// 判据④（复审 P3-2）：**取消就是取消**——不启动、不装机、不写设置，卡片与队列都干净。
///
/// 抓得住：`Dismiss` 顺手派 `LaunchInstance`，或关掉之后 `java_guard` 还挂着（旧卡片会被认到新框）。
#[test]
fn dismissing_the_advisory_starts_nothing_and_writes_nothing() {
    let view = advisory("1.7.10", "怀旧服 1.7.10", 21, AdvisoryTier::Unmeasured);
    let (mut app, recorded) = recording();
    deliver(&mut app, &view);
    assert!(app.guard_for_current().is_some(), "前置：框与卡片都该在");

    let _ = app.update(crate::app::Message::Dialog(
        crate::app::dialog::Message::Dismiss,
    ));

    assert!(
        recorded.borrow().is_empty(),
        "取消不许派任何命令（启动/装机都算）：{:?}",
        recorded.borrow()
    );
    assert!(app.state.dialogs.is_empty(), "队列要清空");
    assert!(
        app.java_guard.is_none(),
        "建议卡片要跟着框一起清（否则下一轮会把旧卡片认到新框上）"
    );
    assert!(app.dialog_model.is_none(), "这一屏不该再画框");
    assert_ne!(app.state.route(), &Route::Settings, "取消不是「去设置」");
}
