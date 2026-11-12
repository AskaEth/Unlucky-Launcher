//! M175 的 App 层用例：**「设为默认」不许吃掉未保存的内存 / JVM 草稿**（r1 复审 P2-1）。
//!
//! 为什么单独一个文件：`tests.rs` 加上它之后 446 行，越过 400 行硬线
//! （`app/components/gate.rs` 的 `every_source_file_stays_under_the_line_limit` 会判红）。

use super::bridged_app;
use crate::app::Message;
use crate::pages;

// --------------------------------------------------------------- M175 r1 P2-1：设为默认不许吃草稿

/// 一条 Java 分组消息 → 页面消息。
fn java(message: pages::settings::java::Message) -> Message {
    Message::Page(pages::Message::Settings(
        pages::settings::GroupMessage::Java(message),
    ))
}

/// 改内存 / JVM 草稿（还没保存）之后去点「设为默认」，**草稿必须还在**，而且仍算未保存。
///
/// 复审 r1 P2-1 的现场：旧代码在 `select_java_default` 里先 `dirty = false` 再 `hydrate`，
/// 于是只写了 `java.selected_path` 的一次操作把用户没保存的内存 / JVM 参数一起冲掉，
/// 一个字节都没落盘、也没有任何提示。
///
/// 变异证明：把 `dirty = false` 那一行加回 `select_java_default` ⇒ 这条红（内存回到 2048、
/// JVM 回到空、`dirty` 变假）。
#[test]
fn setting_the_default_jdk_keeps_unsaved_memory_and_jvm_drafts() {
    let (mut app, path) = bridged_app("java-select");

    let _ = app.update(java(pages::settings::java::Message::EditMemory(
        "6144".to_string(),
    )));
    let _ = app.update(java(pages::settings::java::Message::EditJvm(
        "-XX:+UseG1GC".to_string(),
    )));
    assert!(app.settings.java.dirty, "改过草稿就该是脏的");
    assert_eq!(app.settings.java.saved_memory, 2048, "落盘真值还没变");

    let _ = app.update(java(pages::settings::java::Message::SelectRuntime(
        "C:/jdk-21/bin/java.exe".to_string(),
    )));

    assert_eq!(
        app.settings.java.selected, "C:/jdk-21/bin/java.exe",
        "设为默认必须真的写进去"
    );
    assert_eq!(
        app.settings.java.memory, "6144",
        "内存草稿被「设为默认」清掉了（P2-1 的数据丢失）"
    );
    assert_eq!(
        app.settings.java.jvm, "-XX:+UseG1GC",
        "JVM 草稿被「设为默认」清掉了（P2-1 的数据丢失）"
    );
    assert!(
        app.settings.java.dirty,
        "内存 / JVM 参数还没保存，不许标成已保存"
    );
    assert_eq!(app.settings.java.saved_memory, 2048, "落盘真值仍然没变");
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
}

// --------------------------------------------------------------- r3 P3-3：首屏自动扫必须有落脚点

/// **首屏自动扫一次盘**这件事必须在测试里有落脚点（r3 复审 P3-3）。
///
/// 现场：`hydrate_java_settings` 末尾那次 `if !cfg!(test) { auto_scan_java_if_due(...) }` 整行删掉、
/// 或把实参翻成 `true`（截图抑制档），全套测试仍是绿的——于是「设置页从不自动检测本机 Java」
/// 这个**用户实测的头号抱怨**回来时没人拦。
///
/// 为什么这条不是「再把判据测一遍」：判据（`auto_scan_due` 的四条）与取消语义已经有用例；这里要
/// 证明的是**首屏这条路真的调了它**。所以走的是生产入口（`construct.rs` 构造时与心跳都调
/// `hydrate_java_settings`）与生产作业（真扫盘），没有 `cfg!(test)` 分叉、也没有注入假作业。
///
/// 判据为什么同步可判、且与线程快慢无关：`ScanSlot::start_with` 在起线程**之前**就把 `running`
/// 置真（同一把锁里）；万一扫描线程已经跑完，`finished` 会留着。所以「真的起过一次扫描」= 这两个
/// 位里至少有一个（空槽两者都是假）——不用等线程，也不看这台机器扫到了什么。
///
/// 变异证明：①删掉 `self.auto_scan_java_if_due(false);` 这一行 ⇒ 两位仍是假 ⇒ 红；
/// ②把它改成 `self.auto_scan_java_if_due(true);` ⇒ 被 `auto_scan_due` 的 `!screenshot` 挡掉 ⇒ 红。
#[test]
fn the_first_hydrate_starts_a_real_scan_by_itself() {
    let (mut app, path) = bridged_app("java-autoscan");
    // 构造那一刻已经自动扫过一次（同一个入口）；这里要重新演一遍「首屏」，所以换一个空槽。
    app.settings.java.scan = pages::settings::java::store::ScanSlot::default();

    app.hydrate_java_settings();

    let report = app.settings.java.scan.report();
    assert!(
        report.running || report.finished,
        "首屏 hydrate 没有自动发起扫描（快照是空的 {report:?}）：设置页就不会自己检测本机 Java\
         （r3 P3-3 的回归）"
    );
    // 收尾：取消这次真扫盘，别让它在后台继续探测本机 PATH 上的 java。
    app.settings.java.scan.cancel();
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
}

// --------------------------------------------------------------- r2 P2-2：取消之后不许自己重新起来

static HOLD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// 一个会一直卡住的扫描作业（直到被取消）。
fn held_job(
    _candidates: &[unlucky_core::game::java::staged::Candidate],
    cancelled: &dyn Fn() -> bool,
    _progress: &mut dyn FnMut(usize, usize),
) -> Vec<pages::settings::java::RuntimeRow> {
    while !HOLD.load(std::sync::atomic::Ordering::SeqCst) && !cancelled() {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    Vec::new()
}

/// 取消过一次之后，**配置文件再变（心跳重读）也不会把扫描自己拉起来**，状态行仍是「已取消」。
///
/// 现场（r2 复审 P2-2）：被取消的扫描 `running=false` / `cancelled=true` / `finished` 仍是
/// `false`，而 `auto_scan_due` 不看 `cancelled` ⇒ `clock.rs` 的 `reload_if_changed` 每变一次
/// `launcher.json`（存内存、改主题、改语言都算）就重新起一个扫描线程，用户的取消被无声撤销。
///
/// 变异证明：把 `auto_scan_due` 里的 `!report.cancelled` 去掉 ⇒ 这里 `auto_scan_java_if_due`
/// 会真的起第二个线程，`scanning` 变 `Some`、状态行不再是「已取消」⇒ 本条红。
#[test]
fn a_cancelled_scan_is_not_started_again_by_a_config_reload() {
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    HOLD.store(false, Ordering::SeqCst);
    let (mut app, path) = bridged_app("java-cancel");
    // 构造时已经自动扫过一次（首屏那条生产入口，见上一条用例）；换成空槽，让本用例自己起。
    app.settings.java.scan = pages::settings::java::store::ScanSlot::default();

    // 真起一次扫描（注入卡住的作业），再取消，等线程退出：report 变成
    // cancelled=true / finished=false —— 正是复审指认的那个形状。
    let slot = app.settings.java.scan.clone();
    assert!(slot.start_with(Vec::new(), Vec::new(), false, held_job));
    slot.cancel();
    let deadline = Instant::now() + Duration::from_secs(5);
    while slot.report().running && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!slot.report().running, "取消之后线程必须退出");
    assert!(slot.report().cancelled);
    assert!(!slot.report().finished);

    // 心跳那条路：把状态灌一次（`hydrate_java_scan`），再走「该自动扫就扫」的判据。
    app.hydrate_java_scan();
    assert!(app.settings.java.cancelled, "状态行要留着「已取消」");
    assert!(app.settings.java.scanning.is_none());

    // 心跳那条路：走**生产入口**（`clock.rs` 每发现配置文件变了就调 `hydrate_java_settings`），
    // 它末尾那次自动发起必须被「取消过」这一条挡住。
    app.hydrate_java_settings();
    assert!(
        !slot.report().running,
        "取消过的扫描不许被自动重新拉起（P2-2：用户取消被无声撤销）：{:?}",
        slot.report()
    );
    app.hydrate_java_scan();
    assert!(
        app.settings.java.scanning.is_none(),
        "状态行不许回到「正在扫描」"
    );
    assert!(
        app.settings.java.cancelled,
        "取消标记要保持到下一次手动扫描为止"
    );

    HOLD.store(true, Ordering::SeqCst);
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
}

// --------------------------------------------------------------- r2 P3-3：core 的中文串不许直出

/// 写盘被 schema 拒时，界面拿到的是**页面自己的 key**，而不是 `StoreError` 的中文 `Display`。
///
/// 现场（r2 复审 P3-3）：`WriteError::Schema(format!("{error}"))` 里 `core::setting::StoreError`
/// 的 `Display` 是中文（`IO 失败 @ …` / `文件只读，拒绝保存: …`），原样渲染就是
/// `Not saved: IO 失败 @ …`。
///
/// M199 起页面侧已经撞不到这一档了（`java.extra_args` 超长先被 `State::validate` 具名拒掉，见下一条
/// 用例），所以这里直接钉 `write_notice` 这一层的映射：`Schema` 只能变成页面自己的 key。
///
/// 变异证明：把 `Schema(detail)` 映射成 `Notice::Unwritable(NotDirty)` ⇒ 红；把它拼进
/// `Notice::Refused` 的 detail ⇒ 红（Debug 里会出现那句中文）。
#[test]
fn a_schema_rejection_surfaces_as_a_page_key_not_the_core_sentence() {
    use pages::settings::java::store::WriteError;

    let notice = super::super::java::write_notice(WriteError::Schema(
        "IO 失败 @ /tmp/launcher.json".to_string(),
    ));
    assert_eq!(notice, pages::settings::java::Notice::WriteFailed);
    let printed = format!("{notice:?}");
    assert!(
        !printed.contains("IO 失败"),
        "core 的中文句子漏进了界面文案：{printed}"
    );
}

/// 粘了 2000 字符的 JVM 参数：保存被**具名理由**拦下，而且文件一个字节都不动。
///
/// 现场（r3 复审 P3-4）：`java.extra_args` 的 `max_len=1024` 只在 schema 那一档，页面侧不判长度
/// ⇒ 用户得到的是那句通用的「写不进去」，具体原因只在 stderr。本用例走的是 App 那条真保存路径。
///
/// 变异证明：删掉 `State::validate` 里的 `MAX_JVM_ARGS` 判据 ⇒ notice 变回
/// `Notice::WriteFailed`（schema 那一档），本条红。
#[test]
fn an_overlong_jvm_line_is_refused_with_its_own_reason() {
    use pages::settings::java::Notice;

    let (mut app, path) = bridged_app("java-jvm-long");
    let too_long = "-D".to_string() + &"x".repeat(2000);
    let _ = app.update(java(pages::settings::java::Message::EditJvm(too_long)));
    let _ = app.update(java(pages::settings::java::Message::Save));

    let notice = app
        .settings
        .java
        .notice
        .clone()
        .expect("保存必须留下一条回执");
    let Notice::Refused(reason) = notice else {
        panic!("超长参数要给自己的原因（「太长」），不是那句通用的「写不进去」：{notice:?}");
    };
    assert_eq!(
        reason.label(),
        unlucky_i18n::Key::SettingsJavaReasonJvmTooLong,
        "拒绝原因必须落到「太长」那条文案上（中英各一句，见 i18n 金表）"
    );
    assert!(!path.exists(), "被本地校验拒掉的保存不许碰磁盘");
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
}

// --------------------------------------------------------------- M197：三个设置键 + 真下载的落点

/// 三键改完点保存：**真落盘**、草稿被读回值收尾、不再脏，并且保存顺手重扫一次。
///
/// 变异证明：`save_java_prefs` 里不调 `prefs.adopt(snapshot)` ⇒ 草稿还是 `"  D:/JDKs  "`、
/// `dirty` 仍为真 ⇒ 本条红；不调 `start_java_scan` ⇒ `scan.report().running` 那条红。
#[test]
fn saving_the_three_keys_writes_them_and_reads_them_back() {
    use pages::settings::java::prefs_store;

    let (mut app, path) = bridged_app("java-prefs");
    // 落点用这个临时数据根下的绝对路径：Windows / macOS / Linux 上都是绝对的，
    // 断言于是与平台无关（写 "D:/JDKs" 在非 Windows 上是相对路径，会被解析到数据根下）。
    let home = path.parent().unwrap().parent().unwrap().to_path_buf();
    let jdks = home.join("JDKs").display().to_string();
    let _ = app.update(java(pages::settings::java::Message::EditInstallRoot(
        format!("  {jdks}  "),
    )));
    let _ = app.update(java(pages::settings::java::Message::ToggleAutoInstall(
        false,
    )));
    let _ = app.update(java(pages::settings::java::Message::MoveVendor {
        index: 0,
        up: false,
    }));
    assert!(app.settings.java.prefs.dirty(), "改过三键就该是脏的");

    let _ = app.update(java(pages::settings::java::Message::SavePrefs));

    assert_eq!(
        app.settings.java.notice,
        Some(pages::settings::java::Notice::PrefsSaved)
    );
    assert!(
        !app.settings.java.prefs.dirty(),
        "保存并把读回值当草稿之后不该还脏"
    );
    assert_eq!(
        app.settings.java.prefs.install_root, jdks,
        "草稿必须等于磁盘上那份（trim 过的），不是用户刚敲的原样"
    );
    assert!(
        !app.settings.java.prefs.saved_auto_install,
        "auto_install 要真变成关"
    );

    // 冷读：另起一次 load，确认三个键都在文件里。
    let snapshot = prefs_store::load(&path);
    assert_eq!(snapshot.install_root, jdks);
    assert!(!snapshot.auto_install);
    assert_eq!(snapshot.priority.len(), 4, "下移之后落盘的是整条链");
    assert_ne!(snapshot.priority[0], "temurin", "顺序真的被改过");
    assert!(
        app.settings.java.scan.report().running,
        "改了落点必须重扫一次，否则列表还停在旧目录里"
    );
    // 生效值也跟着换了层级（设了 installRoot ⇒ 落点来自设置）。
    assert_eq!(
        app.settings.java.effective.root, jdks,
        "生效落点必须是刚保存的那一个"
    );
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
}

/// `java.auto_install` 开着且还没有默认 JDK：装完之后自动把新装的那份设为默认。
///
/// 变异证明：把 `hydrate_java_downloads` 里那段 `saved_auto_install && selected.is_empty()`
/// 的判据删掉 ⇒ 本条红（`selected` 还是空）；把关掉那一档的断言反过来说明判据不是恒真。
#[test]
fn auto_install_adopts_the_fresh_jdk_when_no_default_is_set() {
    use pages::settings::java::downloads::{Report, Success};

    let (mut app, path) = bridged_app("java-adopt");
    app.settings.java.prefs.saved_auto_install = true;
    app.settings.java.selected = String::new();
    app.settings.java.downloads.seed_report(Report {
        job: None,
        tries: Vec::new(),
        failure: None,
        success: Some(Success {
            major: 21,
            vendor: "Eclipse Temurin",
            root: "D:/jdks/temurin/21".to_string(),
            binary: "D:/jdks/temurin/21/bin/java.exe".to_string(),
            source: Some("tuna".to_string()),
            reused: false,
        }),
    });
    assert!(app.hydrate_java_downloads(), "快照变了就要重建视图");
    assert_eq!(
        app.settings.java.selected, "D:/jdks/temurin/21/bin/java.exe",
        "auto_install 开着 + 没有默认 JDK ⇒ 新装的那份自动成为默认"
    );
    let snapshot = pages::settings::java::store::load(&path);
    assert_eq!(
        snapshot.selected, "D:/jdks/temurin/21/bin/java.exe",
        "而且真写进了 java.selected_path"
    );

    // 关掉 auto_install 时同样的成功快照**不**改默认。
    let (mut app, path) = bridged_app("java-adopt-off");
    app.settings.java.prefs.saved_auto_install = false;
    app.settings.java.selected = String::new();
    app.settings.java.downloads.seed_report(Report {
        job: None,
        tries: Vec::new(),
        failure: None,
        success: Some(Success {
            major: 17,
            vendor: "Eclipse Temurin",
            root: "D:/jdks/temurin/17".to_string(),
            binary: "D:/jdks/temurin/17/bin/java.exe".to_string(),
            source: None,
            reused: true,
        }),
    });
    assert!(app.hydrate_java_downloads());
    assert_eq!(app.settings.java.selected, "", "关掉就不许替用户改默认");
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
}

/// 点一个目录里没有的大版本：立刻在下载区顶上写清原因，**不**起线程。
///
/// 变异证明：把 `start_java_download` 里 `catalog::find(major).is_none()` 那道判据删掉 ⇒
/// `download_notice` 为 `None`，本条红（点了却什么都不说是禁止的）。
#[test]
fn an_unknown_major_is_refused_on_the_spot() {
    use pages::settings::java::downloads::Refusal;

    let (mut app, path) = bridged_app("java-dl-refuse");
    let _ = app.update(java(pages::settings::java::Message::Download(99)));
    assert_eq!(
        app.settings.java.download_notice,
        Some(Refusal::UnknownMajor)
    );
    assert!(!app.settings.java.downloads.busy(), "不该起任何作业");
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
}
