//! M373：在飞时再请一次扫描是**排队**，不是拒绝。
//!
//! 缺口现场（M370 挖出来的 P72，本 mission 的复现对象）：首屏扫描还在跑时保存「安装根」⇒
//! `start_with` 撞上 `report.running` 直接 `return false`，而 `save_java_prefs` 照样把回执写成
//! 「已保存」⇒ **列表停在旧目录**，界面上没有一句说要重扫。
//!
//! 本文件守的是扫描槽这一层（App 层那一条在 `app/update/settings/tests/java_refresh.rs`）：
//!
//! | 用例 | 判据 |
//! |---|---|
//! | 在飞时排队 | ①请了两次（`requests`）= 2；②第二轮真的按**新目录**扫，且最终 `State.runtimes` 就是新目录那一份；③2 这个数在扫描**结束之后**仍然成立（排队那次不重数） |
//! | 不在飞时 | 只扫一轮（不许多扫一次） |
//!
//! 两条都与机器快慢无关：卡住的是**作业自己**（一支静态门），不是靠「本机扫得慢」蒙出来的。

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use super::super::state::{RuntimeRow, State};
use super::super::{scan::Outcome, store};
use super::{fake_jdk, java_bin, root};

/// 等一件事发生（最多 5 秒；判据一律是「状态真的变了」，不是固定 sleep）。
fn wait_until(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < deadline, "等超时了：{what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// 一条扫描**在飞**时再请一次：那一次排队，等这一轮结束按**新目录**再扫一遍——最终列表来自新目录。
///
/// 这就是 M370 P72 那条缺口的正面判据：旧代码在第二次 `start_with` 处 `return false`，
/// 于是 `rounds` 只有一轮、`State.runtimes` 停在旧目录，而且**没有任何信号**表明漏了一次。
///
/// 变异证明（真跑，输出见 `DevDocs/artifacts/rescan/M373.md`）：把排队那一支拆回
/// `if report.running { return false }` ⇒ 第二条断言先红（返回的不是 `Queued`）；
/// 只把「排队」改成「盖掉、但不接着跑」⇒ `rounds.len() == 2` 与 `State.runtimes` 两条红。
#[test]
fn a_request_while_a_scan_is_running_is_queued_and_scans_the_new_location() {
    let dir = root("rescan-queue");
    let old = fake_jdk(&dir, "old-jdk");
    let fresh = fake_jdk(&dir, "new-jdk");
    let new_bin = fresh.join("bin").join(java_bin()).display().to_string();

    queue::GATE.store(false, Ordering::SeqCst);
    if let Ok(mut rounds) = queue::ROUNDS.lock() {
        rounds.clear();
    }

    let slot = store::ScanSlot::default();
    // 首屏那一轮：扫**旧目录**，并卡在作业里（= 首屏扫描还在跑）。
    assert_eq!(
        slot.start_with(Vec::new(), vec![old], false, queue::job),
        Outcome::Running,
        "第一轮应该真的起来"
    );
    wait_until("第一轮进作业", || queue::rounds().len() == 1);

    // 在飞期间「保存安装根」：这一次必须排队，不许被拒。
    assert_eq!(
        slot.start_with(Vec::new(), vec![fresh], false, queue::job),
        Outcome::Queued,
        "在飞的扫描还在跑时，新的请求要排队（旧代码在这里 return false，回执照样写「已保存」）"
    );
    assert_eq!(
        slot.scan_requests(),
        2,
        "首屏 1 次 + 保存 1 次：请求计数必须是 2"
    );

    // 放开：这一轮结束，排队的那一轮应当**自己**接着跑完。
    queue::GATE.store(true, Ordering::SeqCst);
    wait_until("排队的那一轮跑完", || !slot.report().running);

    let report = slot.report();
    assert!(report.finished, "两轮都跑完之后快照必须是「扫完了」");
    // ②最终结论 = 新目录那一轮的结果，而且要**从 state 里读出来**（不只是断言计数）。
    // 这一条先判：它是「用户看到的列表」本身——旧代码在这里给出的是**旧目录**那一份。
    let mut state = State::default();
    state.apply_scan(&report);
    assert_eq!(
        state
            .runtimes
            .iter()
            .map(|row| row.binary.clone())
            .collect::<Vec<_>>(),
        vec![new_bin.clone()],
        "列表必须换成新目录扫到的那些（停在旧目录就是 M370 那条缺口）"
    );
    let rounds = queue::rounds();
    assert_eq!(
        rounds.len(),
        2,
        "排队的那一次必须真的又扫了一轮（旧代码只有一轮）：{rounds:?}"
    );
    assert_eq!(
        rounds[1],
        vec![new_bin],
        "第二轮扫的必须是**新**目录（不是旧目录）：{rounds:?}"
    );
    // ③收尾之后计数仍是 2：排队的那次是同一次请求，跑起来时**不重数**。
    assert_eq!(
        slot.scan_requests(),
        2,
        "扫描结束之后仍然是 2（排队的那次不许在真正跑起来时再 +1）"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// 不在飞时请一次：只许扫一轮。
///
/// 这条是「队列化没有把常规路径染色」的背面判据：把排队那一支写成「永远排队」（比如把
/// `pending` 无条件塞进槽）⇒ `rounds` 会变 2、`requests` 也不再是 1 ⇒ 红。
#[test]
fn a_request_with_no_scan_running_runs_exactly_once() {
    let dir = root("rescan-direct");
    let fresh = fake_jdk(&dir, "only-jdk");
    let bin = fresh.join("bin").join(java_bin()).display().to_string();

    single::GATE.store(true, Ordering::SeqCst);
    if let Ok(mut rounds) = single::ROUNDS.lock() {
        rounds.clear();
    }

    let slot = store::ScanSlot::default();
    assert_eq!(
        slot.start_with(Vec::new(), vec![fresh], false, single::job),
        Outcome::Running,
        "没在扫就该直接起来"
    );
    wait_until("这一轮跑完", || !slot.report().running);

    let rounds = single::rounds();
    assert_eq!(rounds.len(), 1, "不在飞时请一次只许扫一轮：{rounds:?}");
    assert_eq!(rounds[0], vec![bin.clone()], "扫的就是这一次请求的目录");
    assert_eq!(slot.scan_requests(), 1, "请求计数只 +1（不许重复请）");
    assert!(!slot.scan_queued(), "跑完之后队列必须是空的");
    assert_eq!(
        slot.report()
            .runtimes
            .iter()
            .map(|row| row.binary.clone())
            .collect::<Vec<_>>(),
        vec![bin]
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// 一条扫描**在飞**时先点「取消」、再点「保存」：这一次请求排在**取消之后**，必须照跑。
///
/// 现场（M373 r1 复审 P2-2）：`run()` 收尾原来写 `let next = if report.cancelled { None } else { taken }`
/// ——取消之后才排进来的请求被直接丢掉，而回执已经承诺「会按新目录重扫」。可达时序（UI 合法）：
/// 改安装根 → 点取消（`can_save()` 在扫描 / 取消期间都不灰）→ 线程真正退出前点保存 ⇒ 拿到
/// `Queued` 与「会重扫」的回执 ⇒ 收尾把请求丢掉 ⇒ `finished` 仍假，而 `auto_scan_due` 又要求
/// `!cancelled && !finished` ⇒ **不会自动补扫** ⇒ 列表停在旧目录、回执在说谎。
///
/// 规则：**最后的用户动作胜**。`cancel()` 清掉的是「取消之前」排进来的（那由
/// `tests/runtime.rs::only_one_scan_thread_can_run_and_cancel_finishes_it` 钉住）；取消**之后**排
/// 进来的必须能跑——这条用例钉的就是那一半。
///
/// 作业为什么**不听** `cancelled()`：要的是「取消之后仍有一次请求排着」这个确定状态，而不是靠
/// 「线程恰好在取消那一刻还在不在跑」。真实作业是听取消的（它会在下一个候选边界退出），所以卡住的
/// 位置要靠一支只听静态门的作业来固定。
///
/// 变异证明（真跑见 `DevDocs/artifacts/rescan/M373.md` §8.4）：把交接改回
/// `if report.cancelled { None } else { next }` ⇒ 第二轮不跑，`rounds.len() == 1` 与
/// `State.runtimes` 两条红。
#[test]
fn a_request_queued_after_a_cancel_still_runs() {
    let dir = root("rescan-after-cancel");
    let old = fake_jdk(&dir, "cancel-old");
    let fresh = fake_jdk(&dir, "cancel-new");
    let new_bin = fresh.join("bin").join(java_bin()).display().to_string();

    after_cancel::GATE.store(false, Ordering::SeqCst);
    if let Ok(mut rounds) = after_cancel::ROUNDS.lock() {
        rounds.clear();
    }

    let slot = store::ScanSlot::default();
    assert_eq!(
        slot.start_with(Vec::new(), vec![old], false, after_cancel::job),
        Outcome::Running,
        "前提：先起一轮卡住的扫描（扫旧目录）"
    );
    wait_until("第一轮进作业", || after_cancel::rounds().len() == 1);

    // 用户在扫描中途点了「取消」……
    slot.cancel();
    assert!(slot.report().cancelled, "取消之后要标记 cancelled");
    // ……线程还没真退出，又点了「保存」：这一次排在取消之后，必须照跑。
    assert_eq!(
        slot.start_with(Vec::new(), vec![fresh], false, after_cancel::job),
        Outcome::Queued,
        "取消到线程退出之间再请一次：这一轮已死，请求要排队"
    );
    assert_eq!(
        slot.scan_requests(),
        2,
        "首屏 1 次 + 保存 1 次：请求计数必须是 2"
    );

    after_cancel::GATE.store(true, Ordering::SeqCst);
    wait_until("排队的那一轮跑完", || !slot.report().running);

    let report = slot.report();
    assert!(
        !report.cancelled,
        "取消之后又排了一次请求 ⇒ 这一轮是新的一轮，取消位必须被复位：{report:?}"
    );
    assert!(
        report.finished,
        "第二轮必须真的跑完（不是「跑完但丢结论」）"
    );
    let mut state = State::default();
    state.apply_scan(&report);
    assert_eq!(
        state
            .runtimes
            .iter()
            .map(|row| row.binary.clone())
            .collect::<Vec<_>>(),
        vec![new_bin.clone()],
        "列表必须是新目录那一轮的结果（停在旧目录 = 回执在说谎）"
    );
    let rounds = after_cancel::rounds();
    assert_eq!(
        rounds.len(),
        2,
        "取消之后排进来的那一次必须真的又扫了一轮：{rounds:?}"
    );
    assert_eq!(
        rounds[1],
        vec![new_bin],
        "第二轮扫的必须是新目录：{rounds:?}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// 在飞那条用例的注入作业 + 两支静态门。
///
/// 为什么作业要从**候选表**里造行，而不是返回写死的一行：候选表是 `staged::candidates` 拿
/// `manual` / `roots` 在盘上收集出来的，所以「第二轮看到的候选」就是「第二轮扫的是哪个目录」
/// 的证据——判据落在数据上，不用去读线程的入参。
mod queue {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    use std::time::Duration;

    use super::RuntimeRow;

    /// 每一轮看到的候选（按轮次追加；第一轮卡住期间只有一条）。
    pub(super) static ROUNDS: Mutex<Vec<Vec<String>>> = Mutex::new(Vec::new());
    /// 门：`false` 时第一轮卡在作业里（模拟「首屏扫描还在跑」）；放开之后每一轮都直接跑完。
    pub(super) static GATE: AtomicBool = AtomicBool::new(false);

    pub(super) fn rounds() -> Vec<Vec<String>> {
        ROUNDS
            .lock()
            .map(|rounds| rounds.clone())
            .unwrap_or_default()
    }

    pub(super) fn job(
        candidates: &[unlucky_core::game::java::staged::Candidate],
        cancelled: &dyn Fn() -> bool,
        _progress: &mut dyn FnMut(usize, usize),
    ) -> Vec<RuntimeRow> {
        if let Ok(mut rounds) = ROUNDS.lock() {
            rounds.push(binaries(candidates));
        }
        // 卡住直到放开（或被取消）：**在飞**这件事由这支门保证，不靠机器扫得快慢。
        while !GATE.load(Ordering::SeqCst) && !cancelled() {
            std::thread::sleep(Duration::from_millis(5));
        }
        rows(candidates)
    }

    pub(super) fn binaries(
        candidates: &[unlucky_core::game::java::staged::Candidate],
    ) -> Vec<String> {
        candidates
            .iter()
            .map(|candidate| candidate.binary.display().to_string())
            .collect()
    }

    pub(super) fn rows(
        candidates: &[unlucky_core::game::java::staged::Candidate],
    ) -> Vec<RuntimeRow> {
        candidates
            .iter()
            .map(|candidate| RuntimeRow {
                binary: candidate.binary.display().to_string(),
                version: String::new(),
                major: Some(21),
                vendor: None,
                source: candidate.source.id(),
            })
            .collect()
    }
}

/// 「不在飞」那条用例自己的一套静态门（与 `queue` 分开：测试是并行的，共用会互相踩）。
mod single {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    use std::time::Duration;

    use super::queue::{binaries, rows};
    use super::RuntimeRow;

    pub(super) static ROUNDS: Mutex<Vec<Vec<String>>> = Mutex::new(Vec::new());
    pub(super) static GATE: AtomicBool = AtomicBool::new(true);

    pub(super) fn rounds() -> Vec<Vec<String>> {
        ROUNDS
            .lock()
            .map(|rounds| rounds.clone())
            .unwrap_or_default()
    }

    pub(super) fn job(
        candidates: &[unlucky_core::game::java::staged::Candidate],
        cancelled: &dyn Fn() -> bool,
        _progress: &mut dyn FnMut(usize, usize),
    ) -> Vec<RuntimeRow> {
        if let Ok(mut rounds) = ROUNDS.lock() {
            rounds.push(binaries(candidates));
        }
        while !GATE.load(Ordering::SeqCst) && !cancelled() {
            std::thread::sleep(Duration::from_millis(5));
        }
        rows(candidates)
    }
}

/// 「取消之后才排进来」那条用例的静态门：作业**不听** `cancelled()`，只听门。
///
/// 为什么必须不听：真实作业会在下一个候选边界看到取消就退出，于是「取消之后线程还没死」这一段
/// 有多长完全取决于机器——那样就测不准。这里让第一轮一直卡着，取消只是置位、线程不动，于是
/// 「取消之后又排了一次请求」是**确定**的状态。
mod after_cancel {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    use std::time::Duration;

    use super::queue::{binaries, rows};
    use super::RuntimeRow;

    pub(super) static ROUNDS: Mutex<Vec<Vec<String>>> = Mutex::new(Vec::new());
    pub(super) static GATE: AtomicBool = AtomicBool::new(false);

    pub(super) fn rounds() -> Vec<Vec<String>> {
        ROUNDS
            .lock()
            .map(|rounds| rounds.clone())
            .unwrap_or_default()
    }

    pub(super) fn job(
        candidates: &[unlucky_core::game::java::staged::Candidate],
        _cancelled: &dyn Fn() -> bool,
        _progress: &mut dyn FnMut(usize, usize),
    ) -> Vec<RuntimeRow> {
        if let Ok(mut rounds) = ROUNDS.lock() {
            rounds.push(binaries(candidates));
        }
        while !GATE.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(5));
        }
        rows(candidates)
    }
}
