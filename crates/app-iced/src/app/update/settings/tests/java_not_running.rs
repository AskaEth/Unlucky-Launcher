//! M373 r1 P2-3：「**不在飞**时保存 ⇒ 回执就是 `PrefsSaved`」这一档。
//!
//! 为什么要单独一条：`java.rs` 的 `saving_the_three_keys_writes_them_and_reads_them_back` 原本写死
//! `Notice::PrefsSaved`，但它的前提（首屏那一轮扫完了没有）取决于机器快慢——本机实测保存那一刻扫描
//! 还在飞，于是那条断言变成时序的函数（M373 的 r0 就是被它抓到的）。放宽成「两档都算」之后，
//! **「不在飞」这一档就没人钉了**：把 `prefs.rs` 的 `match outcome` 改成无条件
//! `Notice::PrefsSavedRescanQueued`，上面那条用法照样绿——回执会对每一次保存都说「稍后重扫」，
//! 那也是一种谎话（反过来的一档）。
//!
//! 所以这里用**空槽**把前提钉死（`running` / `finished` 都是假、请求计数 0），走生产入口保存，
//! 断言回执是 `PrefsSaved` 而不是「稍后重扫」那一档。
//!
//! 为什么另开文件：`java_refresh.rs` 已 399/400；`tests.rs` 的 `mod` 声明不在本 mission 的 scope
//! 里，所以从 `java_refresh.rs` 用 `#[path]` 挂上来。
//!
//! 变异证明（真跑见 `DevDocs/artifacts/rescan/M373.md` §8.5）：把 `prefs.rs` 的 `match` 改成无条件
//! `Notice::PrefsSavedRescanQueued` ⇒ 本条红。

use crate::pages;
use pages::settings::java::Notice;

use super::super::bridged_app;
use super::java;

/// 没有扫描在跑时保存三键：回执是 `PrefsSaved`，而且**只请了一次**扫描。
#[test]
fn saving_with_no_scan_running_leaves_the_plain_saved_receipt() {
    let (mut app, path) = bridged_app("java-rescan-idle");
    // 空槽 = 确定「没有扫描在跑」。构造时那次首屏自动扫快慢不定，换掉它这个前提才是确定的。
    app.settings.java.scan = pages::settings::java::store::ScanSlot::default();
    let slot = app.settings.java.scan.clone();
    assert!(!slot.report().running, "前提：这一槽没有扫描在跑");
    assert_eq!(slot.scan_requests(), 0, "前提：这一槽还没请过扫描");

    let home = path.parent().unwrap().parent().unwrap().to_path_buf();
    let jdks = home.join("JDKs").display().to_string();
    let _ = app.update(java(pages::settings::java::Message::EditInstallRoot(
        jdks.clone(),
    )));
    let _ = app.update(java(pages::settings::java::Message::SavePrefs));

    assert_eq!(
        app.settings.java.notice,
        Some(Notice::PrefsSaved),
        "不在飞时保存：回执就是「已保存并从磁盘读回」，不许对每一次保存都说「稍后重扫」"
    );
    assert_eq!(
        slot.scan_requests(),
        1,
        "不在飞时保存：只请一次扫描（不许重复请）"
    );
    assert!(!slot.scan_queued(), "不在飞 ⇒ 不该有排队位");
    assert_eq!(app.settings.java.prefs.install_root, jdks, "落点真的落盘了");

    // 收尾：这一次排上的是**真扫盘**作业，撤掉它，别在后台真扫本机。
    slot.cancel();
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
}
