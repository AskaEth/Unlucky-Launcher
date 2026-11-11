//! `transparency::dwm` 的单测（M182 从 `dwm.rs` 内联挪出来：那个文件贴着 400 行上限，
//! 而 M182 要往 `preference_for` 里加第三档映射）。
//!
//! 挪动是**逐行照搬** + M182 的新断言，没有重写判据。

use super::{
    apply_corner, corner_preference_hr, preference_for, CornerOutcome,
    DWMWA_VISIBLE_FRAME_BORDER_THICKNESS, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
    DWMWCP_ROUND,
};
use crate::platform::host::HostOs;
use crate::platform::win32::corner::{CornerPlan, CornerSource};
use unlucky_presenter::prefs::WindowsVersion;

/// **抓得住**：属性号写错（34 写成 33 / 37 写成 36 —— 这两个都在 Win11 上存在，改错不会崩，
/// 只会静默读出别的东西）、两个偏好值互换。
/// **抓不住**：这些值在真窗口上是否真的生效（那要探针读数，见 `probe` 与报告）。
#[test]
fn attribute_numbers_follow_the_win32_header() {
    assert_eq!(DWMWA_WINDOW_CORNER_PREFERENCE, 34);
    assert_eq!(DWMWA_VISIBLE_FRAME_BORDER_THICKNESS, 37);
    assert_eq!(DWMWCP_ROUND, 2);
    assert_eq!(DWMWCP_DONOTROUND, 1);
}

/// **抓得住**：`preference_for` 把圆角/直角映射反了（这一格反了就是「设置里选直角反而更圆」）。
#[test]
fn the_preference_mapping_cannot_be_swapped() {
    assert_eq!(preference_for(CornerSource::Dwm), DWMWCP_ROUND);
    assert_eq!(preference_for(CornerSource::Square), DWMWCP_DONOTROUND);
}

/// M182 的第三档：`Compositor`（跟随合成器）在 Windows 上**不会出现**（见
/// `plan_corner_for` 的真值表），所以这里钉两件事——① match 必须写全、不许 panic；
/// ② 真要出现时它落到 `DWMWCP_DEFAULT`（`0` = 让 DWM 自己决定），而不是被偷偷当成直角。
///
/// **抓得住**：有人把这一臂写成 `DWMWCP_DONOTROUND`（那就成了「说跟随合成器、实际强制直角」，
/// 与 `source=compositor` 那行日志自相矛盾）。
#[test]
fn the_compositor_tier_maps_to_letting_the_system_decide() {
    assert_eq!(
        preference_for(CornerSource::Compositor),
        0,
        "Compositor 档必须落到 DWMWCP_DEFAULT(0)，不是 DONOTROUND"
    );
    // 且它在 Windows 的判据里不可达（真值表不会产出它）。
    for os in [None, Some(WindowsVersion::new(10, 0, 26_100))] {
        for backend in [
            crate::Backend::Gpu,
            crate::Backend::Cpu,
            crate::Backend::Auto,
        ] {
            let plan = crate::platform::win32::corner::plan_corner_for(
                unlucky_presenter::prefs::WindowCorner::System,
                HostOs::Windows,
                os,
                backend,
            );
            assert_ne!(plan.source, CornerSource::Compositor, "{plan:?}");
        }
    }
}

/// 无效句柄：**不下发**、也不 panic。句柄 0 走 `NoWindow`（还没拿到窗口 ≠ 平台不支持）。
#[test]
fn a_missing_window_is_reported_not_guessed() {
    let plan = CornerPlan {
        source: CornerSource::Dwm,
        excluded: None,
        host: HostOs::Windows,
        os: Some(WindowsVersion::new(10, 0, 26_100)),
    };
    assert_eq!(apply_corner(0, &plan), CornerOutcome::NoWindow);
    assert_eq!(corner_preference_hr(0).0, None);
    // 1 号句柄永远无效：非 Windows 上是「平台不支持」，Windows 上是 DWM 的失败码，
    // 两种都不许表现成「成功」。
    let outcome = apply_corner(1, &plan);
    assert!(
        matches!(
            outcome,
            CornerOutcome::Failed { .. }
                | CornerOutcome::Unsupported { .. }
                | CornerOutcome::UnsupportedPlatform
        ),
        "无效句柄应报失败，实际 {outcome:?}"
    );
}

/// 日志尾巴三态可区分：设了 / 设了但读不回 / 属性不支持。
#[test]
fn the_log_tail_separates_applied_from_unsupported() {
    let applied = CornerOutcome::Applied {
        preference: DWMWCP_ROUND,
        readback: Some(DWMWCP_ROUND),
        read_hresult: 0,
    };
    assert!(
        applied.describe().starts_with("pref=2 readback=2"),
        "{applied:?}"
    );

    let blind = CornerOutcome::Applied {
        preference: DWMWCP_ROUND,
        readback: None,
        read_hresult: 0x8000_4005,
    };
    assert!(
        blind.describe().contains("readback=?"),
        "{}",
        blind.describe()
    );

    let old = CornerOutcome::Unsupported {
        hresult: 0x8007_0057,
    };
    assert!(
        old.describe().contains("attribute unsupported"),
        "{}",
        old.describe()
    );
    assert!(CornerOutcome::UnsupportedPlatform
        .describe()
        .contains("platform=unsupported"));
}
