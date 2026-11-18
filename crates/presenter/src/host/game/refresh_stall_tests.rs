//! 扫盘**线程** / 首屏**三态** / 超时兜底（M302 第二组；第一组是 `refresh_tests.rs` 的交错用例）。
//!
//! 从 `refresh_tests.rs` 拆出来守 400 行硬线（那边是三条**交错**用例 + 账本单测）。
//! 这一组验的是另外两件事：
//!
//! | 用例 | 抓得住 | 抓不住 |
//! |---|---|---|
//! | [`an_empty_root_publishes_an_empty_snapshot`] | 把「零实例」当成「没扫完」（界面永远停在「正在扫描」），或者反过来在没扫完时发空快照 | 界面上这两态怎么画（`pages/instances/tests/cold_boot.rs`） |
//! | [`a_stalled_scan_reports_once_and_a_late_snapshot_still_lands`] | `tick` 不检查超时 / 每拍都报 / 报完把在飞的结果丢掉 | 真实 15 s 阈值合不合适（那要真盘压测） |

use std::time::{Duration, Instant};

use super::tests::{host_with, next_scan, snapshots};
use super::{clear_probe, install_probe, GATE, SCAN_STALL};
use crate::dto::{AppEvent, Backend, CoreEvent, InstancePhase};

// ------------------------------------------------------------------ 空数据根 / 超时兜底

/// **真·空数据根**：扫完就是一份**空快照**，而且这一代算「已经落地」。
///
/// 抓得住：把「零实例」误当成「没扫完」（`adopted()` 为空 ⇒ 界面永远停在「正在扫描」）；
/// 或者反过来在没扫完时发一份空快照（那正是任务禁止的假空态）。
/// 抓不住：界面上那两种状态怎么画（`pages/instances/tests/cold_boot.rs` 的控件树 + 出图）。
#[test]
fn an_empty_root_publishes_an_empty_snapshot() {
    let (mut host, _env, _ids, _guard) = host_with("empty-root", 0);
    let _ = host.drain_events();

    host.refresh_instances();
    host.pump_instances();

    let published = snapshots(host.drain_events());
    assert_eq!(
        published.len(),
        1,
        "空根也要发一份快照（不发就永远不知道扫完了）"
    );
    assert!(published[0].is_empty(), "零档案的根扫出来就是空");
    assert_eq!(
        host.instances_scan.adopted().map(|adopted| adopted.count),
        Some(0),
        "账本要记「这一代落地了、0 条」——不是「没有落地」"
    );
}

/// **超时兜底真的会发一条「超时」**，且只发一次；迟到的结果仍然落地（线程没被掐）。
///
/// 判据用 `shorten_budget` 把上限压到 0，不等真实的 15 s（阈值本身仍是 `SCAN_STALL`）。
/// 抓得住：`tick` 没检查超时（用户永远看着「正在扫描实例…」）；每拍都发（面板/日志被刷成噪音）；
/// 报超时之后把在飞的结果丢掉（占位永远不消失）。
/// 抓不住：真实的 15 s 阈值合不合适（那要真盘上压测，见 `SCAN_STALL` 的取值说明）。
#[test]
fn a_stalled_scan_reports_once_and_a_late_snapshot_still_lands() {
    // 阈值是**产品决定**（取值依据写在常量文档里）：改它必须一起改那段话与这条断言
    assert_eq!(SCAN_STALL, Duration::from_secs(15));
    let (mut host, _env, _ids, _guard) = host_with("stall", 1);
    let _ = host.drain_events();

    host.refresh_instances();
    // 把这一轮的上限压到 0：下一次 `tick` 就该判超时
    host.instances_scan.shorten_budget(Duration::ZERO);
    let task = next_scan(&host);

    host.tick(Duration::from_millis(1));
    let events = host.drain_events();
    assert_eq!(
        stalls(&events),
        1,
        "超过上限必须**恰好**报一条「超时」（否则占位永远不消失）：{events:?}"
    );

    // 再拍三下：一条都不许再报。复审 r2 P2-3：老断言把 `drain_events` 调了两次，第二次已经是空的，
    // 「只报一次」根本抓不住重复上报——现在数的是**实际发出的条数**。
    let mut extra = 0;
    for _ in 0..3 {
        host.tick(Duration::from_millis(1));
        extra += stalls(&host.drain_events());
    }
    assert_eq!(extra, 0, "超时只许报一次，又报了 {extra} 次");

    // 迟到的结果照样落地（代次没变 ⇒ 不掐线程的代价是零）
    host.handle(task);
    let published = snapshots(host.drain_events());
    assert_eq!(published.len(), 1, "报过超时之后，迟到的快照仍然要落地");
    assert!(!published[0].is_empty());
}

// ------------------------------------------------------------------ 相位守卫 / 线程判据

fn stalls(events: &[AppEvent]) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                AppEvent::Core(CoreEvent::InstancePhaseChanged(InstancePhase::Stalled))
            )
        })
        .count()
}

fn phases(events: &[AppEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, AppEvent::Core(CoreEvent::InstancePhaseChanged(_))))
        .count()
}

/// **已就绪之后再刷新，不许把相位打回 `Scanning`**（复审 r2 P2-4：实现里有守卫，但没有判据）。
///
/// 现场：用户已经在看列表（`Ready`），任何一次刷新都起一轮新扫描。若那一轮也发 `Scanning`，
/// 界面就把有数据的一屏换成「正在扫描实例…」——把卡顿换成闪屏，正是任务 ⑤ 禁止的。
///
/// 抓得住：删掉 `refresh_instances` 里 `if adopted().is_none()` 那道守卫（发 `Scanning` 变成无条件）。
/// 抓不住：页面怎么画（`pages/instances/tests/cold_boot.rs` 与 app 级
/// `scan_phase::a_refresh_after_ready_keeps_the_list_and_the_tree`）。
#[test]
fn a_refresh_after_ready_emits_no_scanning_phase() {
    let (mut host, _env, _ids, _guard) = host_with("ready-refresh", 1);
    let _ = host.drain_events();
    host.refresh_instances();
    host.pump_instances();
    let _ = host.drain_events();
    assert!(
        host.instances_scan.adopted().is_some(),
        "前提：首份快照已经落地（Ready）"
    );

    host.refresh_instances();
    let events = host.drain_events();
    assert_eq!(
        phases(&events),
        0,
        "已就绪之后再刷新**一条相位事件都不许发**（发了 = 界面退回占位）：{events:?}"
    );

    // 结果照样落地（列表换新，而不是退回占位）
    host.pump_instances();
    assert_eq!(snapshots(host.drain_events()).len(), 1);
}

/// **已就绪之后，一轮慢刷新不许报「超时」**（复审 r2 P2-2：`check_scan_stall` 少了同一个守卫）。
///
/// 现场：`refresh_instances` 有 `adopted().is_none()` 守卫（不打回 `Scanning`），但
/// `check_scan_stall` 没有 ⇒ 只要某一轮扫描慢过 `SCAN_STALL`，界面就被打回「扫描实例比平时久 +
/// 重新扫描」，而列表里其实有数据。
///
/// 抓得住：去掉 `check_scan_stall` 里那道 `adopted().is_some() ⇒ return`。
/// 抓不住：真实 15 s（用 `shorten_budget(0)` 压到 0，判据仍是 `tick` 那条路径）。
#[test]
fn a_slow_refresh_after_ready_never_reports_stalled() {
    let (mut host, _env, _ids, _guard) = host_with("stall-after-ready", 1);
    let _ = host.drain_events();
    host.refresh_instances();
    host.pump_instances();
    let _ = host.drain_events();

    host.refresh_instances();
    host.instances_scan.shorten_budget(Duration::ZERO);
    host.tick(Duration::from_millis(1));
    host.tick(Duration::from_millis(1));
    let events = host.drain_events();
    assert_eq!(
        stalls(&events),
        0,
        "已就绪之后一轮慢刷新不许把界面打回「超时 + 重新扫描」：{events:?}"
    );
}

/// **扫盘不在调用方线程**（复审 r2 P2-1：上一轮那条抓不住 inline，整棵树全绿）。
///
/// 做法：给扫描体装探针 + 闸门（只拦这个数据根的扫描）——扫描体一进来就记线程 id、在闸门下等
/// 放行；调用方返回时断言四件事：**跑过扫描的线程 ≠ 本线程**、**扫描体一趟都没跑完**、
/// **闸门还没放行**、**返回耗时远小于闸门上限**。放行后快照照样落地（不是靠「什么都不扫」换的绿）。
///
/// 抓得住：把 `spawn("instances", job)` 换成直接 `job()`（复审 r2 的变异原样）——四条同时红，
/// 且闸门自带超时（`GATE`），变异时是**红**而不是把用例挂死。
/// 抓不住：线程复用、扫盘真实耗时（后者要真盘量，见 M297 口径）。
#[test]
fn a_refresh_returns_while_the_scan_body_is_still_parked() {
    let (mut host, env, _ids, _guard) = host_with("probe", 1);
    let _ = host.drain_events();
    let (probe, release) = install_probe(env.game.root().to_path_buf());

    let started = Instant::now();
    host.refresh_instances();
    let elapsed = started.elapsed();

    // 扫描体可能还没抢到 CPU：给它一点时间进入闸门
    let deadline = Instant::now() + Duration::from_secs(1);
    while probe.threads().is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        probe.threads().len(),
        1,
        "扫描体必须已经起跑（否则闸门没拦住任何东西，后三条断言会假绿）"
    );
    assert_ne!(
        probe.threads()[0],
        std::thread::current().id(),
        "扫描体跑在调用方线程上（= 扫盘又回到 UI 线程了）"
    );
    assert_eq!(
        probe.finished(),
        0,
        "刷新返回时扫描体**还没跑完**（inline 时它已经跑完了）"
    );
    assert!(
        !probe.released(),
        "闸门还没放行，扫描体却已经走过去了（= 它根本没在后台线程上等）"
    );
    assert!(
        elapsed < GATE / 2,
        "刷新返回耗时 {elapsed:?} 接近闸门上限 {GATE:?} ⇒ 调用方在等扫描（inline）"
    );

    // 放行：结果照样落地
    let _ = release.send(());
    host.pump_instances();
    assert_eq!(snapshots(host.drain_events()).len(), 1);
    assert_eq!(probe.finished(), 1);
    clear_probe();
}
