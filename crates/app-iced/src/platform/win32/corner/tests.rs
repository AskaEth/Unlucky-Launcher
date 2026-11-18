//! `win32::corner` 的单测（M116 从 `corner.rs` 搬出来：那个文件要守 400 行上限，
//! 而日志行的形状断言本身就有几十行）。
//!
//! 判据与真实读数的出处：`DevDocs/artifacts/m116/probe-table.md`。
//!
//! ## M182 的两条纪律（都来自复审 P1-1，别再退回原样）
//!
//! 1. **判据断言一律走 [`plan_corner_for`]，平台显式传值**。`plan_corner` 是**产品入口**，
//!    它读的是 `HostOs::current()`——同一个测试在 Windows 上绿、在 ubuntu 上就可能红。
//!    M182 首轮复审实测：本机（Windows）384+ 条全绿，而其中三条断言的是 Windows 的结果，
//!    合进 main 后 `ubuntu-latest` 的 `test` job 会当场红。
//! 2. **`plan_corner` 只留一条用例**，且那条必须**与宿主无关**：
//!    [`the_production_entry_forwards_to_the_pure_judgement`] 两边都用同一个 `HostOs::current()`，
//!    所以它在任何平台上都成立，只证「产品入口确实转发到纯函数」。
//!
//! 「非 Windows」那一组断言住在 [`platforms`]（本文件守 400 行上限）。

use super::{format_line, plan_corner, plan_corner_for, CornerExclusion, CornerPlan, CornerSource};
use crate::platform::host::HostOs;
use crate::Backend;
use unlucky_presenter::prefs::{WindowCorner, WindowsVersion, WIN11_MIN_BUILD};

mod platforms;

/// 本机实测版本（Server 2025 / build 26100）。
fn win11() -> WindowsVersion {
    WindowsVersion::new(10, 0, 26_100)
}

/// Win10 22H2（build 19045）：DWM 没有圆角偏好属性。
fn win10() -> WindowsVersion {
    WindowsVersion::new(10, 0, 19_045)
}

/// 判据断言用的固定宿主：**永远是 Windows**，与本机实际跑在哪个系统上无关。
///
/// 为什么要有这个包装而不是直接写 `HostOs::Windows`：写一次名字就够了，以后想给 Linux /
/// macOS 也加 Windows 专属断言时，改这一处即可。
fn on_windows(policy: WindowCorner, os: Option<WindowsVersion>, backend: Backend) -> CornerPlan {
    plan_corner_for(policy, HostOs::Windows, os, backend)
}

/// **抓得住**：判定链的顺序与每一格的原因码（把 `policy`/`host`/`os`/`backend` 任一条判反都会红）。
/// **抓不住**：DWM 到底有没有真画圆角（那要像素，见 `platform/win32` 的探针输出）。
#[test]
fn truth_table_policy_host_os_backend() {
    let cases = [
        // policy=Square：任何平台/系统/后端都直角，原因是用户选的。
        (
            WindowCorner::Square,
            HostOs::Windows,
            Some(win11()),
            Backend::Gpu,
            CornerSource::Square,
            Some(CornerExclusion::UserSquare),
        ),
        (
            WindowCorner::Square,
            HostOs::Linux,
            None,
            Backend::Cpu,
            CornerSource::Square,
            Some(CornerExclusion::UserSquare),
        ),
        // policy=System + Windows 上探测不出系统：不许猜「Win11」。
        (
            WindowCorner::System,
            HostOs::Windows,
            None,
            Backend::Gpu,
            CornerSource::Square,
            Some(CornerExclusion::UnknownOs),
        ),
        // Win10：直角，GPU 档原因是版本、CPU 档原因是渲染档（同一个直角，原因不同）。
        (
            WindowCorner::System,
            HostOs::Windows,
            Some(win10()),
            Backend::Gpu,
            CornerSource::Square,
            Some(CornerExclusion::OsBuild),
        ),
        (
            WindowCorner::System,
            HostOs::Windows,
            Some(win10()),
            Backend::Auto,
            CornerSource::Square,
            Some(CornerExclusion::OsBuild),
        ),
        (
            WindowCorner::System,
            HostOs::Windows,
            Some(win10()),
            Backend::Cpu,
            CornerSource::Square,
            Some(CornerExclusion::CpuTier),
        ),
        // Win11 / Server 2025：三个后端都用 DWM（D36 只管我们自己的合成技巧）。
        (
            WindowCorner::System,
            HostOs::Windows,
            Some(win11()),
            Backend::Gpu,
            CornerSource::Dwm,
            None,
        ),
        (
            WindowCorner::System,
            HostOs::Windows,
            Some(win11()),
            Backend::Cpu,
            CornerSource::Dwm,
            None,
        ),
        // macOS（M182）：圆角归窗口服务器，但我们交的是 borderless 窗 ⇒ 它不裁。
        (
            WindowCorner::System,
            HostOs::MacOs,
            None,
            Backend::Gpu,
            CornerSource::Compositor,
            Some(CornerExclusion::BorderlessFrame),
        ),
        // Linux（M182）：CSD 窗没人裁，自绘被 D46 排除。
        (
            WindowCorner::System,
            HostOs::Linux,
            None,
            Backend::Gpu,
            CornerSource::Compositor,
            Some(CornerExclusion::ClientSideFrame),
        ),
        // 不认识的平台：不承诺。
        (
            WindowCorner::System,
            HostOs::Other,
            None,
            Backend::Gpu,
            CornerSource::Square,
            Some(CornerExclusion::Unsupported),
        ),
    ];

    for (policy, host, os, backend, want_source, want_excluded) in cases {
        let plan = plan_corner_for(policy, host, os, backend);
        assert_eq!(
            plan.source, want_source,
            "policy={policy:?} host={host:?} os={os:?} backend={backend:?}"
        );
        assert_eq!(
            plan.excluded, want_excluded,
            "policy={policy:?} host={host:?} os={os:?} backend={backend:?} 的原因码"
        );
        assert_eq!(
            plan.host, host,
            "判定必须把输入的平台原样带回来（日志要用）"
        );
    }
}

/// 边界：`WIN11_MIN_BUILD` **本身**算 Win11（`>=`，不是 `>`）。
///
/// 抓得住：把 `build < WIN11_MIN_BUILD` 写成 `<=`——Win11 21H2（build 22000）会静默掉成直角，
/// 而那正好是第一台 Win11 机器。
#[test]
fn the_win11_boundary_is_inclusive() {
    let at = WindowsVersion::new(10, 0, WIN11_MIN_BUILD);
    assert_eq!(
        on_windows(WindowCorner::System, Some(at), Backend::Gpu).source,
        CornerSource::Dwm
    );
    let below = WindowsVersion::new(10, 0, WIN11_MIN_BUILD - 1);
    assert_eq!(
        on_windows(WindowCorner::System, Some(below), Backend::Gpu).source,
        CornerSource::Square,
        "build 21999 = Win10 22H2 家族末尾，不能算 Win11"
    );
}

/// 日志行的形状：五个字段都在，且 `excluded_reason=none` 与 `reason=user` 不许混。
///
/// 为什么要钉这一条：这行是「后来的人分辨没生效 / 生效了看不见」的唯一依据（任务书原话），
/// 少一栏就等于把这条线索丢掉。M182 新增的 `host=` 与「非 Windows 的 `os=n/a`」在这里钉死。
#[test]
fn the_log_line_carries_every_field() {
    let dwm = on_windows(WindowCorner::System, Some(win11()), Backend::Gpu)
        .log_line(WindowCorner::System);
    assert!(dwm.starts_with("corner=policy=system"), "{dwm}");
    assert!(dwm.contains("host=windows"), "{dwm}");
    assert!(dwm.contains("source=dwm"), "{dwm}");
    assert!(dwm.contains("os=10.0.26100"), "{dwm}");
    assert!(dwm.contains("excluded_reason=none"), "{dwm}");
    assert!(dwm.contains("radius=system"), "{dwm}");

    let square = on_windows(WindowCorner::Square, Some(win11()), Backend::Gpu)
        .log_line(WindowCorner::Square);
    assert!(square.contains("source=square"), "{square}");
    assert!(square.contains("excluded_reason=user"), "{square}");

    // Windows 上「版本读不到」：`os=unknown` **配上** `host=windows` 才是完整的真话。
    let unknown =
        on_windows(WindowCorner::System, None, Backend::Gpu).log_line(WindowCorner::System);
    assert!(unknown.contains("host=windows"), "{unknown}");
    assert!(unknown.contains("os=unknown"), "{unknown}");
    assert!(unknown.contains("excluded_reason=unknown-os"), "{unknown}");
}

/// **日志行的形状**，用 M116 探针窗口的**真实读数**构造（`DevDocs/artifacts/m116/probe-table.md`）。
///
/// 抓得住：字段被删/改名/顺序错乱（`excluded_reason` 少一栏、`radius=system` 丢了、
/// `frame` / `layered` 被挪出这一行）——这些都会让「没生效 vs 生效了看不见」的区分重新变成猜谜。
/// 抓不住：**产品进程**里那一行（本测试只钉格式）。真值已有：2026-11-04 两次真跑产品 bin
/// （α=1.0 / α=0.85）的 `[corner] boot …` 两行，见 `DevDocs/artifacts/m116/README.md`。
#[test]
fn the_log_line_shape_is_pinned_with_the_measured_readings() {
    // 本机实测：Server 2025 / build 26100；读回 `E_INVALIDARG`；产品形状是 `overlapped`，
    // 而 `layered=on` 只可能来自整窗 α（我们的 `WS_EX_LAYERED`） ⇒ 带 `suppressed_by=`。
    let os = WindowsVersion::new(10, 0, 26_100);
    let plan: CornerPlan = on_windows(WindowCorner::System, Some(os), Backend::Gpu);
    let line = format_line(
        "boot",
        &plan,
        WindowCorner::System,
        "pref=2 readback=? read_hr=0x80070057",
        "frame=overlapped layered=on",
        Some("window_alpha"),
    );
    assert_eq!(
        line,
        concat!(
            "[corner] boot corner=policy=system host=windows source=dwm os=10.0.26100 ",
            "excluded_reason=none radius=system pref=2 readback=? ",
            "read_hr=0x80070057 frame=overlapped layered=on ",
            "suppressed_by=window_alpha"
        )
    );

    // 设置里选「直角」：原因码必须从 `none` 变成 `user`，其余字段不动。
    let square = format_line(
        "change",
        &on_windows(WindowCorner::Square, Some(os), Backend::Gpu),
        WindowCorner::Square,
        "pref=1 readback=? read_hr=0x80070057",
        "frame=overlapped layered=off",
        None,
    );
    assert!(
        square.contains("corner=policy=square host=windows source=square"),
        "{square}"
    );
    assert!(square.contains("excluded_reason=user"), "{square}");
    assert!(square.contains("frame=overlapped layered=off"), "{square}");
    // α = 1（不补分层）时**不许**出现 `suppressed_by=`：那一栏只在整窗 α 打开时才有意义。
    assert!(!square.contains("suppressed_by"), "{square}");

    // Win10：原因码是 `os-build`（GPU 档）/ `cpu-tier`（CPU 档），来源仍是 square。
    let win10 = WindowsVersion::new(10, 0, 19_045);
    let os_build =
        on_windows(WindowCorner::System, Some(win10), Backend::Gpu).log_line(WindowCorner::System);
    assert!(
        os_build.contains("source=square") && os_build.contains("excluded_reason=os-build"),
        "{os_build}"
    );
    let cpu =
        on_windows(WindowCorner::System, Some(win10), Backend::Cpu).log_line(WindowCorner::System);
    assert!(cpu.contains("excluded_reason=cpu-tier"), "{cpu}");
}

/// **产品入口只有一条用例**，且它**与宿主无关**（复审 P1-1 的要求）。
///
/// 断言的是「`plan_corner` 就是 `plan_corner_for(HostOs::current(), …)`」：两边喂同一个
/// `HostOs::current()`，所以在 Windows / ubuntu / macOS 上恒成立——它证明的是**转发**，
/// 不是某个平台的结果。任何一个平台的结果都由 [`truth_table_policy_host_os_backend`]
/// 显式传 `HostOs` 钉住。
///
/// **抓得住**：`plan_corner` 里偷偷改成写死的 `HostOs::Windows`（那样 ubuntu 上这条会红）、
/// 或者干脆不再转发（判据分叉）。
#[test]
fn the_production_entry_forwards_to_the_pure_judgement() {
    for policy in [WindowCorner::System, WindowCorner::Square] {
        for os in [None, Some(win10()), Some(win11())] {
            for backend in [Backend::Gpu, Backend::Cpu, Backend::Auto] {
                let production = plan_corner(policy, os, backend);
                assert_eq!(
                    production,
                    plan_corner_for(policy, HostOs::current(), os, backend),
                    "plan_corner 必须原样转发到纯函数（policy={policy:?} os={os:?} backend={backend:?}）"
                );
                assert_eq!(production.host, HostOs::current());
            }
        }
    }
}

/// 每个原因码都是一个**非空、无空格、互不重复**的稳定串（复审 P2-1：`unknown-os` 此前没有任何断言）。
///
/// 为什么值得一条表测试：这些串是产品日志里 `excluded_reason=` 的唯一取值，而它们只在
/// **构建这样一张表**的时候才会被逐个过一遍——散在断言里时，`unknown-os` 这种「只在一台
/// Windows 机器、且版本探测失败时才出现」的值最容易一直没有断言（复审就是这么抓到的）。
///
/// **抓得住**：两个原因码共用一个串（日志再也分不清原因）、串里混进空格（那一栏会被截断）、
/// 或者某一条被改成空串。
#[test]
fn every_exclusion_code_is_a_stable_non_empty_string() {
    use std::collections::BTreeSet;

    let codes = [
        (CornerExclusion::UserSquare, "user"),
        (CornerExclusion::OsBuild, "os-build"),
        (CornerExclusion::UnknownOs, "unknown-os"),
        (CornerExclusion::CpuTier, "cpu-tier"),
        (CornerExclusion::BorderlessFrame, "borderless"),
        (CornerExclusion::ClientSideFrame, "client-side"),
        (CornerExclusion::Unsupported, "unsupported"),
    ];

    let mut seen = BTreeSet::new();
    for (reason, code) in codes {
        assert_eq!(reason.as_str(), code, "{reason:?} 的日志取值");
        assert!(
            !code.is_empty() && !code.contains(char::is_whitespace),
            "原因码必须是单个词（日志按空格分栏）：{code:?}"
        );
        assert!(seen.insert(code), "两个原因码共用了同一个串：{code}");
    }
    assert_eq!(seen.len(), 7, "目前共七个原因码，少一个就是漏了断言");
}

/// **同一策略 N 次只下发 / 只记 1 行**（M116 r1 复审 P2-4 的判据）。
///
/// 为什么要有这条：设置页的整窗 α / 面板 α / 暗化滑条每动一下都会触发一次外观变更，
/// 而圆角与它们无关。没有去重时拖一次滑条就会刷出一串 `[corner] change …`，
/// 把「分辨有没有生效」的那唯一一行日志淹掉。
///
/// 抓得住：把去重删掉（`needs_redispatch` 恒 `true` 时这里会数到 N）、或指纹里漏掉某一维
/// （换档 / 首次拿到句柄就不重下了）。
/// 抓不住：真进程里到底打了几行（`eprintln!` 在 `App` 里，本测试只钉判定）。
#[test]
fn the_same_policy_is_dispatched_only_once() {
    use super::{needs_redispatch, CornerApplied};

    let os = WindowsVersion::new(10, 0, 26_100);
    let plan = on_windows(WindowCorner::System, Some(os), Backend::Gpu);

    // 模拟「滑条被拖动 20 次」：每次都是同一个策略、同一个系统、句柄一直在。
    let mut last: Option<CornerApplied> = None;
    let mut dispatches = 0;
    for _ in 0..20 {
        let next = CornerApplied::new(WindowCorner::System, &plan, true, false);
        if needs_redispatch(last.as_ref(), &next) {
            dispatches += 1;
            last = Some(next);
        }
    }
    assert_eq!(dispatches, 1, "同一策略连按 20 次只该下发/记 1 次");

    // 换档必须重下。
    let square = on_windows(WindowCorner::Square, Some(os), Backend::Gpu);
    assert!(needs_redispatch(
        last.as_ref(),
        &CornerApplied::new(WindowCorner::Square, &square, true, false)
    ));

    // 句柄从无到有必须重下（首次拿到 HWND 时才真能设属性）。
    let mut no_window: Option<CornerApplied> = None;
    let first = CornerApplied::new(WindowCorner::System, &plan, false, false);
    assert!(needs_redispatch(no_window.as_ref(), &first));
    no_window = Some(first);
    assert!(needs_redispatch(
        no_window.as_ref(),
        &CornerApplied::new(WindowCorner::System, &plan, true, false)
    ));

    // 整窗 α 拉低（补分层）与拉回 1.0 都必须各重下/各记一行：系统行为在这两次确实不同。
    let opaque = CornerApplied::new(WindowCorner::System, &plan, true, false);
    let translucent = CornerApplied::new(WindowCorner::System, &plan, true, true);
    assert!(
        needs_redispatch(Some(&opaque), &translucent),
        "α 从 1.0 拉低必须重下（那一行才会出现 suppressed_by=window_alpha）"
    );
    assert!(
        needs_redispatch(Some(&translucent), &opaque),
        "α 拉回 1.0 也必须重下（圆角请求重新生效）"
    );
    assert!(
        !needs_redispatch(Some(&translucent), &translucent),
        "α 一直在 <1 时重复触发不该刷屏"
    );
}
