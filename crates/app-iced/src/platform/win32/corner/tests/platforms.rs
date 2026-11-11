//! 「非 Windows」那一组断言（M182）：`HostOs` 是判据的**参数**，所以本机（Windows）也能验。
//!
//! 为什么单独成文件：`corner/tests.rs` 已经贴着 400 行上限（M182 加进来的三行平台断言
//! 正好是压垮它的那部分）。

use super::{win10, win11};
use crate::platform::host::HostOs;
use crate::platform::win32::corner::{plan_corner_for, CornerExclusion, CornerSource};
use crate::Backend;
use unlucky_presenter::prefs::WindowCorner;

const ALL_HOSTS: [HostOs; 4] = [HostOs::Windows, HostOs::MacOs, HostOs::Linux, HostOs::Other];
const ALL_BACKENDS: [Backend; 3] = [Backend::Gpu, Backend::Cpu, Backend::Auto];

/// **M182 的核心断言**：`os = None` 不再等于「未知平台」。
///
/// 同一个 `(System, None, Gpu)` 三元组，在 Windows 上是「版本读不出来」（`unknown-os`），
/// 在 macOS / Linux 上是「这一栏跟本机无关」（`borderless` / `client-side`）。把三行写成
/// 同一格就是用户看到的那句 `source=square os=unknown`——它同时糊掉了「探测失败」与
/// 「这台根本不是 Windows」两件事。
///
/// **抓得住**：谁把非 Windows 两行改回 `Square`/`UnknownOs`（或把 `host` 从判据里摘掉）。
#[test]
fn a_missing_windows_version_does_not_mean_unknown_platform() {
    let rows: Vec<_> = [HostOs::Windows, HostOs::MacOs, HostOs::Linux]
        .into_iter()
        .map(|host| {
            let plan = plan_corner_for(WindowCorner::System, host, None, Backend::Gpu);
            (host, plan.source, plan.excluded)
        })
        .collect();

    assert_eq!(
        rows,
        vec![
            (
                HostOs::Windows,
                CornerSource::Square,
                Some(CornerExclusion::UnknownOs)
            ),
            (
                HostOs::MacOs,
                CornerSource::Compositor,
                Some(CornerExclusion::BorderlessFrame)
            ),
            (
                HostOs::Linux,
                CornerSource::Compositor,
                Some(CornerExclusion::ClientSideFrame)
            ),
        ]
    );
}

/// 非 Windows 的日志行必须自己解释完：有 `host=`、`os=n/a`（不是 `unknown`）、原因码说人话。
///
/// **抓得住**：把 `os=` 在非 Windows 上也写成 `unknown`——那会让人去找一个不存在的注册表项。
#[test]
fn the_non_windows_log_line_explains_itself() {
    let mac = plan_corner_for(WindowCorner::System, HostOs::MacOs, None, Backend::Gpu)
        .log_line(WindowCorner::System);
    assert!(mac.contains("host=macos"), "{mac}");
    assert!(mac.contains("os=n/a"), "{mac}");
    assert!(!mac.contains("os=unknown"), "{mac}");
    assert!(mac.contains("source=compositor"), "{mac}");
    assert!(mac.contains("excluded_reason=borderless"), "{mac}");

    let linux = plan_corner_for(WindowCorner::System, HostOs::Linux, None, Backend::Cpu)
        .log_line(WindowCorner::System);
    assert!(linux.contains("host=linux"), "{linux}");
    assert!(linux.contains("os=n/a"), "{linux}");
    assert!(linux.contains("excluded_reason=client-side"), "{linux}");

    // Windows 上 `os=unknown` 仍要保留它本来的意思：真的没探测到版本。
    let win = plan_corner_for(WindowCorner::System, HostOs::Windows, None, Backend::Gpu)
        .log_line(WindowCorner::System);
    assert!(
        win.contains("host=windows") && win.contains("os=unknown"),
        "{win}"
    );
}

/// macOS / Linux 两行**不看渲染档**：我们什么都不下发，也就没有「CPU 档要不要让步」这回事。
///
/// **抓得住**：有人按 Windows 的形状给这两行加 `backend` 分支（那会变成凭空造的降级）。
#[test]
fn the_non_windows_rows_ignore_the_backend() {
    for host in [HostOs::MacOs, HostOs::Linux] {
        let plans: Vec<_> = ALL_BACKENDS
            .into_iter()
            .map(|backend| plan_corner_for(WindowCorner::System, host, None, backend))
            .collect();
        for plan in &plans {
            assert_eq!(
                (plan.source, plan.excluded),
                (plans[0].source, plans[0].excluded),
                "{host:?}：三个渲染档必须得到同一份判定（实际 {plan:?}）"
            );
        }
    }
}

/// macOS 与 Linux **不许共用同一个原因码**：两边的机制不同（缺 `Titled` 位 ≠ 协议里没有圆角）。
///
/// **抓得住**：复制粘贴把 Linux 行也标成 `borderless`（那会让以后看日志的人去研究
/// `NSWindowStyleMask`，而 Linux 上根本没有这个东西）。
#[test]
fn macos_and_linux_report_different_reasons() {
    let mac = plan_corner_for(WindowCorner::System, HostOs::MacOs, None, Backend::Gpu);
    let linux = plan_corner_for(WindowCorner::System, HostOs::Linux, None, Backend::Gpu);
    assert_ne!(mac.excluded, linux.excluded);
    assert_eq!(mac.excluded, Some(CornerExclusion::BorderlessFrame));
    assert_eq!(linux.excluded, Some(CornerExclusion::ClientSideFrame));
    // 两行都不是「我们要求 DWM 画」，而是「决定权在合成器」（`is_compositor` / `is_dwm`
    // 这两个小判据的用处就在这里：以后加平台时先问一句「这一档算不算合成器档」）。
    assert!(mac.source.is_compositor() && linux.source.is_compositor());
    assert!(!mac.source.is_dwm() && !linux.source.is_dwm());
}

/// `Compositor` 这一档**不许**在 Windows 上出现；`Dwm` 也**不许**在非 Windows 上出现。
///
/// **抓得住**：「非 Windows 一律 compositor」写成「一律不 square」这类图省事的判定，
/// 或者反过来把 Windows 的 DWM 判据搬去 Linux 假装能用（M182 任务书点名禁止）。
#[test]
fn each_source_stays_on_its_own_platforms() {
    for os in [None, Some(win10()), Some(win11())] {
        for backend in ALL_BACKENDS {
            let win = plan_corner_for(WindowCorner::System, HostOs::Windows, os, backend);
            assert_ne!(
                win.source,
                CornerSource::Compositor,
                "Windows 上不许出现 compositor 档：{win:?}"
            );
        }
    }
    for host in [HostOs::MacOs, HostOs::Linux, HostOs::Other] {
        for backend in ALL_BACKENDS {
            // 非 Windows 传一个「看起来像 Win11」的版本号进去：判据**不许**被它带偏。
            let plan = plan_corner_for(WindowCorner::System, host, Some(win11()), backend);
            assert_ne!(
                plan.source,
                CornerSource::Dwm,
                "{host:?} 上没有 DWM：{plan:?}"
            );
        }
    }
}

/// `source` 与 `excluded` 不许互相撒谎：说「从合成器来」就必须说清为什么没来。
///
/// **抓得住**：把 macOS/Linux 写成 `source=compositor excluded_reason=none`——那行的字面意思是
/// 「已经按合成器圆了」，而用户屏幕上仍是直角（正是 M182 要修的那种误读）。
#[test]
fn the_source_never_claims_a_corner_that_the_exclusion_denies() {
    for policy in [WindowCorner::System, WindowCorner::Square] {
        for host in ALL_HOSTS {
            for os in [None, Some(win10()), Some(win11())] {
                for backend in ALL_BACKENDS {
                    let plan = plan_corner_for(policy, host, os, backend);
                    match plan.source {
                        // 真下发了 DWM 偏好时不该有原因码。
                        CornerSource::Dwm => assert_eq!(plan.excluded, None, "{plan:?}"),
                        // 合成器档在当前实现里**永远**带原因码（没有任何平台真拿到系统圆角）。
                        CornerSource::Compositor => assert!(
                            plan.excluded.is_some(),
                            "source=compositor 必须交代原因：{plan:?}"
                        ),
                        CornerSource::Square => assert!(plan.excluded.is_some(), "{plan:?}"),
                    }
                }
            }
        }
    }
}

/// 用户选「直角」必须在**每一个**平台上都赢（包括将来真的迁移了标题栏的 macOS）。
///
/// **抓得住**：把 `policy == Square` 的短路写在 `match host` 之后或写进某个平台分支里。
#[test]
fn the_user_square_choice_wins_on_every_platform() {
    for host in ALL_HOSTS {
        for backend in ALL_BACKENDS {
            let plan = plan_corner_for(WindowCorner::Square, host, Some(win11()), backend);
            assert_eq!(plan.source, CornerSource::Square, "{host:?} {backend:?}");
            assert_eq!(plan.excluded, Some(CornerExclusion::UserSquare));
        }
    }
}
