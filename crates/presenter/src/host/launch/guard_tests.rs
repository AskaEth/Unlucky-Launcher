//! M331 的判据：**Java 与目标版本不符且用户没显式指定 ⇒ 启动受理处就拦下，什么都不起**。
//!
//! | 判据 | 抓得住什么 |
//! |---|---|
//! | ① 不符且非显式 ⇒ 不标 pending / 不发 Busy / 无会话事件流 + 一条结构化拦下事件 | 删掉 `select::decide(...).block()` 那一步，或把它挪到 `spawn` 之后 |
//! | ② 点「仍要启动」⇒ 照常受理（同一拍 Busy + pending），且**一个字节的设置都不写** | `decide` 的 `explicit` 恒传 `true`（①红）、「仍要启动」退化成写 `javaPath` 再启动（②的设置断言红） |
//! | ③ 满足要求 ⇒ 行为不变（照常受理，没有拦下事件） | 把 `check` 换成「见到 Java 就拦」这类粗暴判断 |
//! | ④ 实例里**自己钉过** `javaPath` ⇒ 不拦（与 CLI 的 `explicit_java` 同口径） | 只认「仍要启动」那一个来源（GUI 与 CLI 判得不一样，用户自己钉的 JDK 每次都被拦） |
//!
//! ## ①的四条锚，以及它们各抓什么
//!
//! 拦下的语义是「**没有受理**」，所以锚分两层：
//!
//! | 锚 | 层级 | 抓得住 |
//! |---|---|---|
//! | `!host.in_flight(&id)`（没标 `pending`） | 受理记账 | 把 `mark_pending` 提到判定之前 |
//! | `!runtime_event_emitted(&events)`（没有 `Busy`） | 界面事实 | 先发 Busy 再拦（磁贴闪一下「启动中」） |
//! | `host.launches.is_empty()` | 启动表 | 拦下之后又走回受理、并跑到了 `LaunchStarted` |
//! | `session_logs(&host)` 为空（没有 `.jsonl`） | **盘面事实** | 绕开受理记账**直接**把进程起起来（`Launcher::spawn` 一进去就写会话事件流） |
//!
//! 最后一条只有在**夹具本来能走到 spawn** 时才有意义，所以①会先断言
//! `build_request(...).is_ok()`（见用例里的 [witness] 注）——否则它就是一条恒真空断言
//! （M331 复审 P2-2 实测：没有版本本体时装配在读清单那一步就失败，`spawn` 永远调不到）。
//!
//! **抓不住**：`Launcher::spawn` 之后、会话写入之前那几行的失败（`SessionWriter::create`
//! 之后才可能失败的那部分）；以及「进程起来了但立刻退出」——那已经是收尾（`LaunchFinished`）
//! 的地盘，不属于「有没有受理」。
//!
//! ## 本机依赖（老实写在这里）
//!
//! 判据要成立，本机得有一份**与 1.12.2 + Forge 不符**的 Java（要求 `min = max = 8`）。没有就
//! 跳过并打出原因：机器上一份 Java 都没有时，产品**也不会拦**（`pick_java` 无候选 ⇒ 无可判），
//! 那时这条判据不适用。跳过条件只看 `system_candidates()`（core 的发现结果），**不看**
//! `pick_java` 自己挑了什么——否则「删掉 `check` 那一步」会让用例从红变跳，判据就假绿了。
//!
//! 夹具用 [`host_unpinned`]：**不钉** `javaPath`（钉过就算「用户显式指定」，永远不拦）。
//!
//! ## M348：这道闸的**作用域**变了
//!
//! `<1.13` 的上限只属于 **LaunchWrapper 系加载器**（Forge / LiteLoader，判据是主类
//! `net.minecraft.launchwrapper.Launch`），原版老版本不再拦（原版 1.12.2 + Java 17 实测到主菜单，
//! 进世界未验）。所以这里的样本版本从 `1.12.2` 换成 `1.12.2-forge-14.23.5.2860`，并把版本本体
//! 的主类写成 LaunchWrapper；原版那一侧由
//! [`a_vanilla_legacy_version_is_not_blocked_any_more`] 单独钉住。
//!
//! ⚠️ **这个样本抓的不是「主类判据」**（r1 复审 P3-3）：样本的 id 带 `forge`、清单主类也是
//! LaunchWrapper，两个信号一致 —— 换成「只按 id 认」的实现它照样绿。它锚的是**拦下的语义**
//! （受理记账 / 界面事实 / 盘面事实四层锚 + 「仍要启动」+ 钉过 `javaPath`）。
//! 主类优先（以及 id 兜底只认哪几家）由 core 钉：
//! `crates/core/tests/game/java_mapping.rs::a_launch_wrapper_legacy_version_still_caps_at_java_8`
//! 与 `crates/core/src/game/java/mapping.rs::launch_wrapper_is_a_main_class_question_not_a_version_question`。

use crate::dto::{AppCommand, AppEvent, Backend, CoreEvent, InstanceRuntime, JavaGuardDirection};
use unlucky_core::game::java::LAUNCH_WRAPPER_MAIN_CLASS;

use super::super::CoreHost;
use super::fixtures::{
    host_unpinned, land_snapshot, profile, runtime_event_emitted, session_logs, settle,
    wait_while_no_spawn, write_minimal_version,
};

/// 1.12.2 + Forge 的要求（core：Java 8 起，且 LaunchWrapper 只能用 8）。
const LEGACY_VERSION: &str = "1.12.2-forge-14.23.5.2860";

/// 本机的 Java 主版本号（`system_candidates()` 是 core 的发现结果，与 `pick_java` 无关）。
fn system_majors() -> Vec<u32> {
    unlucky_core::game::java::system_candidates()
        .iter()
        .filter_map(|runtime| runtime.major)
        .collect()
}

/// 「1.12.2 + Forge + 本机只有新 Java」这个组合成不成立（不成立就跳过，理由见模块文档）。
fn legacy_mismatch_is_reproducible() -> bool {
    let majors = system_majors();
    !majors.is_empty() && !majors.contains(&8) && majors.iter().any(|major| *major > 8)
}

/// 盘面上的**设置**快照（`config/` 整棵树；拦下与「仍要启动」都不许改它一个字节）。
fn config_bytes(host: &CoreHost) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    fn walk(dir: &std::path::Path, out: &mut Vec<(std::path::PathBuf, Vec<u8>)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if let Ok(bytes) = std::fs::read(&path) {
                out.push((path, bytes));
            }
        }
    }
    let mut out = Vec::new();
    walk(&host.env().layout.config(), &mut out);
    out.sort();
    out
}

/// 事件里那条拦下事实（`None` = 这一拍没拦）。
fn blocked_event(events: &[AppEvent]) -> Option<crate::dto::LaunchBlockedView> {
    events.iter().find_map(|event| match event {
        AppEvent::Core(CoreEvent::LaunchBlocked(view)) => Some((**view).clone()),
        _ => None,
    })
}

/// **判据①**：不符且非显式 ⇒ 拦下，且**没有子进程**（变异：删掉 `check`/`decide` 那一步 ⇒ 本用例红）。
#[test]
fn a_mismatched_java_is_blocked_before_anything_starts() {
    if !legacy_mismatch_is_reproducible() {
        eprintln!(
            "跳过：本机 Java 组合（{:?}）不构成「1.12.2 + Forge 只能用 8」的不符",
            system_majors()
        );
        return;
    }
    let demo = profile("demo", LEGACY_VERSION);
    let id = demo.id.as_str().to_string();
    let (mut host, _root) = host_unpinned("guard-block", std::slice::from_ref(&demo));
    // **witness（P2-2）**：给夹具补一份最小版本本体，然后确认装配真的能走通。
    // 没有这一步，下面「会话目录没有 `.jsonl`」在任何实现下都绿——装配在读清单就失败，
    // `Launcher::spawn` 永远调不到，那条锚就是假的。有了它，任何绕开拦截走到 spawn 的路径
    // 都会写下会话事件流（`core/src/launch/launcher.rs`：`SessionWriter::create` 排在一切
    // 可能失败的动作**之前**），那条锚才会红。
    write_minimal_version(host.env(), LEGACY_VERSION, LAUNCH_WRAPPER_MAIN_CLASS);
    let witness = super::LaunchTarget {
        id: id.clone(),
        name: demo.name.clone(),
        version_id: LEGACY_VERSION.to_string(),
        account: None,
    };
    super::build_request(host.env(), &witness)
        .expect("夹具必须能装到 spawn 那一步，否则「没有会话文件」是空断言");
    land_snapshot(&mut host);
    let settings_before = config_bytes(&host);

    host.dispatch(AppCommand::LaunchInstance(id.clone()));
    let events = host.drain_events();

    let blocked = blocked_event(&events).expect("Java 不符且用户没指定 ⇒ 必须拦下（结构化事件）");
    assert_eq!(blocked.id, id, "拦下事实必须指名是哪一条实例");
    assert_eq!(blocked.version_id, LEGACY_VERSION);
    assert_eq!(blocked.required_min, 8);
    assert_eq!(
        blocked.required_max,
        Some(8),
        "LaunchWrapper 系的上限还在（M348 只放宽原版）"
    );
    assert_eq!(
        blocked.direction,
        JavaGuardDirection::TooNew,
        "本机挑中的必然比 8 新（前置条件已判过）"
    );
    assert!(
        blocked.actual > 8,
        "实际用的是 Java {}——与 TooNew 自相矛盾",
        blocked.actual
    );

    // ①的四条锚（见模块文档的表）：受理记账 / 界面事实 / 启动表 / 盘面事实各守一层。
    assert!(
        !runtime_event_emitted(&events),
        "拦下等于**没受理**：不许发 Busy/运行中事件，实际：{events:?}"
    );
    assert!(!host.in_flight(&id), "拦下不许标 pending");
    // 盘面锚要**给时间**：拦下这条路上不填 `pending`，`settle()` 会立刻返回，而「绕开受理记账
    // 直接把进程起起来」的路径是后台线程跑的——不等到它跑到 spawn，会话事件流与启动表都还是空的，
    // 这两条锚就成了「断言得太早」的假绿（复审 P2-2 之后我自己踩到的那一版）。
    // 有动静就提前退出，所以判据仍不靠计时：计时上限只决定「正确的实现要等多久」。
    let late = wait_while_no_spawn(&mut host, std::time::Duration::from_secs(1));
    assert!(host.launches.is_empty(), "拦下不许进启动表");
    assert!(
        session_logs(&host).is_empty(),
        "拦下之后出现了会话事件流 {:?} ⇒ `Launcher::spawn` 被调过",
        session_logs(&host)
    );
    assert!(
        !runtime_event_emitted(&late),
        "拦下之后又冒出了运行时事件（换了条路起进程）：{late:?}"
    );
    // 拦下不是失败：磁贴的「失败/重试」外观不该被点亮
    assert!(
        !host.failed.contains_key(&id),
        "拦下不许记成启动失败（那是「起了又挂」的形状）"
    );
    // 连设置都没动过：拦下是纯读判
    assert_eq!(config_bytes(&host), settings_before, "拦下不许改任何设置");
}

/// **判据②**：点「仍要启动」⇒ 照常受理；**设置一个字节都不变**（只放过这一次）。
#[test]
fn insisting_starts_normally_and_persists_nothing() {
    if !legacy_mismatch_is_reproducible() {
        eprintln!("跳过：本机 Java 组合不构成不符（同判据①）");
        return;
    }
    let demo = profile("demo", LEGACY_VERSION);
    let id = demo.id.as_str().to_string();
    let (mut host, _root) = host_unpinned("guard-anyway", std::slice::from_ref(&demo));
    land_snapshot(&mut host);
    let settings_before = config_bytes(&host);

    // 先拦一次（真实路径：用户第一次点启动）
    host.dispatch(AppCommand::LaunchInstance(id.clone()));
    let first = host.drain_events();
    assert!(
        blocked_event(&first).is_some(),
        "第一次点启动必须被拦下（否则「仍要启动」没有来路）"
    );
    assert!(!host.in_flight(&id));

    // 用户看完对话框点「仍要启动」：等价于 CLI 的 `--java`（显式指定）
    host.dispatch(AppCommand::LaunchInstanceAnyway(id.clone()));
    let events = host.drain_events();
    assert!(
        blocked_event(&events).is_none(),
        "显式指定之后不许再拦：{events:?}"
    );
    assert!(
        runtime_event_emitted(&events),
        "「仍要启动」必须**同一拍**照常受理（Busy）：{events:?}"
    );
    assert!(host.in_flight(&id), "受理 = 标 pending");
    let busy = events.iter().any(|event| {
        matches!(
            event,
            AppEvent::Core(CoreEvent::InstanceRuntimeChanged {
                runtime: InstanceRuntime::Busy { .. },
                ..
            })
        )
    });
    assert!(busy, "磁贴要进「启动中」：{events:?}");

    // 夹具里没有版本本体 ⇒ 后台线程必然在装配阶段收尾（不会真起游戏进程）。
    settle(&mut host);
    assert_eq!(
        config_bytes(&host),
        settings_before,
        "「仍要启动」是一次性放行：不许把挑中的 Java 钉进实例设置（javaPath）"
    );
}

/// **判据③**：满足要求 ⇒ 行为不变（照常受理、没有拦下事件）。
#[test]
fn a_satisfying_java_is_never_blocked() {
    let majors = system_majors();
    if !majors.iter().any(|major| *major >= 17) {
        eprintln!("跳过：本机没有 Java 17+（{:?}）", majors);
        return;
    }
    // 1.20.1 要求 17+：本机挑中的必然满足（没有 8 那种极端组合也照样满足）
    let demo = profile("demo", "1.20.1");
    let id = demo.id.as_str().to_string();
    let (mut host, _root) = host_unpinned("guard-satisfied", std::slice::from_ref(&demo));
    land_snapshot(&mut host);

    host.dispatch(AppCommand::LaunchInstance(id.clone()));
    let events = host.drain_events();
    assert!(
        blocked_event(&events).is_none(),
        "满足要求不许拦：{events:?}"
    );
    assert!(
        runtime_event_emitted(&events),
        "满足要求就该照常受理（Busy）：{events:?}"
    );
    settle(&mut host);
}

/// **受理去重**：连点两次「仍要启动」不许起两个进程。
///
/// 这条与「同一事件重投只弹一次」（app 侧）不是一回事：**弹框去重 ≠ 受理去重**。
/// 判据落在受理层：第一次已经 `mark_pending`（`accept_launch` 的第四步），第二次在**第一道闸门**
/// （`in_flight`，同一实例已在 `pending`/`launches` 里）就被挡住，只给一条 Warn，不发 Busy。
///
/// 注意这条只有「放行之后」才成立得起来：拦下的那一次**不标 pending**（拦下不是受理），
/// 所以连点两次启动（非显式）会各拦一次，那是正确行为——用户每次都要看见为什么没起。
#[test]
fn insisting_twice_never_starts_two_processes() {
    if !legacy_mismatch_is_reproducible() {
        eprintln!("跳过：本机 Java 组合不构成不符（同判据①）");
        return;
    }
    let demo = profile("demo", LEGACY_VERSION);
    let id = demo.id.as_str().to_string();
    let (mut host, _root) = host_unpinned("guard-twice", std::slice::from_ref(&demo));
    land_snapshot(&mut host);

    host.dispatch(AppCommand::LaunchInstanceAnyway(id.clone()));
    let first = host.drain_events();
    assert!(
        runtime_event_emitted(&first),
        "第一次「仍要启动」必须照常受理：{first:?}"
    );
    assert!(host.in_flight(&id), "受理 = 立刻标 pending");

    // 用户手快点了第二次（对话框可能还在退场、磁贴已经是「启动中」）
    host.dispatch(AppCommand::LaunchInstanceAnyway(id.clone()));
    let second = host.drain_events();
    assert!(
        !runtime_event_emitted(&second),
        "第二次不许再受理（否则会起第二个进程）：{second:?}"
    );
    assert!(
        second.iter().any(|event| matches!(
            event,
            AppEvent::Toast(toast) if toast.kind == crate::toast::ToastKind::Warn
        )),
        "第二次要给一条「已经在启动/运行中」：{second:?}"
    );
    assert!(
        host.launches.len() <= 1,
        "启动表里不该出现两条同一个实例：{:?}",
        host.launches
    );

    settle(&mut host);
}

/// **判据④（复审 P2-1）**：实例里**自己钉过** `javaPath` ⇒ 不拦（与 CLI 同口径）。
///
/// CLI 的判据是 `explicit_java = chosen_java.is_some()`（`cli/commands/launch.rs`）——用户自己
/// 钉的 JDK 是他明确的选择，CLI 直接放行。GUI 只认「仍要启动」的话，同一份配置在两条路径上
/// 判得不一样，而且用户每次点启动都要再点一次「仍要启动」，那是骚扰不是保护。
///
/// 抓得住：`decide` 的第二个实参退回只认 `explicitly_chosen`（本用例红在「钉过之后仍被拦」）。
/// 抓不住：GUI 里真点开「启动参数 → Java 路径」填表那一段（`pages/instances` 的表单接线），
/// 这里走的是它的后端出口 `launch_args::apply`，与 `AppCommand::SetInstanceLaunchArgs` 同一条。
#[test]
fn an_instance_that_pins_its_own_java_is_not_blocked() {
    if !legacy_mismatch_is_reproducible() {
        eprintln!("跳过：本机 Java 组合不构成不符（同判据①）");
        return;
    }
    // 用户钉的必须是一份**真能探测出「不符」**的 Java（他钉的 JDK 17 跑 1.12.2 那个现场）。
    let Some(pinned) = unlucky_core::game::java::system_candidates()
        .into_iter()
        .find(|runtime| runtime.major.is_some_and(|major| major > 8))
    else {
        eprintln!("跳过：本机没有一份「比 8 新」的 Java 可钉");
        return;
    };

    let demo = profile("demo", LEGACY_VERSION);
    let id = demo.id.as_str().to_string();
    let (mut host, _root) = host_unpinned("guard-pinned", std::slice::from_ref(&demo));
    land_snapshot(&mut host);

    // 来路：没钉之前，同一条实例**会被拦**（否则下面「不拦」证明不了什么）
    host.dispatch(AppCommand::LaunchInstance(id.clone()));
    let before = host.drain_events();
    assert!(
        blocked_event(&before).is_some(),
        "前置条件：还没钉 javaPath 时必须先被拦一次，实际：{before:?}"
    );
    assert!(!host.in_flight(&id));

    // 用户去实例设置里钉一个 Java（走 GUI 的那个后端出口：`launch_args::apply` → 写实例设置）
    crate::host::launch_args::apply(
        host.env(),
        &id,
        LEGACY_VERSION,
        &crate::dto::LaunchArgsPatch {
            java_path: Some(pinned.binary.display().to_string()),
            ..crate::dto::LaunchArgsPatch::default()
        },
    )
    .expect("写实例设置");
    // 这段写盘是真的落到了启动链读的那一份（不是 `InstanceProfile.java_path` 那个没人读的字段）
    assert!(
        crate::host::launch_args::resolved(host.env(), &id)
            .java_path
            .is_some(),
        "javaPath 必须真的写进 config/instances/<id>.json 并被 resolved 读到"
    );

    // 再点启动：钉过 = 显式指定 ⇒ 不拦、不弹框、照常受理
    host.dispatch(AppCommand::LaunchInstance(id.clone()));
    let events = host.drain_events();
    assert!(
        blocked_event(&events).is_none(),
        "用户自己钉过 Java 的实例不许再拦（CLI 同配置直接放行）：{events:?}"
    );
    assert!(
        runtime_event_emitted(&events),
        "钉过 Java ⇒ 照常受理（同一拍 Busy）：{events:?}"
    );
    assert!(host.in_flight(&id), "受理 = 标 pending");
    settle(&mut host);
}
