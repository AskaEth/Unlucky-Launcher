use super::{ratio, MonitorRects, PhysicalRect};

/// 一块 1920×1080 的屏 + 底部 40 px 任务栏（Win32 的 RECT 右下边界**不含**）。
const MONITOR: PhysicalRect = PhysicalRect {
    left: 0,
    top: 0,
    right: 1920,
    bottom: 1080,
};
const WORK: PhysicalRect = PhysicalRect {
    left: 0,
    top: 0,
    right: 1920,
    bottom: 1040,
};

/// **本 mission 的核心断言**：可用区必须**严格小于**显示器尺寸（任务栏真的被扣了）。
///
/// 抓得住：把 `logical_work_area` 里的 `self.work.size()` 写成 `self.monitor.size()`（= 退回
/// M362 的行为）——`<` 与下面的差 40 px 两条都红。抓不住：FFI 是否真读到这一对矩形（那要真机）。
#[test]
fn work_area_is_strictly_smaller_than_the_monitor_when_a_taskbar_is_reserved() {
    let rects = MonitorRects {
        work: WORK,
        monitor: MONITOR,
    };
    let monitor_logical = (1920.0, 1080.0);
    let work = rects
        .logical_work_area(monitor_logical)
        .expect("100% DPI 上必须算得出可用区");

    assert_eq!(work, (1920.0, 1040.0), "可用区就是 rcWork，不是显示器尺寸");
    assert!(
        work.1 < monitor_logical.1,
        "扣掉任务栏后高度必须**严格**变小：work={work:?} monitor={monitor_logical:?}"
    );
    assert_eq!(
        monitor_logical.1 - work.1,
        40.0,
        "差值必须恰好是任务栏那 40 px（多扣或少扣都是错的）"
    );
}

/// 125% DPI：折算必须走**显示器逻辑尺寸**这条基准，任务栏在逻辑 px 下是 `40 / 1.25 = 32`。
///
/// 抓得住：拿「请求尺寸」或写死的 scale 当基准（数值会差几 px 且随屏变）。
#[test]
fn logical_conversion_follows_the_monitor_grid() {
    let rects = MonitorRects {
        work: PhysicalRect {
            right: 1366,
            bottom: 728,
            ..PhysicalRect::default()
        },
        monitor: PhysicalRect {
            right: 1366,
            bottom: 768,
            ..PhysicalRect::default()
        },
    };
    let monitor_logical = (1366.0 / 1.25, 768.0 / 1.25);
    let work = rects.logical_work_area(monitor_logical).expect("125% DPI");

    assert!((work.0 - 1092.8).abs() < 0.01, "{work:?}");
    assert!((work.1 - 582.4).abs() < 0.01, "{work:?}");
    assert!(
        (monitor_logical.1 - work.1 - 32.0).abs() < 0.01,
        "125% 下任务栏是 40 / 1.25 = 32 逻辑 px：{work:?} vs {monitor_logical:?}"
    );
}

/// 退化输入一律 `None`（**不编值**）：零/负矩形、零/负基准、非有限的逻辑尺寸。
#[test]
fn degenerate_rects_yield_none_instead_of_a_made_up_area() {
    let cases = [
        // rcWork 宽为 0（虚拟显示器适配器的经典产物）
        MonitorRects {
            work: PhysicalRect::default(),
            monitor: MONITOR,
        },
        // rcMonitor 高为 0 ⇒ 没有折算基准
        MonitorRects {
            work: WORK,
            monitor: PhysicalRect {
                right: 1920,
                ..PhysicalRect::default()
            },
        },
        // 负尺寸（RECT 左右写反）
        MonitorRects {
            work: PhysicalRect {
                left: 1920,
                right: 0,
                top: 0,
                bottom: 1040,
            },
            monitor: MONITOR,
        },
    ];
    for rects in cases {
        assert_eq!(
            rects.logical_work_area((1920.0, 1080.0)),
            None,
            "{rects:?} 不许算出一个假的可用区"
        );
    }

    // 逻辑尺寸本身非法（iced 在某些虚拟屏上会给 0）⇒ 也是 None，不许掉成 0×0
    for monitor_logical in [(0.0, 1080.0), (1920.0, f32::NAN)] {
        let rects = MonitorRects {
            work: WORK,
            monitor: MONITOR,
        };
        assert_eq!(rects.logical_work_area(monitor_logical), None);
    }
}

/// `ratio` 的两个边界（除零、负值）各一格，防止「几何判据」被一次 refactor 悄悄放宽。
#[test]
fn ratio_refuses_non_positive_inputs() {
    assert_eq!(ratio(1040, 1080), Some(1040.0 / 1080.0));
    assert_eq!(ratio(0, 1080), None, "零高等价于「没有可用区」");
    assert_eq!(ratio(1040, 0), None, "零基准等价于「算不出比例」");
    assert_eq!(ratio(-5, 1080), None);
}

/// **真机上验一次 FFI 的线是不是通的**（只在 Windows 编译）。
///
/// 抓得住：`MONITORINFO` 的字段顺序 / `cbSize` 写错 —— 那时 `GetMonitorInfoW` 返回 FALSE，
/// `probe` 会**静默**回退到显示器尺寸（本 mission 最坏的失败形态）。抓不住：任务栏是否真被扣
/// （没有任务栏的机器上两块矩形相等，所以这里只断言 `≤`；「严格小于」在纯判据那条测试里）。
#[cfg(target_os = "windows")]
#[test]
fn monitor_info_layout_is_accepted_by_windows() {
    // SAFETY: `GetDesktopWindow` 不接受参数、只返回一个常量句柄，不转移所有权。
    let desktop = unsafe { super::GetDesktopWindow() };
    assert_ne!(desktop, 0, "桌面窗口句柄不该是 0");

    let rects = super::monitor_rects(desktop).expect("GetMonitorInfoW 拒绝了这个布局");
    let (monitor_w, monitor_h) = rects.monitor.size();
    let (work_w, work_h) = rects.work.size();
    assert!(
        monitor_w > 0 && monitor_h > 0,
        "显示器全尺寸必须为正：{rects:?}"
    );
    assert!(
        work_w > 0 && work_h > 0 && work_w <= monitor_w && work_h <= monitor_h,
        "工作区必须落在显示器内：{rects:?}"
    );
}
