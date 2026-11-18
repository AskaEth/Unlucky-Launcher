//! M302 的判据：**扫盘搬后台线程之后，谁的结果说了算**。
//!
//! ## 为什么这组用例必须在 `game/` 下
//!
//! 本 mission 的 scope 只开到 `host/mod.rs`、`host/game.rs`、`host/game/**`、`host/job.rs`、
//! `host/dispatch.rs`、`host/handle.rs`（外加 `launch_state_tests.rs`），所以「host 级刷新编排」
//! 的判据落在 `game/refresh.rs` 的子模块里——判据对象是 host，不是 `game::instances` 的盘面判据
//! （后者由 `game/tests*` 那几份守着，本条**一个字节都没动**）。
//!
//! ## 三条交错（任务原文）
//!
//! | 交错 | 用例 | 抓得住 |
//! |---|---|---|
//! | 并发两次刷新 | [`two_refreshes_in_flight_publish_only_the_newest`] | 两份结果里只有最新那份能被采纳 |
//! | 盘面在两轮之间变了 | [`an_older_scan_cannot_resurrect_what_a_newer_one_saw_deleted`] | 「删掉的实例又回来了」 |
//! | 启动收尾触发的第三次刷新 | [`a_launch_end_refresh_beats_the_two_that_were_in_flight`] | 收尾后列表不许被扫描期间的旧结果盖回「运行中」 |
//!
//! 三份都是**真构造**：真临时数据根、真 `profiles.json` 写入、真后台线程跑真 `game::instances`。
//! 只有**交付顺序**是人为摆的（把 `rx` 里的任务按住、按想验的顺序喂给 host）——那正是要验的「先发后到」。
//!
//! 每条用例的「抓不住」写在各自文档里；共同的一条：**不量真实耗时**（进程内可能只有几百微秒），
//! 也不证明用户能感知的快慢——那要真盘上的 236 ms / 1.35 s 场景（M297 的实测口径）。

use std::time::Duration;

use unlucky_core::game::install::instance::write_profiles;
use unlucky_core::game::{InstanceProfile, ProfileId, ProfileList};

use crate::dto::{
    AppEvent, Backend, CoreEvent, InstanceRuntime, InstanceSummary, LaunchFinished, LogOutcome,
};

use crate::host::{CoreHost, HostEnv, HostTask, LiveLaunch};

use super::Scan;

/// 取任务的等待上限（真线程 + 临时根，毫秒级就回来；超时说明真的坏了）
const WAIT: Duration = Duration::from_secs(5);
// ------------------------------------------------------------------ 夹具

/// 临时数据根守卫：测试结束（含断言 panic）时删掉目录。
pub(super) struct TempRoot(std::path::PathBuf);

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 建一个空数据根（`HostEnv::at_root` ⇒ 不做多根发现，盘面只有我们写的东西）。
fn root(label: &str) -> (HostEnv, TempRoot) {
    let dir = std::env::temp_dir().join(format!(
        "ull-m302-{label}-{}-{}",
        std::process::id(),
        line!()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let env = HostEnv::at_root(&dir);
    std::fs::create_dir_all(env.layout.config()).expect("config 目录");
    (env, TempRoot(dir))
}

/// 把盘上的档案**写成** `ids` 这几条（真读写的唯一真相源是 `profiles.json`），返回它们。
fn seed(env: &HostEnv, ids: &[String]) {
    let profiles: Vec<InstanceProfile> = ids
        .iter()
        .enumerate()
        .map(|(index, id)| InstanceProfile {
            id: ProfileId::new(id),
            name: format!("demo-{index}"),
            version_id: "1.20.1".to_string(),
            game_dir: None,
            running_dir: None,
            java_path: None,
            created_at: None,
            last_played_at: None,
            extra: std::collections::BTreeMap::new(),
        })
        .collect();
    write_profiles(
        &env.layout,
        &ProfileList {
            profiles,
            ..ProfileList::default()
        },
    )
    .expect("写 profiles.json");
}

/// `count` 条新档案的 id（落盘用 [`seed`]）。
fn ids(count: usize) -> Vec<String> {
    (0..count)
        .map(|_| ProfileId::generate().as_str().to_string())
        .collect()
}

/// 建好 `count` 条档案的 host（headless：不跑 `refresh_all`，不起版本清单网络线程）。
pub(super) fn host_with(label: &str, count: usize) -> (CoreHost, HostEnv, Vec<String>, TempRoot) {
    let (env, guard) = root(label);
    let ids = ids(count);
    seed(&env, &ids);
    (CoreHost::headless(env.clone()), env, ids, guard)
}

// ------------------------------------------------------------------ 取件

/// 从通道里等下一份扫盘任务（**不经过 `tick`**：这里要自己按住顺序）。
pub(super) fn next_scan(host: &CoreHost) -> HostTask {
    host.rx.recv_timeout(WAIT).expect("扫盘任务要在 5 s 内回来")
}

/// 任务的代次。
fn epoch_of(task: &HostTask) -> u64 {
    match task {
        HostTask::Instances { epoch, .. } => *epoch,
        other => panic!("这不是实例扫描任务：{other:?}"),
    }
}

/// 一份任务里的实例 id（排好序，方便比集合）。
fn ids_of(task: &HostTask) -> Vec<String> {
    match task {
        HostTask::Instances { instances, .. } => {
            let mut ids: Vec<String> = instances.iter().map(|item| item.id.clone()).collect();
            ids.sort();
            ids
        }
        other => panic!("这不是实例扫描任务：{other:?}"),
    }
}

/// 事件队列里所有 `InstancesChanged` 快照（按先后）。
pub(super) fn snapshots(events: Vec<AppEvent>) -> Vec<Vec<InstanceSummary>> {
    events
        .into_iter()
        .filter_map(|event| match event {
            AppEvent::Core(CoreEvent::InstancesChanged(list)) => Some(list),
            _ => None,
        })
        .collect()
}

/// 快照里的实例 id（排好序）。
fn sorted_ids(snapshot: &[InstanceSummary]) -> Vec<String> {
    let mut ids: Vec<String> = snapshot.iter().map(|item| item.id.clone()).collect();
    ids.sort();
    ids
}

// ------------------------------------------------------------------ 判据 ①：真的不在 UI 线程

/// `refresh_instances()` 返回时，**host 的事件队列里**还没有快照。
///
/// 抓得住：谁把「扫盘 + `handle` 落地」整条塞进调用方线程（那样 `drain_events()` 立刻有快照）；
/// 也抓得住「干脆不扫了」（后半段要求它照样会到）。
/// **抓不住**：扫盘本身跑在哪条线程——inline 之后 `HostTask` 仍然要等 `tick` 才进队列，这条照样绿
/// （复审 r2 实测）。「不在调用方线程」由
/// `refresh_stall_tests::a_refresh_returns_while_the_scan_body_is_still_parked` 证明（那里有闸门与线程 id）。
#[test]
fn a_refresh_returns_before_the_scan_is_done() {
    let (mut host, _env, _ids, _guard) = host_with("inline", 1);
    let _ = host.drain_events();

    host.refresh_instances();

    assert!(
        snapshots(host.drain_events()).is_empty(),
        "刷新返回时快照已经到了 ⇒ 扫盘又跑回调用方线程了（M302 要的就是它不在那儿）"
    );

    // 结果照样会到（不是把功能删掉换成「什么都不发」）
    host.pump_instances();
    assert_eq!(
        snapshots(host.drain_events()).len(),
        1,
        "等一代之后必须有且只有一份快照"
    );
}

// ------------------------------------------------------------------ 判据 ②-1：并发两次刷新

/// **并发两次刷新**：两份结果里只有**最新代次**那份能被采纳，先发的那份后到也不许盖。
///
/// 抓得住：去掉代次判据（`admit` 恒真）时这条红——旧代次的那份会照发出第二条快照，
/// 而 `CoreEvent::InstancesChanged` 是**整体替换**，界面就被过期盘面盖掉了。
/// 抓不住：两次扫描之间盘面若完全一样，两份内容无法区分——所以这条另配
/// [`an_older_scan_cannot_resurrect_what_a_newer_one_saw_deleted`] 用内容判别。
#[test]
fn two_refreshes_in_flight_publish_only_the_newest() {
    let (mut host, _env, _ids, _guard) = host_with("concurrent", 2);
    let _ = host.drain_events();

    host.refresh_instances();
    let first_epoch = host.instances_scan.epoch();
    host.refresh_instances();
    let second_epoch = host.instances_scan.epoch();
    assert!(second_epoch > first_epoch, "刷新必须换代次");

    // 两条真线程都真扫完了
    let a = next_scan(&host);
    let b = next_scan(&host);
    // 交付顺序摆成「旧代次后到」——这就是要防的那种交错
    let (stale, fresh) = if epoch_of(&a) < epoch_of(&b) {
        (a, b)
    } else {
        (b, a)
    };
    assert_eq!(epoch_of(&stale), first_epoch);
    assert_eq!(epoch_of(&fresh), second_epoch);

    host.handle(fresh);
    host.handle(stale);

    let published = snapshots(host.drain_events());
    assert_eq!(
        published.len(),
        1,
        "两份结果只许落地一条快照（实际 {} 条）：多了说明旧代次那份也发出去了",
        published.len()
    );
    assert_eq!(published[0].len(), 2, "盘上就是两条");
    assert_eq!(
        host.instances_scan.adopted().map(|adopted| adopted.epoch),
        Some(second_epoch),
        "落地的那份必须是当代次"
    );
    assert_eq!(
        host.instances_scan.dropped(),
        1,
        "旧代次那份要被记成丢弃（可观测口径：> 0 就证明「先发后到」真发生了）"
    );
}

// ------------------------------------------------------------------ 判据 ②-2：扫描途中删实例

/// **扫描途中有一个实例被删**：后到的那份旧扫描里还有它，绝不许把它复活。
///
/// 现场就是最坏的一种：刷新 #1 扫到 3 条 → 用户（或别的启动器）删掉第 3 条 → 刷新 #2 扫到 2 条
/// → #2 先到、#1 后到。没有代次判据时，`InstancesChanged` 的整体替换会把被删的磁贴**放回来**
/// （点进去必然报错，用户以为删失败了）。
///
/// 抓得住：把 `admit` 去掉（或改成恒真）——这条红，因为旧代次那份会被采纳、把被删的磁贴放回来；
/// 把丢掉旧代次那一步做成「发一条空快照」也红（那正是任务里禁止的「清空再重填」）。
/// **不**抓：把 `admit` 的判据从 `==` 改成 `>=` —— 代次只增不减、在飞任务不可能带着**未来**代次，
/// 两种写法行为等价（复审 r1 实测变异③全绿，这条 docstring 原先声称会红，是错的）。
/// 抓不住：刷新 #1 与删除动作**同时**发生的窄窗口——本用例把两份扫描都等回来了再摆交付顺序，
/// 所以它验的是「顺序」，不是「磁盘并发」。
#[test]
fn an_older_scan_cannot_resurrect_what_a_newer_one_saw_deleted() {
    let (mut host, env, ids, _guard) = host_with("delete", 3);
    let _ = host.drain_events();
    let (keep, doomed) = ids.split_at(2);
    let doomed = doomed[0].clone();

    // #1：盘上 3 条
    host.refresh_instances();
    let older = next_scan(&host);
    assert_eq!(ids_of(&older).len(), 3, "前提：#1 扫到了 3 条");

    // 扫描途中删掉一条（真写 profiles.json，与用户删实例是同一条落盘路径）
    seed(&env, keep);

    // #2：盘上 2 条
    host.refresh_instances();
    let newer = next_scan(&host);
    assert_eq!(ids_of(&newer).len(), 2, "前提：#2 扫到了 2 条");

    // 新快照先到
    host.handle(newer);
    let published = snapshots(host.drain_events());
    assert_eq!(published.len(), 1);
    assert_eq!(
        sorted_ids(&published[0]),
        {
            let mut expected = keep.to_vec();
            expected.sort();
            expected
        },
        "新扫描的盘面就是剩下两条"
    );

    // 旧的那份后到：**一条事件都不许出**（不然被删的磁贴又回来了）
    host.handle(older);
    assert!(
        host.drain_events().is_empty(),
        "旧代次的快照不许落地，也不许发空快照（那只是把卡顿换成白屏）"
    );
    assert_eq!(host.instances_scan.dropped(), 1);
    assert!(!doomed.is_empty(), "被删的实例 id 不该是空串");
}

// ------------------------------------------------------------------ 判据 ②-3：收尾触发的第三次刷新

/// **启动收尾触发的第三次刷新**要压住前两次在飞的扫描。
///
/// 现场：连点两次「刷新」→ 扫描在飞 → 游戏退出（`LaunchEnd`）又触发一次刷新。前两次扫的时候
/// 那个实例还在跑，它们的结果若后到，盖回运行时态虽然仍是 `Idle`（盖的时候读的是 host 当前
/// 状态），但**盘面/最后游玩时间**是旧的——而且 `InstancesChanged` 整体替换，界面会被过期快照
/// 顶一下。
///
/// 抓得住：去掉代次判据时这条会落地三条快照（实际断言 1 条）；也抓得住「收尾那次刷新根本没发」
/// （`adopted().epoch` 不是第三次）。
/// 抓不住：真实游戏进程的收尾时序——这里喂的是 `HostTask::LaunchEnd`（host 的唯一收尾入口，
/// 与 `launch` 线程回报的形状一致），进程真的起来/退出由 `launch_state_tests` 与真机覆盖。
#[test]
fn a_launch_end_refresh_beats_the_two_that_were_in_flight() {
    let (mut host, _env, ids, _guard) = host_with("launch-end", 2);
    let id = ids[0].clone();
    let _ = host.drain_events();

    // 两份在飞的扫描（当时这个实例还在跑）
    host.launches.push(LiveLaunch {
        instance: id.clone(),
        session: "20261113T120000-m302a".to_string(),
        pid: 4_242,
    });
    host.refresh_instances();
    let flying_1 = next_scan(&host);
    host.refresh_instances();
    let flying_2 = next_scan(&host);

    // 收尾：清启动表 + 触发第三次刷新
    host.handle(HostTask::LaunchEnd(Box::new(LaunchFinished {
        id: id.clone(),
        session: "20261113T120000-m302a".to_string(),
        outcome: LogOutcome::Ok,
        exit_code: Some(0),
        uptime_ms: 2_000,
        failure: None,
    })));
    let third_epoch = host.instances_scan.epoch();
    let third = next_scan(&host);
    assert_eq!(epoch_of(&third), third_epoch, "收尾必须触发一次新刷新");

    // 先交付收尾那份，再把两份旧的补上
    host.handle(third);
    host.handle(flying_1);
    host.handle(flying_2);

    let published = snapshots(host.drain_events());
    assert_eq!(
        published.len(),
        1,
        "只许收尾那次落地（实际 {} 条）：多出来的是扫描期间就已经过期的盘面",
        published.len()
    );
    let tile = published[0]
        .iter()
        .find(|item| item.id == id)
        .expect("实例还在盘上");
    assert_eq!(
        tile.runtime,
        InstanceRuntime::Idle,
        "收尾之后磁贴必须是 Idle：运行态是**落地那刻**按 host 当前表盖的，收尾已清表，\
         所以哪怕是过期快照落地也会是 Idle —— 这条断言只验「盖的结果对」，\
         新旧之分靠上面的条数与代次断言（`published.len() == 1` / `adopted.epoch` / `dropped`）"
    );
    assert_eq!(
        host.instances_scan.adopted().map(|adopted| adopted.epoch),
        Some(third_epoch)
    );
    assert_eq!(host.instances_scan.dropped(), 2, "前两份都要记成丢弃");
    assert!(!host.in_flight(&id), "收尾之后该实例不该还在飞行态");
}

// ------------------------------------------------------------------ 代次账（纯单元）

/// `Scan` 的账目：换代 → 只认当代次 → 旧代次计数 → 记账。
///
/// 抓得住：把 `admit` 去掉（或改成恒真）、把丢弃计数漏掉、把 `adopt` 记到别的代次上。
/// **不**抓：把 `==` 写成 `>=`（代次只增不减 ⇒ 两种写法等价，复审 r1 变异③实测全绿）。
/// 抓不住：真实交错的时序（那是上面三条用例的事）。
#[test]
fn the_ledger_only_admits_the_current_epoch() {
    let mut scan = Scan::default();
    assert_eq!(scan.epoch(), 0, "初值 = 还没刷新过");
    assert!(scan.adopted().is_none());

    let first = scan.begin();
    assert_eq!(first, 1);
    assert!(scan.admit(first), "当代次必须放行");
    scan.adopt(first, 3, 236);
    let adopted = scan.adopted().expect("刚记的账");
    assert_eq!(
        (adopted.epoch, adopted.count, adopted.elapsed_ms),
        (1, 3, 236)
    );

    let second = scan.begin();
    assert_eq!(second, 2);
    assert!(!scan.admit(first), "旧代次必须丢");
    assert_eq!(scan.dropped(), 1);
    assert_eq!(
        scan.adopted().map(|adopted| adopted.epoch),
        Some(1),
        "丢掉的旧结果不该改写账本"
    );
    assert!(scan.admit(second));
}
