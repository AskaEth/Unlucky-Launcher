//! host 启动状态机单测（P2-1/P2-2 的回归网）。
//!
//! 覆盖三件事：
//! 1. `running_pid` 按实例 id 取 pid（并发启动不会把 A 的停止键指到 B）；
//! 2. 重复启动守卫（`launches` 已落盘 + `pending` 窗口两态都要挡）；
//! 3. `refresh_instances` 重建快照后仍保留真实运行时（Running / Busy / Failed）。

use unlucky_core::game::install::instance::write_profiles;
use unlucky_core::game::{InstanceProfile, ProfileId, ProfileList};

use super::*;
use crate::dto::{LaunchFinished, LogOutcome};
use crate::toast::ToastKind;

/// 临时数据根守卫：测试结束（含断言 panic）时把目录删掉，不留空壳。
struct TempRoot(std::path::PathBuf);

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 建一个只有一条实例档案的数据根，返回 `(host, instance_id, 守卫)`。
fn host_with_instance(label: &str) -> (CoreHost, String, TempRoot) {
    let dir = std::env::temp_dir().join(format!(
        "ull-host-launch-{label}-{}-{}",
        std::process::id(),
        line!()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let env = HostEnv::at_root(&dir);
    std::fs::create_dir_all(env.layout.config()).expect("config 目录");

    let profile = InstanceProfile {
        id: ProfileId::generate(),
        name: "demo".to_string(),
        version_id: "1.20.1".to_string(),
        game_dir: None,
        running_dir: None,
        java_path: None,
        created_at: None,
        last_played_at: None,
        extra: std::collections::BTreeMap::new(),
    };
    let id = profile.id.as_str().to_string();
    let profiles = ProfileList {
        profiles: vec![profile],
        ..ProfileList::default()
    };
    write_profiles(&env.layout, &profiles).expect("写 profiles");
    (CoreHost::headless(env), id, TempRoot(dir))
}

fn live(instance: &str, session: &str, pid: u32) -> LiveLaunch {
    LiveLaunch {
        instance: instance.to_string(),
        session: session.to_string(),
        pid,
    }
}

#[test]
fn running_pid_is_per_instance_not_a_single_slot() {
    let (mut host, id, _root) = host_with_instance("pid");
    host.launches.push(live("a", "s-a", 111));
    host.launches.push(live("b", "s-b", 222));

    assert_eq!(host.running_pid("a"), Some(111));
    assert_eq!(host.running_pid("b"), Some(222));
    assert_eq!(
        host.running_pid("c"),
        None,
        "没启动过的实例不该拿到别人的 pid"
    );

    // A 收尾后只影响 A：这正是 review r1 P2-1 的场景（旧实现共用一个槽位）。
    host.launches.retain(|entry| entry.instance != "a");
    assert_eq!(host.running_pid("a"), None);
    assert_eq!(host.running_pid("b"), Some(222));

    // 落盘实例默认是 Idle，兜底分支不误报。
    assert_eq!(host.running_pid(&id), None);
}

#[test]
fn duplicate_launch_is_rejected_before_spawning() {
    let (mut host, id, _root) = host_with_instance("dup");
    let _ = host.drain_events();
    host.launches.push(live(&id, "s", 111));

    host.start_launch(&id);

    let events = host.drain_events();
    assert!(
        !events.iter().any(|event| matches!(
            event,
            AppEvent::Core(CoreEvent::InstanceRuntimeChanged { .. })
        )),
        "已在运行/启动中的实例不该再发 Busy（那会起第二个进程）"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AppEvent::Toast(toast) if toast.kind == ToastKind::Warn)),
        "至少要给用户一条「已经在启动/运行中」的提示"
    );
    assert_eq!(host.launches.len(), 1, "启动表不该被重复启动污染");
}

#[test]
fn pending_window_also_blocks_a_second_launch() {
    let (mut host, id, _root) = host_with_instance("pending");
    // 走 start_launch 用的同一个入口，模拟「已受理、后台线程还没回报 LaunchStarted」。
    host.mark_pending(&id);
    assert!(host.in_flight(&id));

    let _ = host.drain_events();
    host.start_launch(&id);
    let events = host.drain_events();
    assert!(
        !events.iter().any(|event| matches!(
            event,
            AppEvent::Core(CoreEvent::InstanceRuntimeChanged { .. })
        )),
        "pending 窗口内重复点启动必须被挡住"
    );
    assert_eq!(host.pending.len(), 1);
}

#[test]
fn launch_started_registers_and_launch_end_clears() {
    let (mut host, id, _root) = host_with_instance("cycle");
    host.mark_pending(&id);

    host.handle(HostTask::LaunchStarted {
        id: id.clone(),
        session_id: "20261031T120000-abcd12".to_string(),
        pid: 4_242,
    });
    assert!(!host.pending.contains(&id), "回报后 pending 要清掉");
    assert_eq!(host.running_pid(&id), Some(4_242));

    host.handle(HostTask::LaunchEnd(Box::new(LaunchFinished {
        id: id.clone(),
        session: "20261031T120000-abcd12".to_string(),
        outcome: LogOutcome::Ok,
        exit_code: Some(0),
        uptime_ms: 12_500,
        failure: None,
    })));
    assert!(host.launches.is_empty(), "收尾后启动表要清空");
    assert!(!host.failed.contains_key(&id));
    assert_eq!(host.running_pid(&id), None);
}

#[test]
fn refresh_keeps_running_instances_running() {
    let (mut host, id, _root) = host_with_instance("refresh-run");
    host.launches.push(live(&id, "s", 4_242));
    let _ = host.drain_events();

    host.refresh_instances();

    let instance = instances_from_events(host.drain_events())
        .into_iter()
        .find(|instance| instance.id == id)
        .expect("刷新后应有该实例");
    assert_eq!(
        instance.runtime,
        InstanceRuntime::Running { pid: 4_242 },
        "刷新（装版本/账户变更/别的实例收尾）不能把在跑实例打回 Idle"
    );
}

#[test]
fn refresh_keeps_pending_as_busy_and_failed_as_failed() {
    let (mut host, id, _root) = host_with_instance("refresh-busy");

    // Busy：走 start_launch 用的同一个入口（mark_pending），而不是直接改字段。
    host.mark_pending(&id);
    let pending = instances_from_events({
        host.refresh_instances();
        host.drain_events()
    })
    .into_iter()
    .find(|instance| instance.id == id)
    .expect("实例存在");
    assert!(matches!(
        pending.runtime,
        InstanceRuntime::Busy {
            stage: crate::dto::LaunchStage::Launch,
            ..
        }
    ));

    // Failed：走唯一真实写入路径 LaunchEnd（不是手动塞 failed 表）。
    host.handle(HostTask::LaunchEnd(Box::new(LaunchFinished {
        id: id.clone(),
        session: "20261031T120000-dead01".to_string(),
        outcome: LogOutcome::Crashed,
        exit_code: Some(1),
        uptime_ms: 3_000,
        failure: None,
    })));
    assert!(!host.pending.contains(&id), "LaunchEnd 必须清 pending");
    assert!(!host.in_flight(&id), "收尾后该实例要能被重新启动");

    let failed = instances_from_events({
        host.refresh_instances();
        host.drain_events()
    })
    .into_iter()
    .find(|instance| instance.id == id)
    .expect("实例存在");
    // 跑起来又崩：原因是**结局**，不是「进程起不来」（复审 P1-1）。
    // 抓得住：把崩溃塞回 `LaunchFailureStage::Spawn`（磁贴会说「启动游戏进程失败」，假话），
    // 或者把退出码漏成 `Some(1)` 这种 Debug 文本（这里只认数字）。
    assert_eq!(
        failed.runtime,
        InstanceRuntime::Failed {
            cause: crate::dto::LaunchFailureCause::Crashed { exit_code: Some(1) }
        },
        "崩溃要如实报成「起来了又崩」，且经得住一次刷新"
    );
}

#[test]
fn launch_end_before_launch_started_still_releases_the_instance() {
    let (mut host, id, _root) = host_with_instance("fail-early");
    // build_request / spawn 失败的现场：start_launch 已标 pending、后台线程只发 LaunchEnd。
    host.mark_pending(&id);
    assert!(host.in_flight(&id));

    host.handle(HostTask::LaunchEnd(Box::new(LaunchFinished {
        id: id.clone(),
        session: String::new(),
        outcome: LogOutcome::Crashed,
        exit_code: None,
        uptime_ms: 0,
        failure: Some(crate::dto::LaunchFailureView::new(
            crate::dto::LaunchFailureStage::Identity,
            "找不到账户 `steve`",
        )),
    })));

    assert!(
        !host.pending.contains(&id),
        "LaunchEnd 早于 LaunchStarted 时也必须清 pending（否则该实例本会话再也起不来）"
    );
    assert!(!host.in_flight(&id), "失败后要能重试");

    let failed = instances_from_events({
        host.refresh_instances();
        host.drain_events()
    })
    .into_iter()
    .find(|instance| instance.id == id)
    .expect("实例存在");
    assert_eq!(
        failed.runtime,
        InstanceRuntime::Failed {
            cause: crate::dto::LaunchFailureCause::Stage {
                stage: crate::dto::LaunchFailureStage::Identity,
                detail: "找不到账户 `steve`".to_string()
            }
        },
        "早失败（进程没起来）要留在阶段那一支，不能被 Idle 盖掉"
    );
}

/// **三种失败形状各归各的**（复审 P1-1 的核心判据）。
///
/// | 收尾事件 | 期望原因 |
/// |---|---|
/// | `failure: Some(..)` | `Stage{..}`（进程**没起来**）|
/// | `failure: None` + `Crashed` | `Crashed{exit_code}`（起来又崩）|
/// | `failure: None` + `SilentFail` | `SilentFailure{exit_code}` |
///
/// 抓得住：把「起来了又崩」塞进 `LaunchFailureStage::Spawn`（磁贴会说「启动游戏进程失败」，
/// 而进程明明起来了）；或者把静默失败也塌成崩溃（丢掉「退出码 0 却没跑起来」这条信息）。
/// 断言的是**枚举变体**，不是字符串里有没有某个词。
#[test]
fn a_launch_end_without_a_stage_reports_the_outcome_not_a_start_failure() {
    use crate::dto::{LaunchFailureCause, LaunchFailureStage, LaunchFailureView};

    /// 一行用例：`(标签, 结局, 阶段失败, 退出码, 期望原因)`。
    type Case = (
        &'static str,
        LogOutcome,
        Option<LaunchFailureView>,
        Option<i32>,
        LaunchFailureCause,
    );

    let cases: Vec<Case> = vec![
        (
            "spawn-failed",
            LogOutcome::Crashed,
            Some(LaunchFailureView::new(
                LaunchFailureStage::Spawn,
                "os error 5",
            )),
            None,
            LaunchFailureCause::Stage {
                stage: LaunchFailureStage::Spawn,
                detail: "os error 5".to_string(),
            },
        ),
        (
            "crashed-after-start",
            LogOutcome::Crashed,
            None,
            Some(1),
            LaunchFailureCause::Crashed { exit_code: Some(1) },
        ),
        (
            "silent-after-start",
            LogOutcome::SilentFail,
            None,
            Some(0),
            LaunchFailureCause::SilentFailure { exit_code: Some(0) },
        ),
    ];

    for (label, outcome, failure, exit_code, expected) in cases {
        let (mut host, id, _root) = host_with_instance(label);
        host.mark_pending(&id);
        host.handle(HostTask::LaunchEnd(Box::new(LaunchFinished {
            id: id.clone(),
            session: format!("20261031T120000-{label}"),
            outcome,
            exit_code,
            uptime_ms: 1_000,
            failure,
        })));
        let failed = instances_from_events({
            host.refresh_instances();
            host.drain_events()
        })
        .into_iter()
        .find(|instance| instance.id == id)
        .expect("实例存在");
        match (&failed.runtime, &expected) {
            (InstanceRuntime::Failed { cause }, want) => {
                assert_eq!(cause, want, "{label}：失败原因说错了");
                assert_eq!(
                    cause.slug(),
                    want.slug(),
                    "{label}：slug 与变体不一致（日志/事件流按 slug 判）"
                );
            }
            (other, want) => panic!("{label}：期望 Failed{{cause:{want:?}}}，实际 {other:?}"),
        }
    }
}

/// 从事件队列里取最后一次 `InstancesChanged`。
fn instances_from_events(events: Vec<AppEvent>) -> Vec<crate::dto::InstanceSummary> {
    events
        .into_iter()
        .rev()
        .find_map(|event| match event {
            AppEvent::Core(CoreEvent::InstancesChanged(instances)) => Some(instances),
            _ => None,
        })
        .unwrap_or_default()
}
