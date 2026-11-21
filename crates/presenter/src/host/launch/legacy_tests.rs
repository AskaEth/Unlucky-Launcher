//! M349 判据：**老版本原版 + 更高版本 Java ⇒ 启动前先问一句**（不拦、也不静默放行）。
//!
//! | 判据 | 抓得住什么 |
//! |---|---|
//! | ① `1.7.10` + Java 21 ⇒ 一条 `Advisory(Unmeasured)`、**没有 Busy / pending / 会话** | 把建议那一步删掉（GUI 回到「什么都不说」）——第一条断言红；把它挪到 `spawn` 之后——`mark_pending` 之前的那几条锚红 |
//! | ② 点上「仍要启动」⇒ 照常受理，且**不再问第二次** | `explicitly_chosen` 不跳过建议（点一次弹一次，永远起不来） |
//! | ③ `1.12.2` ⇒ `Advisory(Measured)`；`1.20.1` ⇒ 什么都不问 | 两档写成同一档（1.7.10 也敢说「可能能启动」）/ 对现代版本也提示（把判据写成「见到 Java 就提示」） |
//! | ④ `1.12.2` + LaunchWrapper 系加载器 ⇒ 仍走**硬拦**（`TooNew`），不是建议 | 建议抢在拦下之前说话（用户以为「仍要启动」就能玩，实际是被拦的那一档） |
//! | ⑤ 全局默认比该版本下限高、而机器上另有一份**正好符合**的 Java ⇒ 不弹建议 | 复审 P2-1 的死路：`fall_back_to_auto` 不把「建议」算成理由 ⇒ 全局默认那份被一直挑中，装着建议里说的那个 Java 也永远不被选中 |
//! | ⑥ 建议事件必须带**证据**（core 的两个常量） | 界面自己编一个实测组合（复审 P2-2 的另一半） |
//!
//! ## 为什么钉一份「比 8 新」的真 Java
//!
//! 自动挑会随开发机装了什么而变（托管里 8 与 21 并存时挑哪个都合法）。所以第①③条**把
//! 本机一份 main > 8 的 Java 钉进实例设置**（`launch_args::apply`，与 GUI「启动参数 → Java
//! 路径」同一个出口），结果就与开发机无关；本机一份 > 8 的都没有时才跳过并打出原因。
//!
//! **钉过 Java 仍然要给建议**（`explicit_java` 不跳过它）：命令行的 `--java` 也照样打那一行
//! （`cli/commands/java_launch.rs` 不看 explicit）——同一份配置在两条路径上必须说同一句话。

use unlucky_core::game::java::{
    system_candidates, JavaRequirement, JavaRuntime, MEASURED_LEGACY_JAVA, MEASURED_LEGACY_VERSION,
};

use crate::dto::{
    AdvisoryTier, AppCommand, AppEvent, Backend, CoreEvent, JavaGuardDirection, LaunchBlockedView,
};

use super::super::CoreHost;
use super::fixtures::{
    host_unpinned, land_snapshot, profile, runtime_event_emitted, settle, write_version_json,
};

/// 本机一份「比 8 新」的真 Java（钉进实例设置，让结果不随开发机变化）。
fn newer_java() -> Option<JavaRuntime> {
    system_candidates()
        .into_iter()
        .find(|runtime| runtime.major.is_some_and(|major| major > 8))
}

/// 把这份 Java 钉进实例设置（产品写盘路径，与 GUI 的「Java 路径」输入框同一个出口）。
fn pin(host: &CoreHost, id: &str, version: &str, runtime: &JavaRuntime) {
    crate::host::launch_args::apply(
        host.env(),
        id,
        version,
        &crate::dto::LaunchArgsPatch {
            java_path: Some(runtime.binary.display().to_string()),
            ..crate::dto::LaunchArgsPatch::default()
        },
    )
    .expect("写实例设置");
}

/// 事件里那条启动前的框（`None` = 这一拍什么都没弹）。
fn guard_event(events: &[AppEvent]) -> Option<LaunchBlockedView> {
    events.iter().find_map(|event| match event {
        AppEvent::Core(CoreEvent::LaunchBlocked(view)) => Some((**view).clone()),
        _ => None,
    })
}

/// 建一条「已经钉好某份 Java」的实例（`None` = 本机够不着，调用方跳过）。
fn pinned_case(
    label: &str,
    version: &str,
) -> Option<(CoreHost, super::fixtures::TempRoot, String, u32)> {
    let runtime = newer_java()?;
    let major = runtime.major.expect("前缀条件已判过");
    let demo = profile("demo", version);
    let id = demo.id.as_str().to_string();
    let (mut host, root) = host_unpinned(label, std::slice::from_ref(&demo));
    pin(&host, &id, version, &runtime);
    land_snapshot(&mut host);
    Some((host, root, id, major))
}

/// **判据①**：老版本原版 + 更高版本 Java ⇒ 先问一句，且**这一拍什么都没起**。
#[test]
fn an_unmeasured_legacy_version_asks_before_launching() {
    let Some((mut host, _root, id, major)) = pinned_case("advisory-unmeasured", "1.7.10") else {
        eprintln!("跳过：本机没有一份 main > 8 的 Java 可钉");
        return;
    };

    host.dispatch(AppCommand::LaunchInstance(id.clone()));
    let events = host.drain_events();

    let advisory = guard_event(&events).expect("老版本原版 + 高版本 Java ⇒ 必须先问一句");
    assert_eq!(advisory.id, id, "建议必须指名是哪一条实例");
    assert_eq!(advisory.version_id, "1.7.10");
    assert_eq!(
        advisory.direction,
        JavaGuardDirection::Advisory(AdvisoryTier::Unmeasured),
        "1.7.10 不在实测范围内 ⇒ 没测过的那一档"
    );
    assert_eq!(advisory.required_min, 8);
    assert_eq!(
        advisory.required_max, None,
        "老版本原版没有上限（M348）：这条建议之所以存在就是因为上限不在这条线上"
    );
    assert_eq!(
        advisory.actual, major,
        "「现在用的是 Java X」必须是真挑中的那个"
    );
    assert_eq!(
        advisory.install_major, None,
        "**钉过实例 Java ⇒ 不给装机出口**（复审 r2 P2-1）：钉死那份优先，装进托管目录的任何一份都不会被挑中"
    );

    // 先问一句 ≠ 受理：不标 pending、不发 Busy、更没起进程。
    assert!(!host.in_flight(&id), "先问一句不算受理（不许标 pending）");
    assert!(
        !runtime_event_emitted(&events),
        "还没拿到用户的回答，磁贴不许进「启动中」：{events:?}"
    );
    assert!(host.launches.is_empty(), "不许进启动表");
}

/// **判据①的对照面**（复审 r3 塔的提醒：重点不是「多一个分支」，而是每枚出口都能落地）：
/// **没钉过** Java 的同一条实例必须仍然给装机出口 —— 否则「钉不钉都一样不给」这种实现也会绿。
#[test]
fn an_unpinned_legacy_version_still_offers_the_install_exit() {
    let Some(runtime) = newer_java() else {
        eprintln!("跳过：本机没有一份 main > 8 的 Java（同判据①）");
        return;
    };
    let version = "1.7.10";
    let demo = profile("demo", version);
    let id = demo.id.as_str().to_string();
    let (mut host, _root) = host_unpinned("advisory-installable", std::slice::from_ref(&demo));
    // **全局默认**（不是实例层钉）：这一档重启会退回自动挑，装机出口是真能落地的。
    host.dispatch(AppCommand::SetSetting {
        key: "java.selected_path".to_string(),
        value: runtime.binary.display().to_string(),
    });
    let _ = host.drain_events();
    land_snapshot(&mut host);

    host.dispatch(AppCommand::LaunchInstance(id));
    let events = host.drain_events();
    let advisory = guard_event(&events).expect("先问一句");
    assert_eq!(
        advisory.install_major,
        Some(8),
        "没钉实例 Java ⇒ 装机出口必须在（否则「去设置/下载」两枚都是死路）"
    );
}

/// **判据②**：点「仍要启动」⇒ 照常受理，而且**不会再问第二次**（否则就是死循环）。
#[test]
fn insisting_after_the_advisory_starts_and_stops_asking() {
    let Some((mut host, _root, id, _major)) = pinned_case("advisory-anyway", "1.7.10") else {
        eprintln!("跳过：本机没有一份 main > 8 的 Java 可钉");
        return;
    };

    host.dispatch(AppCommand::LaunchInstance(id.clone()));
    let first = host.drain_events();
    assert!(guard_event(&first).is_some(), "第一次点启动必须先问一句");

    host.dispatch(AppCommand::LaunchInstanceAnyway(id.clone()));
    let events = host.drain_events();
    assert!(
        guard_event(&events).is_none(),
        "「仍要启动」就是这一步的回答，不许再问一遍：{events:?}"
    );
    assert!(
        runtime_event_emitted(&events),
        "同一拍照常受理（Busy）：{events:?}"
    );
    assert!(host.in_flight(&id), "受理 = 标 pending");

    // 夹具没有版本本体 ⇒ 后台线程在装配阶段收尾，不会真起游戏进程。
    settle(&mut host);
}

/// **判据③**：实测过的那个版本说「可能能启动」，现代版本一句话都不说。
#[test]
fn only_pre_1_13_vanilla_gets_the_advisory() {
    let Some((mut host, _root, id, major)) = pinned_case("advisory-measured", "1.12.2") else {
        eprintln!("跳过：本机没有一份 main > 8 的 Java 可钉");
        return;
    };
    host.dispatch(AppCommand::LaunchInstance(id.clone()));
    let events = host.drain_events();
    let advisory = guard_event(&events).expect("1.12.2 + 高版本 Java 也要先问一句");
    assert_eq!(
        advisory.direction,
        JavaGuardDirection::Advisory(AdvisoryTier::Measured),
        "1.12.2 就是 M347 实测过的那个版本（Java {major}）"
    );
    assert!(
        !runtime_event_emitted(&events),
        "先问一句这一拍不许受理：{events:?}"
    );
    settle(&mut host);

    // 1.20.1 要求 17+：钉的这份 > 8 的 Java 必然满足，而且它不在「老版本原版」这一档。
    let modern = profile("demo", "1.20.1");
    let modern_id = modern.id.as_str().to_string();
    let runtime = newer_java().expect("前缀条件已判过");
    let (mut host, _root) = host_unpinned("advisory-modern", std::slice::from_ref(&modern));
    pin(&host, &modern_id, "1.20.1", &runtime);
    land_snapshot(&mut host);
    host.dispatch(AppCommand::LaunchInstance(modern_id.clone()));
    let events = host.drain_events();
    assert!(
        guard_event(&events).is_none(),
        "≥1.13 没有这条建议：{events:?}"
    );
    assert!(
        runtime_event_emitted(&events),
        "现代版本照常受理（Busy）：{events:?}"
    );
    settle(&mut host);
}

/// **判据④**：`<1.13` + LaunchWrapper 系加载器仍然**硬拦**（`TooNew`），不是「只是建议」。
///
/// 两条必须分得开：拦下那一档「仍要启动」是用户自己承担崩的代价；建议那一档本该能跑。
/// 混成一条，用户就分不清哪次是真的不该起。
#[test]
fn launch_wrapper_legacy_is_blocked_not_advised() {
    let Some(runtime) = newer_java() else {
        eprintln!("跳过：本机没有一份 main > 8 的 Java 可钉");
        return;
    };
    let version = "1.12.2-forge-14.23.5.2860";
    let demo = profile("demo", version);
    let id = demo.id.as_str().to_string();
    // 不写版本本体：装配必然在读清单那一步失败（不会真起进程），而**要求本身**按 id 的
    // 加载器标记就能算出来（core 的 `uses_launch_wrapper` 兜底支路）。
    let (mut host, _root) = host_unpinned("advisory-wrapper", std::slice::from_ref(&demo));
    pin(&host, &id, version, &runtime);
    land_snapshot(&mut host);

    // 钉过 Java 算「显式指定」⇒ 这道闸按 M331 的口径不拦；但**也不许**把它降级成建议。
    host.dispatch(AppCommand::LaunchInstance(id.clone()));
    let events = host.drain_events();
    assert!(
        guard_event(&events).is_none(),
        "上下限都不符但用户钉过 Java ⇒ 既不拦也不建议：{events:?}"
    );
    settle(&mut host);

    // 没钉过的那一条：LaunchWrapper + 高版本 Java ⇒ 拦下（TooNew），而不是 Advisory。
    let plain = profile("demo-plain", version);
    let plain_id = plain.id.as_str().to_string();
    let (mut host, _root) = host_unpinned("advisory-wrapper-block", std::slice::from_ref(&plain));
    land_snapshot(&mut host);
    host.dispatch(AppCommand::LaunchInstance(plain_id.clone()));
    let events = host.drain_events();
    let blocked = guard_event(&events).expect("LaunchWrapper + 高版本 Java 必须拦下");
    assert_eq!(
        blocked.direction,
        JavaGuardDirection::TooNew,
        "这是拦下那一档，不是建议：{blocked:?}"
    );
    assert_eq!(blocked.required_max, Some(8));
    assert!(!runtime_event_emitted(&events), "拦下不许受理：{events:?}");
}

/// **判据⑤（复审 P2-1）**：全局默认比该版本下限高、而机器上另有一份**正好符合**的 Java
/// ⇒ 不弹建议框（否则就是死循环：装好建议里那个 Java，下一拍挑中的仍是全局默认那一份）。
///
/// ## 为什么这里不是「装一个 Java 8」
///
/// 本机没有 JDK 8（有 8 的话 M331 那族用例本来就跳过），而「装好」的判据在**真机**上就是
/// 「托管里出现一份 `java -version` 探得出来、且满足要求的 JDK」。所以这里用同一台机器上的
/// **两档真实 Java**（`JAVA_HOME` 那份 = 低、`PATH` 那份 = 高）复现同一件事：
/// 版本 JSON 声明下限 = 低那档，全局默认 = 高那档 ⇒ 修好之后必须退回自动挑、挑中低那档
/// （`actual == 下限` ⇒ 不再有建议）；没修之前挑中的是全局默认，建议照弹。
///
/// 抓得住：把 `accept.rs` 里 `unsuitable` 的第二个理由（`legacy_advisory_facts(...)`）删掉——
/// 本用例第一处断言红（弹了建议）。
#[test]
fn a_satisfying_java_that_is_not_the_global_default_stops_the_advisory() {
    let Some(fixture) = two_tier_case("advisory-fallback") else {
        eprintln!("跳过：本机没有两档主版本不同的真 Java（需要一份当全局默认、一份正好符合）");
        return;
    };
    let (mut host, _root, id, low, high) = fixture;

    host.dispatch(AppCommand::LaunchInstance(id.clone()));
    let events = host.drain_events();
    assert!(
        guard_event(&events).is_none(),
        "全局默认是 Java {high}，但机器上有一份正好符合下限的 Java {low} ⇒ 必须退回自动挑、不该再弹建议（弹了就是复审 P2-1 那条死路）：{events:?}"
    );
    assert!(
        runtime_event_emitted(&events),
        "既然没有建议，就该同一拍照常受理（Busy）：{events:?}"
    );
    settle(&mut host);
}

/// **判据⑥**：建议事件带着**证据**——`MEASURED_LEGACY_VERSION` / `MEASURED_LEGACY_JAVA` 原样过来。
///
/// 抓得住：presenter 给 `evidence: None`（界面于是少一句、或被迫自己编一个实测组合）。
#[test]
fn the_advisory_event_carries_the_measured_evidence() {
    let Some((mut host, _root, id, _major)) = pinned_case("advisory-evidence", "1.7.10") else {
        eprintln!("跳过：本机没有一份 main > 8 的 Java 可钉");
        return;
    };
    host.dispatch(AppCommand::LaunchInstance(id));
    let events = host.drain_events();
    let advisory = guard_event(&events).expect("先问一句");
    let evidence = advisory
        .evidence
        .expect("建议必须带证据（界面拿它填实测组合）");
    assert_eq!(evidence.version_id, MEASURED_LEGACY_VERSION);
    assert_eq!(evidence.java, MEASURED_LEGACY_JAVA);
}

/// 两档真 Java 的夹具：低那档 = 自动挑会选中的、高那档 = 全局默认。
///
/// `None` = 本机撑不起这个组合（少于两档主版本，或自动挑中的不是最低那一档）——调用方跳过。
fn two_tier_case(label: &str) -> Option<(CoreHost, super::fixtures::TempRoot, String, u32, u32)> {
    let candidates = system_candidates();
    let mut majors: Vec<u32> = candidates
        .iter()
        .filter_map(|runtime| runtime.major)
        .collect();
    majors.sort_unstable();
    majors.dedup();
    let low = *majors.first()?;
    let high = *majors.last()?;
    if high == low {
        return None;
    }
    // 「装上那份正好符合的」= 自动挑会选中的那一份（同 core 的 `pick`，别自己猜优先序）。
    let required = JavaRequirement::table(low, None);
    let satisfying = unlucky_core::game::java::pick(&candidates, &required)?;
    if satisfying.major != Some(low) {
        return None;
    }
    // 全局默认 = 更高那一档（用户「设过默认」的常见状态）。
    let global = candidates
        .iter()
        .find(|runtime| runtime.major == Some(high))?
        .clone();

    const VERSION: &str = "1.12.2";
    let demo = profile("demo", VERSION);
    let id = demo.id.as_str().to_string();
    let (mut host, root) = host_unpinned(label, std::slice::from_ref(&demo));
    // 版本清单声明下限 = 低那档：于是「正好符合」的那份 Java 真的存在（`actual == 下限`）。
    write_version_json(
        host.env(),
        VERSION,
        "net.minecraft.client.main.Main",
        Some(low),
    );
    // 用户去设置页「设为默认」：走的是产品那条命令（`SetSetting` → launcher.json），
    // 与 `m334_tests::a_global_default_alone_still_blocks` 同一个出口。
    host.dispatch(AppCommand::SetSetting {
        key: "java.selected_path".to_string(),
        value: global.binary.display().to_string(),
    });
    let _ = host.drain_events();
    assert_eq!(
        crate::host::launch_args::global_defaults(host.env())
            .java_path
            .as_deref(),
        Some(global.binary.as_path()),
        "前置：全局默认必须真的进了生效值，否则这条判据测的不是它"
    );
    land_snapshot(&mut host);
    Some((host, root, id, low, high))
}
