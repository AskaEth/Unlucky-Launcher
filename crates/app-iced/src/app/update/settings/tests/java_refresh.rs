//! M204：Java 组**写盘成功之后请 host 重发设置快照**这一拍（`JAVA_REFRESH_ACTION`）。
//!
//! ## 为什么需要这一条（缺口是什么）
//!
//! Java 组的三键（`java.default_memory_mb` / `java.extra_args` / `java.selected_path`）走的是页面
//! 自己的 `SettingStore`（`pages/settings/java/store.rs`：`set` 几个键 + **一次** `save()`）——
//! 那是**同步、原子**的一次落盘，但**不经过** `AppCommand::SetSetting` 的写盘分支。而
//! `CoreEvent::SettingsChanged` 全仓只有两处发出：`host/dispatch.rs` 的 `SetSetting` 分支与
//! `host/mod.rs` 的 `refresh_all`。于是三键写成功之后，`AppState::settings_groups` 里 java 组那几行
//! 会**停在旧值**，直到下一次任何别的写入（改并发、改主题……）顺手带一份新快照回来。
//!
//! M204 的修法：落盘成功之后派发一条动作键（`dto::JAVA_REFRESH_ACTION`，`host::settings::set`
//! 见到它只返回 `Ok(())`，不写盘），让 `SetSetting` 分支照常重发 `SettingsChanged`。**没有**改用
//! `RefreshAll`：那个还会 `versions::spawn_list` 与 `start_update_check`，每点一次保存打两个网络请求。
//!
//! ## 两条用例证明的事不同
//!
//! | 用例 | 后端 | 证明 |
//! |---|---|---|
//! | 第一条 | **真 `CoreHost` + 临时数据根** | 写盘 ⇒ 快照真的变了（磁盘 + 快照两头的字节级证据） |
//! | 第二条 | 假后端（记账）+ 本地校验拒绝 | **被拒的保存不许请求刷新**（那一拍只该跟在真落盘之后） |

use std::cell::RefCell;
use std::rc::Rc;

use unlucky_design::color::SchemeFlavor;
use unlucky_presenter::dto::{find_row, AppCommand, AppEvent, Backend, SettingGroupView};
use unlucky_presenter::settings_bridge::SettingsBridge;

use crate::cli::Options;
use crate::pages;
use crate::theme::Families;

use super::*;

/// 本组要断言的键（与 presenter 快照 / schema 同名）。
const JVM: &str = "java.extra_args";

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

/// 一条 Java 分组消息 → 页面消息（与 `tests/java.rs` 同一个漏斗）。
fn java(message: pages::settings::java::Message) -> Message {
    Message::Page(pages::Message::Settings(
        pages::settings::GroupMessage::Java(message),
    ))
}

/// 快照里某个键的**上屏文本**（`None` = 快照里没这一行）。
///
/// 用 `value_text` 而不是控件序号：Java 组这三个键里 `extra_args` / `selected_path` 是 text 行，
/// `value_text` 就是文件里的原文；内存那一行是四档 choice（见报告「没做/没验」第 ③ 条），
/// 它的**当前值**表达在 `options[selected]` 上，所以本文件不对内存行做取值断言——那会把
/// 「presenter 的四档表达不了 6144」这件事伪装成绿。
fn row_value(groups: &[SettingGroupView], key: &str) -> Option<String> {
    find_row(groups, key).map(|row| row.value_text.clone())
}

/// **主用例**：改内存 + JVM 参数 ⇒ 点保存 ⇒ 磁盘变了 **且** host 发回来的快照也变了。
///
/// 链子全长（与本组的产品路径逐段对应）：`Message::EditMemory/EditJvm` → `State` 草稿 →
/// `Message::Save` → `store::save`（一次 `store.save()` 写两个键）→ `request_settings_refresh()`
/// → 真 `CoreHost` 的 `SetSetting` 分支（`settings::set` 认动作键、不写盘）→
/// `CoreEvent::SettingsChanged(settings::groups(env))` → `AppState::settings_groups` →
/// `pages::Model::settings`。断言落在**磁盘字节**与**快照文本**上，不落在任何中间变量上。
///
/// 抓得住：
/// 1. 摘掉 `request_settings_refresh()` 的 dispatch（`save_java_settings` 里那一行）⇒ 快照仍是
///    空串 ⇒ 红（**这就是本用例的变异验证**，且是本文件第二条都不红的唯一原因）；
/// 2. 写盘没真发生（`store::save` 的 `save()` 被删）⇒ 磁盘断言红；
/// 3. 动作键在 presenter 侧被当成真键（`host::settings::set` 少了那条早退）⇒ `store.set` 报
///    「未知键」⇒ 页面拿到 error Toast、快照不更新 ⇒ 红；
/// 4. 快照回流被丢掉（`view::model` 的 `settings:` 那行被删）⇒ `app.view.settings` 空 ⇒ 红。
///
/// 抓不住：真实鼠标点击（本用例从 `Message` 起跑）；「页面上的值是不是旧值」这一半——写盘成功后
/// 页面本来就该显示新值，旧值那一半由 `tests/java.rs` 的拒绝用例守（本地校验不过时文件一个字节
/// 不动、回执是具名原因）。
#[test]
fn saving_the_java_form_refreshes_the_host_settings_snapshot() {
    let dir = std::env::temp_dir().join(format!("ull-m204-java-refresh-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".ULL/config")).expect("config 目录");
    let config = dir.join(".ULL/config/launcher.json");

    let env = unlucky_presenter::host::HostEnv::at_root(&dir);
    let mut app = App::with_presenter(
        Options {
            data_dir: Some(dir.clone()),
            ..Options::default()
        },
        FAMILIES,
        SchemeFlavor::Light,
        Box::new(unlucky_presenter::host::CoreHost::new(env)),
        // 设置桥也指向同一个根：不碰用户的真配置。
        SettingsBridge::with_path(unlucky_presenter::backend::config_path(&dir)),
    );
    // 首屏：产品路径在 `on_boot` 里派的就是这一条（真 host 会发 `SettingsChanged`）。
    app.dispatch(AppCommand::RefreshAll);
    assert_eq!(
        row_value(app.settings_snapshot(), JVM).as_deref(),
        Some(""),
        "临时根里没有 `java.extra_args` ⇒ 快照这一行是空串（出厂值就是没有额外参数）"
    );

    let _ = app.update(java(pages::settings::java::Message::EditMemory(
        "6144".to_string(),
    )));
    let _ = app.update(java(pages::settings::java::Message::EditJvm(
        "-XX:+UseG1GC".to_string(),
    )));
    let _ = app.update(java(pages::settings::java::Message::Save));

    let written = std::fs::read_to_string(&config).expect("写回之后 launcher.json 必须存在");
    assert!(
        written.contains("\"default_memory_mb\": 6144"),
        "磁盘上必须真的有 6144：{written}"
    );
    assert!(
        written.contains("-XX:+UseG1GC"),
        "磁盘上必须真的有那段 JVM 参数：{written}"
    );

    assert_eq!(
        row_value(app.settings_snapshot(), JVM).as_deref(),
        Some("-XX:+UseG1GC"),
        "host 必须为刚写进去的三键重发一次快照（否则 AppState 里这一行还是旧值）"
    );
    assert_eq!(
        row_value(&app.view.settings, JVM).as_deref(),
        Some("-XX:+UseG1GC"),
        "页面模型也得跟着换：下一帧画的是 `Model.settings`"
    );

    std::fs::remove_dir_all(&dir).ok();
}

// --------------------------------------------------------------- 被拒的保存不许请求刷新

/// 只记账、不回事件的后端（本用例只关心「有没有发出那条动作键」）。
struct Recorder(Rc<RefCell<Vec<AppCommand>>>);

impl Backend for Recorder {
    fn drain_events(&mut self) -> Vec<AppEvent> {
        Vec::new()
    }

    fn dispatch(&mut self, command: AppCommand) {
        self.0.borrow_mut().push(command);
    }
}

/// 本地校验把保存拦下来时，**一次刷新都不许请求**：那一拍只该跟在真落盘之后。
///
/// 抓得住：把 `request_settings_refresh()` 挪到 `validate()` 之前（或无条件调用）⇒ 这里能看见
/// 动作键 ⇒ 红。抓不住：写盘失败的回执文案（那是 `tests/java.rs` 的 `write_notice` 用例）。
#[test]
fn a_refused_save_does_not_ask_for_a_refresh() {
    use pages::settings::java::Notice;

    let commands = Rc::new(RefCell::new(Vec::new()));
    let dir = std::env::temp_dir().join(format!("ull-m204-java-refuse-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("config")).expect("config 目录");
    let path = dir.join("config/launcher.json");

    let mut app = App::with_presenter(
        Options::default(),
        FAMILIES,
        SchemeFlavor::Light,
        Box::new(Recorder(Rc::clone(&commands))),
        SettingsBridge::with_path(&path),
    );
    // 128 MB 低于 schema 的下限（512）：本地校验就该拦下，不写盘。
    let _ = app.update(java(pages::settings::java::Message::EditMemory(
        "128".to_string(),
    )));
    let _ = app.update(java(pages::settings::java::Message::Save));

    // 越界的内存要在本地就被拒（具名原因 = schema 下限 512 那条文案，不是通用的「写不进去」）。
    let notice = app
        .settings
        .java
        .notice
        .clone()
        .expect("被拒的保存必须留下一条回执");
    let Notice::Refused(reason) = notice else {
        panic!("越界的内存必须是具名的本地拒绝：{notice:?}");
    };
    assert_eq!(
        reason.label(),
        unlucky_i18n::Key::SettingsJavaReasonMemoryRange,
        "拒绝原因要落到「超出允许范围」那条文案上"
    );
    assert!(!path.exists(), "被拒的保存不许碰磁盘");
    assert!(
        !commands.borrow().iter().any(|command| matches!(
            command,
            AppCommand::SetSetting { key, .. }
                if key == unlucky_presenter::dto::JAVA_REFRESH_ACTION
        )),
        "没落盘就没有可刷新的事实（实际下发：{:?}）",
        commands.borrow()
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// 真的落盘时，动作键**必须**发出去（正面那一半；与第一条互为背书）。
///
/// 为什么还要这一条：第一条用真 `CoreHost`，它的失败可能是「快照没回来」的任意一环；这一条把
/// 「到底有没有派出那条动作键」单独钉住，红的行号不同，排查时不必再猜是哪一环。
///
/// 抓得住：`request_settings_refresh()` 的键名写错（比如漏了 `action` 段）⇒ presenter 会把它当
/// 真键报「未知键」，而这条用例在本层就能看见键名对不对。
/// 抓不住：presenter 收到动作键之后有没有真的重发快照（那是第一条的事）。
#[test]
fn a_successful_save_dispatches_the_refresh_action_key() {
    let commands = Rc::new(RefCell::new(Vec::new()));
    let dir = std::env::temp_dir().join(format!("ull-m204-java-dispatch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("config")).expect("config 目录");
    let path = dir.join("config/launcher.json");

    let mut app = App::with_presenter(
        Options::default(),
        FAMILIES,
        SchemeFlavor::Light,
        Box::new(Recorder(Rc::clone(&commands))),
        SettingsBridge::with_path(&path),
    );
    let _ = app.update(java(pages::settings::java::Message::EditJvm(
        "-XX:+UseG1GC".to_string(),
    )));
    let _ = app.update(java(pages::settings::java::Message::Save));

    assert!(path.exists(), "这一条的前提是保存真的落盘了（临时根可写）");
    assert!(
        commands.borrow().iter().any(|command| matches!(
            command,
            AppCommand::SetSetting { key, .. }
                if key == unlucky_presenter::dto::JAVA_REFRESH_ACTION
        )),
        "落盘之后必须派发动作键（实际下发：{:?}）",
        commands.borrow()
    );

    std::fs::remove_dir_all(&dir).ok();
}

// --------------------------------------------------------------- 「立即扫描」真的接线

/// 「立即扫描」在 App 层是**真接线**的：`Message::Scan` ⇒ 起一次真扫盘（不是只画了颗按钮）。
///
/// 为什么这一条住在本文件（而不是 `tests/java.rs`）：那个文件 374 行、`tests.rs` 正好 400 行，
/// 再开一个模块文件就要往 `tests.rs` 加一行 `mod`（400 → 401，撞 `gate` 的 400 行红线）。
/// 主题上也勉强算同一族：两者都是「设置页那一组对 App 做了什么」的守卫（这条守扫描入口，
/// 上面几条守写回闭环）。
///
/// 抓得住：`on_settings_java` 的 `Message::Scan => return self.start_java_scan()` 被改成
/// `Task::none()`（只画按钮不接线 —— 用户在早期投诉里点名的那一条）⇒ `report().running`
/// 仍是 `false`、`scanning` 仍是 `None` ⇒ 红（实测见报告「变异验证」M3）。
/// 抓不住：扫描**结论**是否写对了（那是 `pages/settings/java/tests/runtime.rs` 的事），
/// 也抓不住真实鼠标点击（本用例从 `Message` 起跑）。
#[test]
fn pressing_scan_starts_a_real_scan() {
    // 用截图档构造（`Options.screenshot = Some(..)`）**只为一个目的**：让首屏那次
    // `auto_scan_java_if_due` 不发生（`hydrate_java_settings` 的截图分支灌种子后直接 return），
    // 于是「开局没有扫描在跑」这个前提是确定的。手动点「立即扫描」走的是**同一条产品代码**
    // （`start_java_scan` → `ScanSlot::start`），截图档不拦它。
    let dir = std::env::temp_dir().join(format!("ull-m204-java-scan-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("config")).expect("config 目录");
    let path = dir.join("config/launcher.json");
    let mut app = App::with_presenter(
        Options {
            screenshot: Some(std::path::PathBuf::from("m204-scan.png")),
            ..Options::default()
        },
        FAMILIES,
        SchemeFlavor::Light,
        Box::new(Recorder(Rc::new(RefCell::new(Vec::new())))),
        SettingsBridge::with_path(&path),
    );
    assert!(
        !app.settings.java.scan.report().running,
        "这一条的前提：构造时没有扫描在跑"
    );

    let _ = app.update(java(pages::settings::java::Message::Scan));

    let report = app.settings.java.scan.report();
    assert!(
        report.running,
        "「立即扫描」必须真的起一条扫描线程：{report:?}"
    );
    assert!(
        app.settings.java.scanning.is_some(),
        "状态行要能区分「正在扫描」（`scanning` 不能还是 None）"
    );

    // 别把这条用例起的真扫盘留给后面的用例：置取消位，线程会在下一个候选边界退出。
    app.settings.java.scan.cancel();
    std::fs::remove_dir_all(&dir).ok();
}
