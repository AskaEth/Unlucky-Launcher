//! M304 的判据：启动/停止的实例解析读**已发布快照**，而不是在调用方线程扫一遍盘。
//!
//! | 判据 | 抓得住什么 |
//! |---|---|
//! | ① 点下去**同一拍**进 Busy（无 tick、无后台回程） | 把受理做成「等解析回来才发 Busy」 |
//! | ② 快照落地后**在盘上新增**一条实例 ⇒ 点它必须被拒绝，不是被扫出来 | 解析改回**现场扫盘**（`game::instances`）|
//! | ③ 快照落地后**删掉**这个实例 ⇒ 点它必须拒绝并给具名原因 | 去掉受理那一刻的单实例复核 |
//! | ④ 两个同名实例 ⇒ 按名字点必须拒绝（不猜） | 名字兜底取第一个 |
//! | ⑤ 停止读快照里的 pid | `running_pid` 兜底改回 `game::instances()` |
//!
//! ②与③是一对：只做②（纯快照、不复核）会把已删实例拉起来；只做③（只复核、读盘）会退回扫盘。
//! 两条都靠**盘面在快照之后变了**来判，不靠计时——计时在 CI 上只会假绿。
//!
//! 另外两条在 [`super::revalidate_tests`]：所属根不见了必须拒绝（r1 P2-1）、运行时态事件也要落进
//! 已发布快照（r1 P2-2）。夹具在 [`super::fixtures`]。

use crate::dto::{AppCommand, Backend, InstanceRuntime};

use super::super::job::HostTask;
use super::super::CoreHost;
use super::fixtures::{
    error_toast, host, land_snapshot, profile, runtime_event_emitted, settle, write,
};

/// **判据①**：点启动 ⇒ `dispatch` 返回时 Busy 已经在事件队列里（不需要 `tick`、不需要后台回程）。
///
/// 抓得住：把解析做成异步（「先起线程，等它回来说受理了」）——那时这里读完事件队列还没有 Busy。
#[test]
fn clicking_launch_shows_busy_in_the_same_beat() {
    let demo = profile("demo", "1.20.1");
    let id = demo.id.as_str().to_string();
    let (mut host, _root) = host("same-beat", std::slice::from_ref(&demo));
    land_snapshot(&mut host);
    let epoch = host.instances_scan.epoch();

    host.dispatch(AppCommand::LaunchInstance(id.clone()));

    let events = host.drain_events();
    assert!(
        runtime_event_emitted(&events),
        "点下去必须**同一拍**进 Busy（不许等解析/扫盘回来）：{events:?}"
    );
    assert!(host.in_flight(&id), "受理要立刻落进 pending");
    assert_eq!(
        host.instances_scan.epoch(),
        epoch,
        "受理这条路不许顺手派一轮扫描（代次变了就说明它扫盘了）"
    );

    settle(&mut host);
}

/// **判据②（变异：解析改回现场扫盘 ⇒ 这条红）**：快照落地之后盘上**多了**一条实例。
///
/// 点它必须被拒绝（快照里没有），而不是被「现场扫一遍盘」扫出来直接拉起来。用户看见的列表里
/// 根本没有这一条——把不在屏幕上的实例启动起来，正是「两套真相」最坏的形状。
///
/// 拒绝之后 host 会自己重扫（`Rejection::stale`）⇒ 重扫完成再点，就必须受理。
#[test]
fn an_instance_that_appeared_after_the_snapshot_is_refused_not_launched() {
    let demo = profile("demo", "1.20.1");
    let late = profile("late", "1.21");
    let late_id = late.id.as_str().to_string();
    let (mut host, _root) = host("late", std::slice::from_ref(&demo));
    land_snapshot(&mut host);

    // 快照之后才写进盘的第二条：host 手里的快照里没有它
    write(host.env(), &[demo, late]);

    host.dispatch(AppCommand::LaunchInstance(late_id.clone()));
    let events = host.drain_events();
    assert!(
        !runtime_event_emitted(&events),
        "解析必须读快照：快照里没有的实例不许被拉起来（现场扫盘就会拉起来）：{events:?}"
    );
    assert!(host.pending.is_empty(), "拒绝受理 ⇒ 一个字节都不进 pending");
    assert!(
        error_toast(&events).contains("没有"),
        "要具名说清「列表里没有这条」"
    );

    // 拒绝不是死路：自动重扫之后它就出现在列表里，再点就受理
    host.pump_instances();
    assert!(
        host.published.iter().any(|instance| instance.id == late_id),
        "过期快照应被重扫替换（拒绝之后要自己跟上盘面）"
    );
    let _ = host.drain_events();
    host.dispatch(AppCommand::LaunchInstance(late_id.clone()));
    assert!(
        runtime_event_emitted(&host.drain_events()),
        "重扫之后同一个 id 必须能受理"
    );

    settle(&mut host);
}

/// **判据③（变异：去掉单实例复核 ⇒ 这条红）**：快照落地之后删掉这个实例。
///
/// 点它必须**拒绝并给具名原因**，而不是按快照把它拉起来。这条是本 mission 的真风险——
/// 性能只影响手感，拿过期快照启动影响的是「起错东西」。
#[test]
fn an_instance_deleted_after_the_snapshot_is_refused_with_a_named_reason() {
    let demo = profile("demo", "1.20.1");
    let id = demo.id.as_str().to_string();
    let (mut host, _root) = host("deleted", std::slice::from_ref(&demo));
    land_snapshot(&mut host);

    // 快照之后才删：盘上真的没了（档案表重写成空表）
    write(host.env(), &[]);

    host.dispatch(AppCommand::LaunchInstance(id.clone()));
    let events = host.drain_events();
    assert!(
        !runtime_event_emitted(&events),
        "已删的实例不许被「按快照」拉起来（那会起错东西/给不出原因）：{events:?}"
    );
    assert!(host.pending.is_empty(), "拒绝受理 ⇒ 一个字节都不进 pending");
    let message = error_toast(&events);
    assert!(
        message.contains("demo") && message.contains("实例清单"),
        "具名原因要说出是哪个实例、为什么（现在是：{message}）"
    );

    // 拒绝之后列表自己跟上：重扫把这条磁贴抹掉
    host.pump_instances();
    assert!(
        host.published.is_empty(),
        "重扫之后已删实例不该还留在快照里：{:?}",
        host.published
    );
}

/// 两个实例同名时**不许猜**：拒绝并说清有几个，按 id 点（界面的真实路径）照常受理。
///
/// 多根之后「两个根里都叫 `A`」是常见布局，取第一个就是在赌用户点的是哪一条。
#[test]
fn two_instances_sharing_a_name_are_never_guessed() {
    let first = profile("同名", "1.20.1");
    let second = profile("同名", "1.21");
    let (mut host, _root) = host("same-name", &[first.clone(), second]);
    land_snapshot(&mut host);

    host.dispatch(AppCommand::LaunchInstance("同名".to_string()));
    let events = host.drain_events();
    assert!(
        !runtime_event_emitted(&events),
        "两个同名实例不许猜一个来启动：{events:?}"
    );
    assert!(host.pending.is_empty());
    assert!(
        error_toast(&events).contains("都叫"),
        "要说清为什么不能替你选"
    );

    // 按 id（磁贴真正带的值）点：照常受理
    host.dispatch(AppCommand::LaunchInstance(first.id.as_str().to_string()));
    assert!(
        runtime_event_emitted(&host.drain_events()),
        "按 id 点必须受理（这条路径不受同名影响）"
    );

    settle(&mut host);
}

/// 停止读的是**快照里的 pid**（host 的启动表里没有它时也一样），不为此扫盘。
///
/// 抓得住：`running_pid` 的兜底分支改回 `game::instances(&env)`——那时盘上这条是 `Idle`，
/// 这里会拿到 `None`（状态说「在跑」、点停止却回「没有在跑」）。
#[test]
fn stop_reads_the_pid_from_the_published_snapshot_not_the_disk() {
    let demo = profile("demo", "1.20.1");
    let id = demo.id.as_str().to_string();
    let (mut host, _root) = host("pid", std::slice::from_ref(&demo));
    land_snapshot(&mut host);

    for instance in &mut host.published {
        if instance.id == id {
            instance.runtime = InstanceRuntime::Running { pid: 4_242 };
        }
    }
    assert!(host.launches.is_empty(), "host 自己的启动表里没有这一场");
    assert_eq!(
        host.running_pid(&id),
        Some(4_242),
        "停止要读快照里的 pid，不要为此再扫一次盘"
    );
}

/// **判据⑥（r1 复审 P2-2）**：运行时态事件（`InstanceRuntimeChanged`）也必须落进已发布快照。
///
/// app 侧 `apply` 把它**就地**写进自己的实例列表（只认扫描回程就会分叉），而 `running_pid` 的
/// 兜底读的正是快照里那一格 —— 今天不炸只因 `launches` 先命中。
///
/// 抓得住：`send()` 只认 `InstancesChanged`（快照停在 Idle，这条红）。
#[test]
fn runtime_events_land_in_the_published_snapshot() {
    let demo = profile("demo", "1.20.1");
    let id = demo.id.as_str().to_string();
    let (mut host, _root) = host("published-runtime", std::slice::from_ref(&demo));
    land_snapshot(&mut host);
    let runtime_of = |host: &CoreHost| {
        host.published
            .iter()
            .find(|instance| instance.id == id)
            .map(|instance| instance.runtime.clone())
    };
    assert_eq!(
        runtime_of(&host),
        Some(InstanceRuntime::Idle),
        "前提：快照里是 Idle"
    );

    // 唯一真实写入路径：后台线程回报「起来了」
    host.handle(HostTask::LaunchStarted {
        id: id.clone(),
        session_id: "20261113T120000-m304a".to_string(),
        pid: 4_242,
    });

    assert_eq!(
        runtime_of(&host),
        Some(InstanceRuntime::Running { pid: 4_242 }),
        "运行时态事件必须同时落进已发布快照（否则界面那份比快照新）"
    );
    assert_eq!(host.running_pid(&id), Some(4_242));
}
