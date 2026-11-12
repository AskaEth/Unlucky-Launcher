//! `tests.rs` 的第三块（M175 r2）：手动名单的落盘 / 扫描槽的并发与取消 / 错误文案的 i18n。
//!
//! 守卫①~④（列表来自真扫盘、非法路径被拒、内存越界、内存真进命令行）在 `tests.rs` 与
//! `tests/cmdline.rs`；这一块是 r1 复审补的三条（P2-3 / P2-5 / P2-2）。

use super::super::state::{Refusal, RuntimeRow, State, Unwritable};
use super::super::store;
use super::{config, fake_jdk, java_bin, root};

// --------------------------------------------------------------- 守卫⑤：手动名单真落盘（P2-3）

/// 手动加一条合法路径 → **从盘上新读一份** → 名单里有它；「重启」之后仍在。
///
/// 为什么用注入的探测函数：`probe_manual` 要真跑 `java -version`，用例不能依赖本机装没装 JDK；
/// 但**落盘那一段走的是生产代码**（`manual::save_paths` + `manual::load_paths`），所以这条
/// 是「持久化」的正面断言，而不是「内存里的 vec 有它」。
///
/// 变异证明：删掉 `store::add_manual_with` 里的 `manual::save_paths(...)` 那一行 ⇒ 这条红
/// （`load_manual` 从盘上读回 0 条）；把去重那段删掉 ⇒ 末尾那条 `AddDuplicate` 断言红。
#[test]
fn a_manual_path_is_written_to_disk_and_survives_a_restart() {
    use unlucky_core::game::java::{JavaOrigin, JavaRuntime};

    let dir = root("manual-ok");
    let file = config(&dir);
    let jdk = fake_jdk(&dir, "manual-jdk");
    let binary = jdk.join("bin").join(java_bin());
    let runtime = JavaRuntime {
        binary: binary.clone(),
        origin: JavaOrigin::System,
        version: Some("openjdk version \"21.0.4\" 2024-07-16 LTS".to_string()),
        major: Some(21),
        bits: 64,
        vendor: Some("Temurin".to_string()),
    };
    let probe = |_input: &str| Ok(runtime.clone());

    // 刚加之前：盘上什么都没有。
    assert!(store::load_manual(&file).is_empty());

    let (added, row) = store::add_manual_with(&file, &jdk.display().to_string(), &probe)
        .expect("合法路径必须加得进去");
    assert_eq!(added, binary.display().to_string());
    assert_eq!(row.major, Some(21));

    // **从盘上新读一份**（不是看内存里的 vec）。
    let listed = store::load_manual(&file);
    assert_eq!(listed.len(), 1, "名单必须真的写到了盘上：{listed:?}");
    assert_eq!(listed[0], binary.display().to_string());
    assert!(
        store::manual_file(&file).is_file(),
        "名单文件必须真的存在：{:?}",
        store::manual_file(&file)
    );

    // 「重启」：一个全新的 State 从盘上灌，名单还在。
    let reborn = State {
        manual: store::load_manual(&file),
        ..State::default()
    };
    assert_eq!(reborn.manual.len(), 1);

    // 再加同一份 ⇒ 重复（去重也走盘上的名单）。
    assert_eq!(
        store::add_manual_with(&file, &jdk.display().to_string(), &probe).err(),
        Some(Refusal::AddDuplicate)
    );
    std::fs::remove_dir_all(&dir).ok();
}

// --------------------------------------------------------------- 守卫⑥：扫描只许一个线程（P2-5）

static JOB_LIVE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static JOB_PEAK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static JOB_RELEASE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 一个「卡住」的扫描作业：直到被取消或 `JOB_RELEASE` 放开才返回。
fn blocking_job(
    _candidates: &[unlucky_core::game::java::staged::Candidate],
    cancelled: &dyn Fn() -> bool,
    _progress: &mut dyn FnMut(usize, usize),
) -> Vec<RuntimeRow> {
    use std::sync::atomic::Ordering;
    let live = JOB_LIVE.fetch_add(1, Ordering::SeqCst) + 1;
    JOB_PEAK.fetch_max(live, Ordering::SeqCst);
    while !JOB_RELEASE.load(Ordering::SeqCst) && !cancelled() {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    JOB_LIVE.fetch_sub(1, Ordering::SeqCst);
    Vec::new()
}

/// 扫描进行中点「立即扫描」不许起第二个线程；「取消」必须让线程**真的退出**（之后才允许再扫）。
///
/// 变异证明：把 `start_with` 开头那道 `if report.running { return false }` 删掉 ⇒ 第二次
/// `start_with` 返回 `true`、`JOB_PEAK` 变成 2，这条红；把 `cancel()` 里置 `cancelled` 那两行
/// 删掉 ⇒ 作业永远看不到取消、`report().running` 不退，超时断言红。
#[test]
fn only_one_scan_thread_can_run_and_cancel_finishes_it() {
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    JOB_RELEASE.store(false, Ordering::SeqCst);
    JOB_PEAK.store(0, Ordering::SeqCst);
    let slot = store::ScanSlot::default();

    assert!(
        slot.start_with(Vec::new(), Vec::new(), false, blocking_job),
        "第一次扫描应该能起来"
    );
    assert!(
        !slot.start_with(Vec::new(), Vec::new(), false, blocking_job),
        "扫描进行中不许起第二个线程"
    );
    assert!(slot.report().running, "线程活着时 running 必须为真");

    slot.cancel();
    assert!(slot.report().cancelled, "取消之后要标记 cancelled");
    let deadline = Instant::now() + Duration::from_secs(5);
    while slot.report().running && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !slot.report().running,
        "取消必须让扫描线程在预算内真的退出（不是只把界面翻成「已取消」）"
    );
    assert!(!slot.report().finished, "被取消的扫描不许把结论当真");
    assert_eq!(
        JOB_PEAK.load(Ordering::SeqCst),
        1,
        "任何时刻只有一个扫描线程"
    );

    // 线程真退出之后允许重新扫（这就是「取消 ≠ 永久锁死」）。
    JOB_RELEASE.store(true, Ordering::SeqCst);
    assert!(
        slot.start_with(Vec::new(), Vec::new(), false, blocking_job),
        "上一个线程退出之后应该能重新扫"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while slot.report().running && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(slot.report().finished, "这次应该正常扫完");
    JOB_RELEASE.store(false, Ordering::SeqCst);
}

// --------------------------------------------------------------- 守卫⑦：错误文案是 i18n 的（P2-2）

/// 用户可见的失败原因**全部**来自 catalog：三档「写不进去」各有各的一句，手动路径的
/// 「太长 / 写不进名单」也不再顶替「路径是空的 / 路径上没有东西」。
///
/// 抓得住：①三档文案串台（只读说成「没有改动」）；②新加的原因复用旧 key；③把中文硬编码回
/// `store.rs`（源码扫描那一段）。
/// 抓不住：译文翻得好不好（那是金表与人工的事）。
///
/// 变异证明：`Unwritable::ReadOnly` 的 key 改成 `SettingsJavaSkipNotDirty` ⇒ 这条红；
/// 把 `store.rs` 里加回一句硬编码中文 ⇒ 源码扫描那段红。
#[test]
fn the_unwritable_reasons_have_distinct_text() {
    use unlucky_i18n::{Key, Lang};

    let en = Lang::En.catalog();
    let zh = Lang::ZhCn.catalog();
    let reasons = [
        Unwritable::NotDirty,
        Unwritable::ReadOnly,
        Unwritable::Unreadable,
    ];

    for reason in reasons {
        assert_ne!(
            en.text(reason.label()),
            zh.text(reason.label()),
            "{} 未翻译",
            reason.label().name()
        );
    }
    for (index, left) in reasons.iter().enumerate() {
        for right in reasons.iter().skip(index + 1) {
            assert_ne!(
                en.text(left.label()),
                en.text(right.label()),
                "「写不进去」的两档用了同一句文案：{} / {}",
                left.label().name(),
                right.label().name()
            );
        }
    }

    // 手动路径的两档新原因不许顶替旧原因（P2-2 的第三条与第四条）。
    assert_eq!(
        Refusal::AddTooLong.label(),
        Key::SettingsJavaReasonAddTooLong
    );
    assert_eq!(
        Refusal::AddWriteFailed.label(),
        Key::SettingsJavaReasonAddWriteFailed
    );
    assert_ne!(Refusal::AddTooLong.label(), Refusal::AddEmpty.label());
    assert_ne!(Refusal::AddWriteFailed.label(), Refusal::AddMissing.label());

    // 源码扫描：`store.rs` 里不许再有「把 Rust 枚举名或中文字符串当人话」的写法。
    let source = include_str!("../store.rs");
    assert!(
        !source.contains("format!(\"{refusal:?}\")"),
        "不许把 Rust 枚举名（Refusal 的 Debug 串）当用户文案"
    );
    for hardcoded in ["没有改动", "配置文件只读", "配置文件不可信"] {
        // 只认**字符串字面量**（带引号）：文档注释里出现这几个词是正常的说明。
        let literal = format!("\"{hardcoded}\"");
        assert!(
            !source.contains(&literal),
            "store.rs 里还有硬编码中文字符串：{literal}"
        );
    }
}

/// `core` 的每种拒绝原因都要落到**自己的**页面原因上（P2-2 的第三、四条）。
///
/// 抓得住：`ManualFailure::TooLong` 被写成 `Refusal::AddEmpty`（界面会对一条 600 字符的路径说
/// 「路径是空的」）、名单写盘失败被写成 `Refusal::AddMissing`（磁盘写不进去却说「路径上没有东西」）。
/// 抓不住：文案翻得好不好（那是金表与人工的事）。
///
/// 变异证明：`failure_refusal` 里 `TooLong => AddTooLong` 改成 `AddEmpty` ⇒ 本条红；
/// 写盘失败那行 `map_err(|_| AddWriteFailed)` 改成 `AddMissing` ⇒ 本条红。
#[test]
fn every_manual_failure_maps_to_its_own_reason() {
    use unlucky_core::game::java::manual::ManualFailure;

    let dir = root("manual-map");
    let file = config(&dir);
    for (failure, expected) in [
        (ManualFailure::Empty, Refusal::AddEmpty),
        (ManualFailure::TooLong, Refusal::AddTooLong),
        (ManualFailure::Missing, Refusal::AddMissing),
        (ManualFailure::NotJava, Refusal::AddNotJava),
        (ManualFailure::NoVersion, Refusal::AddNoVersion),
    ] {
        let probe = move |_input: &str| Err(failure);
        assert_eq!(
            store::add_manual_with(&file, "whatever", &probe).err(),
            Some(expected),
            "{failure:?} 应该映射到 {expected:?}"
        );
    }
    // 一个都没写进去。
    assert!(store::load_manual(&file).is_empty());
    std::fs::remove_dir_all(&dir).ok();
}

/// 名单文件写不进去（父路径是个文件）时，报的是「写不进名单」而不是「路径上没有东西」。
#[test]
fn a_failed_list_write_reports_the_write_not_the_path() {
    use unlucky_core::game::java::{JavaOrigin, JavaRuntime};

    let dir = root("manual-write-fail");
    // `<dir>/config` 做成**文件**：`manual_file` 的父目录建不出来 ⇒ save_paths 失败。
    // （`root()` 会先建好 `config/`，所以这里先删掉再放一个同名文件。）
    let blocker = dir.join("config");
    std::fs::remove_dir_all(&blocker).expect("删掉空 config 目录");
    std::fs::write(&blocker, b"not a directory").expect("写占位文件");
    let file = blocker.join("launcher.json");
    let jdk = fake_jdk(&dir, "manual-jdk");
    let binary = jdk.join("bin").join(java_bin());
    let probe = |_input: &str| {
        Ok(JavaRuntime {
            binary: binary.clone(),
            origin: JavaOrigin::System,
            version: Some("openjdk version \"17.0.13\" 2024-10-15".to_string()),
            major: Some(17),
            bits: 64,
            vendor: Some("Temurin".to_string()),
        })
    };
    assert_eq!(
        store::add_manual_with(&file, &jdk.display().to_string(), &probe).err(),
        Some(Refusal::AddWriteFailed),
        "磁盘写不进去要说「写不进名单」，不能说「路径不存在」"
    );
    std::fs::remove_dir_all(&dir).ok();
}

// --------------------------------------------------------------- r2 P2-1：空态不许指向不存在的页

/// 空态给的那条「还有别的办法」必须是**真的存在**的入口。
///
/// 现场（r2 复审 P2-1）：原文写「或者去 Java 页装一个」，而 GUI 里根本没有 Java 页
/// （`presenter::router::Route` 只有 Instances / Download / Content / Logs / Settings /
/// Accounts / Detail / Agent，`spawn_install` 在 `crates/app-iced/src` 里零消费者）。
/// 于是「一个 JDK 都没装」的用户——正是这个 mission 要救的那位——被指到一个不存在的入口。
///
/// 抓得住：文案被改回「Java 页 / the Java page」，或把命令行入口那半句删掉。
/// 抓不住：命令行的措辞好不好读（那是人工的事）。
///
/// 变异证明：把 `SettingsJavaEmptyBody` 改回「或者去 Java 页装一个」⇒ 本条红。
#[test]
fn the_empty_state_points_at_a_real_entry() {
    use unlucky_i18n::Lang;

    for lang in [Lang::En, Lang::ZhCn] {
        let text = lang
            .catalog()
            .text(unlucky_i18n::Key::SettingsJavaEmptyBody);
        assert!(
            text.contains("ull java install"),
            "{}：空态要给一条真能走的路（命令行）——{text}",
            lang.tag()
        );
        for dead in ["Java page", "Java 页"] {
            assert!(
                !text.contains(dead),
                "{}：空态指向了不存在的入口「{dead}」——{text}",
                lang.tag()
            );
        }
    }
}
