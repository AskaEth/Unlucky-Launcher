//! M334 的**第三个出口**与「装完之后」这一段的判据（从 `m334_tests.rs` 拆出：那边贴着
//! 400 行硬线）。
//!
//! | 判据 | 抓得住什么 | 抓不住什么 |
//! |---|---|---|
//! | ③ [`the_download_exit_installs_the_major_the_blocked_instance_needs`] | 「下载 Java {major}」这条命令没接线 / 装的大版本取错（拿第一条实例的 17 顶掉被拦下那条的 8）/ 下载中没有 `busy` 快照（用户看不见进展） | 真的走网络装一个 JDK（用例注入了一步假下载，见 `install::spawn_install_with`）；也抓不住「四家回退链最后哪家命中」 |
//! | ④ [`the_next_click_after_an_install_goes_back_through_the_gate`] | 把「装完了」当成放行凭据缓存下来（第二次点启动不再判） | 装完真的不再弹框这件事（本机没有 Java 8，用例也不联网） |
//! | ④ [`a_global_default_that_cannot_run_the_version_falls_back_to_a_satisfying_java`] | 全局默认那份被无条件当成要用的那一份（自动挑不参与）⇒ 机器上明明有满足要求的 Java 也过不了闸 | 真下载一个 JDK 再扫出来（那一段由③与 `install` 那条通道守） |
//!
//! ## 真失败路径（复审 r1 P2-1）：不注入假实现，也不打网络
//!
//! [`the_real_step_records_one_step_per_vendor_without_network`] 走 `real_step` 本身（Java 7 比
//! 四家的下限都低 ⇒ `supports` 全判否 ⇒ 每家一跳「跳过」、一次请求都不发）；
//! [`the_real_failure_reaches_the_ui_with_steps_and_no_prose`] 走 `spawn_install_with` 整条回流
//! （断「终态带逐家的链、`message` 一个字没有」）；[`strip_single_vendor_prefix_matches_cores_wording`]
//! 用 **core 真的产出的那句失败原文** 断去壳（microsoft 不发 Java 8，core 在 `supports` 就返回）。
//!
//! **抓不住**：真正的网络往返（离线不可构造）⇒「哪家真命中 / 网络类原因长什么样」没有实证。
//!
//! 判据①②（谁算显式）在 [`super::m334_tests`]。

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use crate::dto::{AppCommand, AppEvent, Backend, CoreEvent, JavaChainOutcome, JavaChainStep};

use super::super::launch_args;
use super::fixtures::{host_unpinned, land_snapshot, profile, runtime_event_emitted, settle};
use super::m334_tests::{
    blocked_event, legacy_mismatch_is_reproducible, system_majors, LEGACY_VERSION,
};

/// 用例注入的「下载」一步：记下被要求装的大版本，回一条命中。
///
/// 用 `static` 是因为那一步跑在**后台线程**里（线程局部传不过去）。两条用它的用例都只装
/// Java 8 ⇒ 共用一份不会读错；**不要在用例里把它清 0**——那会在并行跑时踩掉另一条刚写进去的
/// 观测值（r2 返工实测过一次假红）。
static INSTALL_MAJOR: AtomicU32 = AtomicU32::new(0);

fn fake_install(_env: &crate::host::env::HostEnv, major: u32) -> Vec<JavaChainStep> {
    INSTALL_MAJOR.store(major, Ordering::SeqCst);
    vec![JavaChainStep {
        vendor: "temurin".to_string(),
        outcome: JavaChainOutcome::Hit,
        detail: "命中 Adoptium（用例注入的一步，没走网络）".to_string(),
    }]
}

/// **判据③**：拦下后点「下载 Java {major}」⇒ 真的开始装，而且装的是**这一条实例**要的那个版本。
///
/// 走的是产品那条命令（`AppCommand::InstallJavaFor` → `launch::spawn_install_major`），
/// 只有「下载」那一步被换掉（`install::spawn_install_with` 的注入点，用例绝不联网）。
///
/// 锚两条，都不是「函数被调过」：
/// 1. **下载中有一份 `busy = true` 的 Java 页快照**——界面的进展行就靠它点亮；
/// 2. 注入的那一步收到的**大版本是 8**（被拦下这条实例的要求），而不是 `target_major`
///    会给出的「第一条实例的要求」。
#[test]
fn the_download_exit_installs_the_major_the_blocked_instance_needs() {
    if !legacy_mismatch_is_reproducible() {
        eprintln!("跳过：本机 Java 组合（{:?}）不构成不符", system_majors());
        return;
    }
    let demo = profile("demo", LEGACY_VERSION);
    let id = demo.id.as_str().to_string();
    let (mut host, _root) = host_unpinned("m334-install", std::slice::from_ref(&demo));
    land_snapshot(&mut host);

    // 前置：先被拦下（第三个出口的来路就是这条框，大版本号也从这里来）。
    host.dispatch(AppCommand::LaunchInstance(id.clone()));
    let blocked = blocked_event(&host.drain_events()).expect("前置：1.12.2 必须先被拦下");
    assert_eq!(blocked.install_major, Some(8), "1.12.2 要的就是 8");

    super::install::set_install_step_for_tests(Some(fake_install));
    host.dispatch(AppCommand::InstallJavaFor {
        major: blocked.install_major.expect("硬拦一定有装机出口"),
    });

    let mut saw_busy = false;
    let mut finished: Option<crate::dto::JavaPageView> = None;
    // 预算按**时间**给（不是按圈数）：`page()` 会把本机每个 Java 都探一遍（`java -version`
    // 子进程），并行跑整套用例时那两次探测能超过 2 s——按圈数给会长出「假超时」（r2 返工实测）。
    // 有结果就提前退出，所以判据仍不靠计时：上限只决定「正确的实现最多等多久」。
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        host.tick(Duration::from_millis(1));
        for event in host.drain_events() {
            if let AppEvent::Core(CoreEvent::JavaChanged(page)) = event {
                let hit = page
                    .chain
                    .iter()
                    .any(|step| step.outcome == JavaChainOutcome::Hit);
                if page.busy {
                    saw_busy = true;
                } else if hit {
                    finished = Some(page);
                }
            }
        }
        if saw_busy && finished.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    super::install::set_install_step_for_tests(None);

    assert!(
        saw_busy,
        "下载中必须有一份 `busy = true` 的快照：那是界面上「正在下载 Java 8…」的唯一来路"
    );
    assert_eq!(
        INSTALL_MAJOR.load(Ordering::SeqCst),
        8,
        "装的必须是**被拦下那条实例**要的 8；取成 `target_major`（第一条实例的要求）就可能是 17"
    );
    let done = finished.expect("装完必须有一份命中快照（`busy = false` + 链上 `Hit`）");
    assert_eq!(
        done.hit_vendor(),
        Some("temurin"),
        "命中的是哪一家要能从快照里读出来（界面按它说「谁装的」）"
    );
}

/// **判据④（前半）**：装完之后再点启动 ⇒ **重新过闸**，不是把上一次的结论复用。
///
/// 本机没有 Java 8（用例也不联网真装），所以这里照实**再拦一次**——这正是要断的东西：
/// 闸门必须按**当下的**事实重判。变异（把「已经装过 / 用户已经处理过这个框」当成放行凭据
/// 缓存下来）会让第二次不再拦，本用例红。
///
/// 「真的过闸」那一半见 [`a_global_default_that_cannot_run_the_version_falls_back_to_a_satisfying_java`]，
/// 以及报告里「没验的部分」。
#[test]
fn the_next_click_after_an_install_goes_back_through_the_gate() {
    if !legacy_mismatch_is_reproducible() {
        eprintln!("跳过：本机 Java 组合（{:?}）不构成不符", system_majors());
        return;
    }
    let demo = profile("demo", LEGACY_VERSION);
    let id = demo.id.as_str().to_string();
    let (mut host, _root) = host_unpinned("m334-rejudge", std::slice::from_ref(&demo));
    land_snapshot(&mut host);

    host.dispatch(AppCommand::LaunchInstance(id.clone()));
    let first = blocked_event(&host.drain_events()).expect("先被拦下");
    super::install::set_install_step_for_tests(Some(fake_install));
    host.dispatch(AppCommand::InstallJavaFor {
        major: first.install_major.expect("硬拦一定有装机出口"),
    });
    // 等这一步收尾：看到**终态快照**（`busy = false` + 链上命中）就算跑完。
    // 判据不靠共享计数器（并行跑时两条用例都会写它，会互相踩出假红）。
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        host.tick(Duration::from_millis(1));
        let finished = host.drain_events().iter().any(|event| {
            matches!(
                event,
                AppEvent::Core(CoreEvent::JavaChanged(page))
                    if !page.busy
                        && page
                            .chain
                            .iter()
                            .any(|step| step.outcome == JavaChainOutcome::Hit)
            )
        });
        if finished {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    super::install::set_install_step_for_tests(None);

    // 再点一次启动：闸门必须按当下的事实重判（本机还是只有 21/25 ⇒ 照实再拦）。
    host.dispatch(AppCommand::LaunchInstance(id.clone()));
    let second = blocked_event(&host.drain_events()).expect(
        "装完再点启动必须**重新过闸**；这里没有 Java 8 落地，所以正确答案是再拦一次\
         ——不拦说明闸门被上一次的结论短路了",
    );
    assert_eq!(second.id, first.id);
    assert_eq!(second.install_major, first.install_major);
}

/// **判据④（后半，穿过产品路径）**：全局默认那份 Java 跑不了这个版本时，托管/系统里只要
/// 有一份**满足要求**的，下一次点启动就过闸——全局默认那份不会被硬塞进去。
///
/// 这半条不是锦上添花：设过全局默认 Java 21 的用户点一个要 Java 25 的版本，装完 25 之后再点
/// 启动，若还拿 21 去撞就永远过不了闸——用户照对话框做了每一步，结果一点没变。所以
/// `preflight_java` 的优先级是「实例层钉的 → 全局默认（**只在它满足要求时**）→ 自动挑」。
///
/// 构造用的是**本机真实存在的两份 Java**（低的那份当全局默认、高的那份是这个版本的要求），
/// 于是「装完之后的情形」（机器上有一份满足要求的 Java）被如实地摆了出来，不必真装一个 JDK。
///
/// 抓得住：把全局默认无条件当成要用的那一份（`pick_java` 只喂 `effective.java_path`）——
/// 那时即使机器上有满足要求的 Java，点启动依旧被拦，本用例红。
/// 抓不住：真去下载一个 JDK 再把路径扫出来（那一段由③与 `install` 那条通道守）。
#[test]
fn a_global_default_that_cannot_run_the_version_falls_back_to_a_satisfying_java() {
    // 两份**大版本不同**的真 Java：小的那份当全局默认，大的那份是版本要求。
    let mut found: Vec<unlucky_core::game::java::JavaRuntime> =
        unlucky_core::game::java::system_candidates()
            .into_iter()
            .filter(|runtime| runtime.major.is_some())
            .collect();
    found.sort_by_key(|runtime| runtime.major);
    let (Some(low), Some(high)) = (found.first().cloned(), found.last().cloned()) else {
        eprintln!("跳过：本机没有 Java 运行时");
        return;
    };
    let (low_major, Some(high_major)) = (low.major.unwrap_or(0), high.major) else {
        eprintln!("跳过：探测不到主版本号");
        return;
    };
    if low_major >= high_major {
        eprintln!("跳过：本机只有同一个大版本的 Java（{low_major}）");
        return;
    }

    // 版本的 Java 要求 = 高的那个大版本（写进版本本体的 `javaVersion`）。
    let version = "m334-needs-newer-java";
    let demo = profile("demo", version);
    let id = demo.id.as_str().to_string();
    let (mut host, _root) = host_unpinned("m334-fallback", std::slice::from_ref(&demo));
    write_version_requiring(host.env(), version, high_major);
    // witness：要求真的是「不低于 high」，而全局默认那份（low）确实不满足它。
    let required = unlucky_core::game::java::requirement_for_version(&host.env().game, version);
    assert_eq!(
        required.minimum, high_major,
        "版本本体的 javaVersion 要生效"
    );
    assert!(
        unlucky_core::game::java::check(low_major, &required).is_some(),
        "全局默认那份必须**不满足**要求（{low_major} < {high_major}）"
    );
    assert!(
        unlucky_core::game::java::check(high_major, &required).is_none(),
        "高的那份必须满足要求"
    );

    // 用户把 low 设成全局默认（产品那条命令）。
    host.dispatch(AppCommand::SetSetting {
        key: "java.selected_path".to_string(),
        value: low.binary.display().to_string(),
    });
    let _ = host.drain_events();
    assert_eq!(
        launch_args::global_defaults(host.env())
            .java_path
            .as_deref(),
        Some(low.binary.as_path()),
        "前置：全局默认就是那份跑不了这个版本的 Java"
    );

    land_snapshot(&mut host);
    host.dispatch(AppCommand::LaunchInstance(id.clone()));
    let events = host.drain_events();
    assert!(
        blocked_event(&events).is_none(),
        "机器上已经有满足要求的 Java {high_major}（这就是「装完之后」的样子），不许还拿全局默认的 {low_major} 去撞：{events:?}"
    );
    assert!(
        runtime_event_emitted(&events),
        "过闸就该照常受理（同一拍 Busy）：{events:?}"
    );
    assert!(host.in_flight(&id), "受理 = 标 pending");
    settle(&mut host);
}

/// 写一份「要求某个 Java 大版本」的版本本体（`javaVersion.majorVersion`）。
fn write_version_requiring(env: &crate::host::env::HostEnv, version: &str, major: u32) {
    let path = env.game.version_json(version);
    std::fs::create_dir_all(path.parent().expect("版本目录")).expect("版本目录");
    std::fs::write(
        &path,
        format!(
            r#"{{"id":"{version}","type":"release","mainClass":"net.minecraft.client.main.Main","javaVersion":{{"majorVersion":{major}}}}}"#
        ),
    )
    .expect("写版本本体");
}
