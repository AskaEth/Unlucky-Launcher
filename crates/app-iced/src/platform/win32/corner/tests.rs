//! `win32::corner` 的单测（M116 从 `corner.rs` 搬出来：那个文件要守 400 行上限，
//! 而日志行的形状断言本身就有几十行）。
//!
//! 判据与真实读数的出处：`DevDocs/artifacts/m116/probe-table.md`。

use super::{format_line, plan_corner, CornerExclusion, CornerPlan, CornerSource};
use crate::Backend;
use unlucky_presenter::prefs::{WindowCorner, WindowsVersion, WIN11_MIN_BUILD};

/// 本机实测版本（Server 2025 / build 26100）。
fn win11() -> WindowsVersion {
    WindowsVersion::new(10, 0, 26_100)
}

/// Win10 22H2（build 19045）：DWM 没有圆角偏好属性。
fn win10() -> WindowsVersion {
    WindowsVersion::new(10, 0, 19_045)
}

/// **抓得住**：判定链的顺序与每一格的原因码（把 `policy`/`os`/`backend` 任一条判反都会红）。
/// **抓不住**：DWM 到底有没有真画圆角（那要像素，见 `platform/win32` 的探针输出）。
#[test]
fn truth_table_policy_os_backend() {
    let cases = [
        // policy=Square：任何系统/后端都直角，原因是用户选的。
        (
            WindowCorner::Square,
            Some(win11()),
            Backend::Gpu,
            CornerSource::Square,
            Some(CornerExclusion::UserSquare),
        ),
        (
            WindowCorner::Square,
            Some(win10()),
            Backend::Cpu,
            CornerSource::Square,
            Some(CornerExclusion::UserSquare),
        ),
        (
            WindowCorner::Square,
            None,
            Backend::Auto,
            CornerSource::Square,
            Some(CornerExclusion::UserSquare),
        ),
        // policy=System + 探测不出系统：不许猜「Win11」。
        (
            WindowCorner::System,
            None,
            Backend::Gpu,
            CornerSource::Square,
            Some(CornerExclusion::UnknownOs),
        ),
        // Win10：直角，GPU 档原因是版本、CPU 档原因是渲染档（同一个直角，原因不同）。
        (
            WindowCorner::System,
            Some(win10()),
            Backend::Gpu,
            CornerSource::Square,
            Some(CornerExclusion::OsBuild),
        ),
        (
            WindowCorner::System,
            Some(win10()),
            Backend::Auto,
            CornerSource::Square,
            Some(CornerExclusion::OsBuild),
        ),
        (
            WindowCorner::System,
            Some(win10()),
            Backend::Cpu,
            CornerSource::Square,
            Some(CornerExclusion::CpuTier),
        ),
        // Win11 / Server 2025：三个后端都用 DWM（D36 只管我们自己的合成技巧）。
        (
            WindowCorner::System,
            Some(win11()),
            Backend::Gpu,
            CornerSource::Dwm,
            None,
        ),
        (
            WindowCorner::System,
            Some(win11()),
            Backend::Cpu,
            CornerSource::Dwm,
            None,
        ),
        (
            WindowCorner::System,
            Some(win11()),
            Backend::Auto,
            CornerSource::Dwm,
            None,
        ),
    ];

    for (policy, os, backend, want_source, want_excluded) in cases {
        let plan = plan_corner(policy, os, backend);
        assert_eq!(
            plan.source, want_source,
            "policy={policy:?} os={os:?} backend={backend:?}"
        );
        assert_eq!(
            plan.excluded, want_excluded,
            "policy={policy:?} os={os:?} backend={backend:?} 的原因码"
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
        plan_corner(WindowCorner::System, Some(at), Backend::Gpu).source,
        CornerSource::Dwm
    );
    let below = WindowsVersion::new(10, 0, WIN11_MIN_BUILD - 1);
    assert_eq!(
        plan_corner(WindowCorner::System, Some(below), Backend::Gpu).source,
        CornerSource::Square,
        "build 21999 = Win10 22H2 家族末尾，不能算 Win11"
    );
}

/// 日志行的形状：四个字段都在，且 `excluded_reason=none` 与 `reason=user` 不许混。
///
/// 为什么要钉这一条：这行是「后来的人分辨没生效 / 生效了看不见」的唯一依据（任务书原话），
/// 少一栏就等于把这条线索丢掉。
#[test]
fn the_log_line_carries_every_field() {
    let dwm = plan_corner(WindowCorner::System, Some(win11()), Backend::Gpu)
        .log_line(WindowCorner::System);
    assert!(dwm.starts_with("corner=policy=system"), "{dwm}");
    assert!(dwm.contains("source=dwm"), "{dwm}");
    assert!(dwm.contains("os=10.0.26100"), "{dwm}");
    assert!(dwm.contains("excluded_reason=none"), "{dwm}");
    assert!(dwm.contains("radius=system"), "{dwm}");

    let square = plan_corner(WindowCorner::Square, Some(win11()), Backend::Gpu)
        .log_line(WindowCorner::Square);
    assert!(square.contains("source=square"), "{square}");
    assert!(square.contains("excluded_reason=user"), "{square}");

    let unknown: CornerPlan = plan_corner(WindowCorner::System, None, Backend::Gpu);
    assert!(unknown
        .log_line(WindowCorner::System)
        .contains("os=unknown"));
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
    let plan: CornerPlan = plan_corner(WindowCorner::System, Some(os), Backend::Gpu);
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
            "[corner] boot corner=policy=system source=dwm os=10.0.26100 ",
            "excluded_reason=none radius=system pref=2 readback=? ",
            "read_hr=0x80070057 frame=overlapped layered=on ",
            "suppressed_by=window_alpha"
        )
    );

    // 设置里选「直角」：原因码必须从 `none` 变成 `user`，其余字段不动。
    let square = format_line(
        "change",
        &plan_corner(WindowCorner::Square, Some(os), Backend::Gpu),
        WindowCorner::Square,
        "pref=1 readback=? read_hr=0x80070057",
        "frame=overlapped layered=off",
        None,
    );
    assert!(
        square.contains("corner=policy=square source=square"),
        "{square}"
    );
    assert!(square.contains("excluded_reason=user"), "{square}");
    assert!(square.contains("frame=overlapped layered=off"), "{square}");
    // α = 1（不补分层）时**不许**出现 `suppressed_by=`：那一栏只在整窗 α 打开时才有意义。
    assert!(!square.contains("suppressed_by"), "{square}");

    // Win10：原因码是 `os-build`（GPU 档）/ `cpu-tier`（CPU 档），来源仍是 square。
    let win10 = WindowsVersion::new(10, 0, 19_045);
    let os_build =
        plan_corner(WindowCorner::System, Some(win10), Backend::Gpu).log_line(WindowCorner::System);
    assert!(
        os_build.contains("source=square") && os_build.contains("excluded_reason=os-build"),
        "{os_build}"
    );
    let cpu =
        plan_corner(WindowCorner::System, Some(win10), Backend::Cpu).log_line(WindowCorner::System);
    assert!(cpu.contains("excluded_reason=cpu-tier"), "{cpu}");

    // 探测不到版本（macOS / X11 / Wayland 落到同一格）：`unknown-os`，不许猜 Win11。
    let unknown =
        plan_corner(WindowCorner::System, None, Backend::Gpu).log_line(WindowCorner::System);
    assert!(
        unknown.contains("os=unknown") && unknown.contains("excluded_reason=unknown-os"),
        "{unknown}"
    );
}

/// **同一策略 N 次只下发 / 只记 1 行**（M116 r1 复审 P2-4 的判据）。
///
/// 为什么要有这条：设置页的整窗 α / 面板 α / 暗化滑条每动一下都会触发一次外观变更，
/// 而圆角与它们无关。没有去重时拖一次滑条会刷出一串 `[corner] change …`，
/// 把「分辨有没有生效」的那唯一一行日志淹掉。
///
/// 抓得住：把去重删掉（`needs_redispatch` 恒 `true` 时这里会数到 N）、或指纹里漏掉某一维
/// （换档 / 首次拿到句柄就不重下了）。
/// 抓不住：真进程里到底打了几行（`eprintln!` 在 `App` 里，本测试只钉判定）。
#[test]
fn the_same_policy_is_dispatched_only_once() {
    use super::{needs_redispatch, plan_corner, CornerApplied};
    use crate::Backend;
    use unlucky_presenter::prefs::{WindowCorner, WindowsVersion};

    let os = WindowsVersion::new(10, 0, 26_100);
    let plan = plan_corner(WindowCorner::System, Some(os), Backend::Gpu);

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
    let square = plan_corner(WindowCorner::Square, Some(os), Backend::Gpu);
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
