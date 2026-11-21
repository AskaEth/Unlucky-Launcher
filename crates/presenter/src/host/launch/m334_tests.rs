//! M334（台账 P38 / 主人 Q10）的判据：**「算不算用户显式指定 Java」只认实例层与命令行**，
//! 全局默认不算；拦下之后对话框能**真的装一个**对版本的 Java。
//!
//! | 判据 | 抓得住什么 | 抓不住什么 |
//! |---|---|---|
//! | ① [`an_instance_pinning_a_real_java_is_not_blocked`] | 判据里把「实例层钉过的 `javaPath`」删掉 ⇒ 钉了 JDK 17 的 1.12.2 + Forge 又被拦（本用例红） | 页面层「启动参数 → Java 路径」那张表的接线（这里走它的后端出口 `launch_args::apply`） |
//! | ② [`a_global_default_alone_still_blocks`] | 把「全局默认 `java.selected_path`」也算成显式（P38 那个洞）⇒ 本用例红 | 用户在设置页点「设为默认」那个按钮本身（这里派的是同一条 `AppCommand::SetSetting`） |
//! | ③④ 第三个出口与「装完之后」（见 [`super::m334_install_tests`]） | 下载命令没接线 / 装错大版本 / 装完被短路 / 全局默认硬顶 | 真联网装一个 JDK（用例注入假下载） |
//!
//! ## 与 [`super::guard_tests`] 的分工
//!
//! `guard_tests` 判的是「拦下这个动作本身对不对」（受理记账 / 界面事实 / 盘面事实四层锚）；
//! 这里判的是**判据的输入**（谁算显式）与**第三个出口**（装一个）。两边都用 `host_unpinned`：
//! 钉过 `javaPath` 的夹具按口径算「显式」，永远不拦。

use crate::dto::{
    AdvisoryTier, AppCommand, AppEvent, Backend, CoreEvent, JavaGuardDirection, LaunchArgsPatch,
    LaunchBlockedView,
};

use super::super::launch_args;
use super::fixtures::{
    host_unpinned, land_snapshot, profile, runtime_event_emitted, settle, write_minimal_version,
};

/// 1.12.2 + Forge 的要求（Java 8 起，且 LaunchWrapper 只能用 8；M348 之后原版 1.12.2 已无上限）。
///
/// ⚠️ 本文件不写版本 JSON ⇒ 这道上限走的是 **id 标记兜底**那条路（`forge`），
/// **不是**主类判据（r1 复审 P3-3）：换成「只按 id 认」的实现这里照样绿。主类优先由
/// core 的 `java_mapping::a_launch_wrapper_legacy_version_still_caps_at_java_8` 与
/// `mapping::launch_wrapper_is_a_main_class_question_not_a_version_question` 钉住。
pub(super) const LEGACY_VERSION: &str = "1.12.2-forge-14.23.5.2860";

/// 本机的 Java 主版本号（core 的发现结果，与 `pick_java` 无关）。
pub(super) fn system_majors() -> Vec<u32> {
    unlucky_core::game::java::system_candidates()
        .iter()
        .filter_map(|runtime| runtime.major)
        .collect()
}

/// 「1.12.2 + Forge + 本机只有新 Java」这个组合成不成立（不成立就跳过，理由同 `guard_tests`）。
pub(super) fn legacy_mismatch_is_reproducible() -> bool {
    let majors = system_majors();
    !majors.is_empty() && !majors.contains(&8) && majors.iter().any(|major| *major > 8)
}

/// 本机上第一份「比 8 新」的真 Java（判据要一份**真的能探测出不符**的 JDK）。
pub(super) fn a_real_java_newer_than_8() -> Option<unlucky_core::game::java::JavaRuntime> {
    unlucky_core::game::java::system_candidates()
        .into_iter()
        .find(|runtime| runtime.major.is_some_and(|major| major > 8))
}

/// 事件里那条拦下事实（`None` = 这一拍没拦）。
pub(super) fn blocked_event(events: &[AppEvent]) -> Option<LaunchBlockedView> {
    events.iter().find_map(|event| match event {
        AppEvent::Core(CoreEvent::LaunchBlocked(view)) => Some((**view).clone()),
        _ => None,
    })
}

/// **判据①**：实例层钉了一份**真的**与版本不符的 Java ⇒ 不拦（钉过 = 用户自己的选择）。
///
/// 与 `guard_tests::an_instance_that_pins_its_own_java_is_not_blocked` 的区别是**钉的是真 JDK**：
/// 那条钉一个探测不出来的路径，`pick_java` 会按版本要求兜底 ⇒ 判下来本来就相符，于是
/// 「删掉『实例层算显式』」那条变异它**照样绿**（假锚）。这里钉的是本机上真实的 Java 21，
/// 判据里少了实例层就必然拦——变异才打得中。
#[test]
fn an_instance_pinning_a_real_java_is_not_blocked() {
    if !legacy_mismatch_is_reproducible() {
        eprintln!("跳过：本机 Java 组合（{:?}）不构成不符", system_majors());
        return;
    }
    let Some(pinned) = a_real_java_newer_than_8() else {
        eprintln!("跳过：本机没有一份「比 8 新」的 Java 可钉");
        return;
    };
    let demo = profile("demo", LEGACY_VERSION);
    let id = demo.id.as_str().to_string();
    let (mut host, _root) = host_unpinned("m334-pinned", std::slice::from_ref(&demo));

    // 走 GUI 的写盘出口（与「实例 → 启动参数 → Java 路径」同一条）。
    launch_args::apply(
        host.env(),
        &id,
        LEGACY_VERSION,
        &LaunchArgsPatch {
            java_path: Some(pinned.binary.display().to_string()),
            ..LaunchArgsPatch::default()
        },
    )
    .expect("把 javaPath 写进实例设置");
    // witness：钉的这份**真的**与 1.12.2 + Forge 不符，否则下面「不拦」证明不了任何事。
    assert_eq!(
        launch_args::pinned_java(host.env(), &id).as_deref(),
        Some(pinned.binary.as_path()),
        "实例层必须真的钉上那一份"
    );
    assert!(
        pinned.major.is_some_and(|major| major > 8),
        "钉的必须是比 8 新的 Java（{:?}）",
        pinned.major
    );

    land_snapshot(&mut host);
    host.dispatch(AppCommand::LaunchInstance(id.clone()));
    let events = host.drain_events();
    assert!(
        blocked_event(&events).is_none(),
        "实例层钉过 Java ⇒ 算用户显式指定，不许拦：{events:?}"
    );
    assert!(
        runtime_event_emitted(&events),
        "钉过 ⇒ 照常受理（同一拍 Busy）：{events:?}"
    );
    assert!(host.in_flight(&id), "受理 = 标 pending");
    settle(&mut host);
}

/// **判据②（本条修的核心）**：只设了**全局默认** Java、实例层没钉 ⇒ **仍然要拦**。
///
/// 修之前这里读的是三级回落后的生效值（实例 → **全局**），于是凡是在设置里点过一次
/// 「设为默认」的用户，这道闸对所有实例都失效——点 1.12.2 + Forge 照常起、必崩，用户只看到
/// 「退出码 1」（台账 P38）。
///
/// 变异：把判据换成「生效值有值就算显式」⇒ 本用例红在
/// `只设了全局默认 ... 必须拦` 那句。
#[test]
fn a_global_default_alone_still_blocks() {
    if !legacy_mismatch_is_reproducible() {
        eprintln!("跳过：本机 Java 组合（{:?}）不构成不符", system_majors());
        return;
    }
    let Some(global) = a_real_java_newer_than_8() else {
        eprintln!("跳过：本机没有一份「比 8 新」的 Java 可设成全局默认");
        return;
    };
    let demo = profile("demo", LEGACY_VERSION);
    let id = demo.id.as_str().to_string();
    let (mut host, _root) = host_unpinned("m334-global", std::slice::from_ref(&demo));

    // 用户去设置页把某个 JDK「设为默认」：走的是产品那条命令（`SetSetting` → launcher.json）。
    host.dispatch(AppCommand::SetSetting {
        key: "java.selected_path".to_string(),
        value: global.binary.display().to_string(),
    });
    let _ = host.drain_events();

    // witness：全局默认**真的**进了启动链的生效值（否则「仍然要拦」可能只是因为没人读它）。
    assert_eq!(
        launch_args::global_defaults(host.env())
            .java_path
            .as_deref(),
        Some(global.binary.as_path()),
        "全局默认必须真的写进 launcher.json 并被启动链读到"
    );
    assert_eq!(
        launch_args::resolved(host.env(), &id).java_path.as_deref(),
        Some(global.binary.as_path()),
        "生效值就该是那份全局默认（它照样参与「用哪个 Java」）"
    );
    assert!(
        launch_args::pinned_java(host.env(), &id).is_none(),
        "实例层一个字都没钉——这正是 P38 那个现场"
    );

    land_snapshot(&mut host);
    host.dispatch(AppCommand::LaunchInstance(id.clone()));
    let events = host.drain_events();
    let blocked = blocked_event(&events).expect(
        "只设了全局默认 Java 也必须拦：它不算「用户显式指定」\
         （算的话 D4 那道闸对所有实例失效，1.12.2 + Forge 照常起、必崩）",
    );
    assert_eq!(blocked.version_id, LEGACY_VERSION);
    assert_eq!(blocked.actual, global.major.unwrap_or(blocked.actual));
    assert_eq!(
        blocked.install_major,
        Some(8),
        "第三个出口要装的是**这个版本需要的** 8，不是全局默认那个"
    );
    assert!(
        !runtime_event_emitted(&events),
        "拦下等于没受理：{events:?}"
    );
    assert!(!host.in_flight(&id), "拦下不许标 pending");
}

/// **M348 + M349**：**原版**（没有 LaunchWrapper 主类的）`<1.13` 不再被这道闸拦下——闸的作用域
/// 只剩 LaunchWrapper 系加载器（`guard_tests` 的 `LEGACY_VERSION` 现在是 Forge 那条）；
/// 但它**也不许静默放行**（M349）：同一条判据改成「先问一句」，方向是 `Advisory(_)` 而不是
/// `TooOld` / `TooNew`。
///
/// 为什么放在本文件而不是 `guard_tests`：那边贴着 400 行硬线，而这条判据要用的夹具
/// （`host_unpinned` / `land_snapshot` / `blocked_event`）这里全都有。两档的完整矩阵
/// （`Measured` / `Unmeasured` / 现代版本不提示 / LaunchWrapper 仍硬拦）在
/// [`super::legacy_tests`]。
///
/// 变异：①把 `<1.13` 的上限写回静态表（`maximum = Some(8)`）⇒ witness 先红；
/// ②把 M349 那一步删掉（GUI 回到「什么都不说」）⇒ 下面第一处 `expect` 立刻红。
#[test]
fn a_vanilla_legacy_version_is_not_blocked_any_more() {
    if !legacy_mismatch_is_reproducible() {
        eprintln!(
            "跳过：本机 Java 组合（{:?}）不构成「老版本 + 新 Java」的组合（同 guard_tests）",
            system_majors()
        );
        return;
    }
    /// 原版 1.12.2（主类是 `net.minecraft.client.main.Main`，不是 LaunchWrapper）。
    const VANILLA: &str = "1.12.2";
    let demo = profile("demo", VANILLA);
    let id = demo.id.as_str().to_string();
    let (mut host, _root) = host_unpinned("m348-vanilla", std::slice::from_ref(&demo));
    write_minimal_version(host.env(), VANILLA, "net.minecraft.client.main.Main");

    // witness：这条版本的要求**真没有上限**，否则下面「不拦」证明不了闸的作用域。
    let required = unlucky_core::game::java::requirement_for_version(&host.env().game, VANILLA);
    assert_eq!(required.minimum, 8, "下限不变");
    assert_eq!(required.maximum, None, "原版 <1.13 没有上限（M348）");

    land_snapshot(&mut host);
    host.dispatch(AppCommand::LaunchInstance(id.clone()));
    let events = host.drain_events();
    let advisory = blocked_event(&events)
        .expect("原版 1.12.2 + 高版本 Java 必须先问一句（M349）：不拦，但也不许一个字都不说");
    assert_eq!(
        advisory.direction,
        JavaGuardDirection::Advisory(AdvisoryTier::Measured),
        "这是「只是建议」那一档，不是「不符」：{advisory:?}"
    );
    assert_eq!(advisory.required_max, None, "原版 <1.13 没有上限（M348）");
    assert!(
        !runtime_event_emitted(&events),
        "先问一句这一拍不许受理（用户还没回答）：{events:?}"
    );
    assert!(!host.in_flight(&id), "先问一句不算受理");

    // 用户看完点「仍要启动」：照常受理，而且不再问第二次。
    host.dispatch(AppCommand::LaunchInstanceAnyway(id.clone()));
    let events = host.drain_events();
    assert!(
        blocked_event(&events).is_none(),
        "「仍要启动」就是这一步的回答：{events:?}"
    );
    assert!(
        runtime_event_emitted(&events),
        "既然不拦，就该同一拍照常受理（Busy）：{events:?}"
    );
    settle(&mut host);
}
