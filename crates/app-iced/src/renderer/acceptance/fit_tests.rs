//! M362 的**纯函数判据表**：给定 `(请求, MIN, 可用区)` → 期望的 `(尺寸, 最小尺寸, 结论)`。
//!
//! 每一行的「可用区」都是**物理桌面 ÷ scale** 算出来的（不是拍脑袋的数字），算式写在行注释里，
//! 与 `fit.rs` 模块文档那张表逐格对得上。

use super::*;

/// D30 的验收尺寸 / 最小尺寸（`app/mod.rs` 的 `WINDOW_SIZE` / `MIN_SIZE`）。
const DEFAULT: (f32, f32) = (1180.0, 760.0);
const MIN: (f32, f32) = (880.0, 560.0);

/// 浮点比较：逻辑尺寸经过一次除法，允许 0.01 px 的表示误差（结论不受影响）。
fn close(actual: (f32, f32), expected: (f32, f32), what: &str) {
    assert!(
        (actual.0 - expected.0).abs() < 0.01 && (actual.1 - expected.1).abs() < 0.01,
        "{what}: expected {expected:?}, got {actual:?}"
    );
}

/// 判据表本体。**这一条是全 mission 的判据**：七个格子，每格都能被一次定向变异打红。
#[test]
fn judgement_table_over_work_area_scale_and_min() {
    // ① 1920×1080 @100%：放得下 ⇒ 一个像素都不动（回归口径：与今天完全相同）
    let big = fit_to_work_area(DEFAULT, MIN, Some((1920.0, 1080.0)));
    assert_eq!(big.size, DEFAULT, "大屏必须原样：{big:?}");
    assert_eq!(big.min, MIN, "大屏不该放宽最小尺寸：{big:?}");
    assert_eq!(big.reason, FitReason::Fits);

    // ② 边界等于请求尺寸：仍是 fits（`≥` 不是 `>`，否则 1180×760 的屏会被缩掉 0 像素还报 shrunk）
    let exact = fit_to_work_area(DEFAULT, MIN, Some(DEFAULT));
    assert_eq!(exact.size, DEFAULT);
    assert_eq!(exact.reason, FitReason::Fits);

    // ③ 1366×768 @125%：桌面逻辑 = 1366/1.25 × 768/1.25 = 1092.8×614.4
    //    两维都放不下 ⇒ 按边缩到 1092.8×614.4；两维都仍 ≥ MIN ⇒ 最小尺寸不动
    let small = fit_to_work_area(DEFAULT, MIN, Some((1366.0 / 1.25, 768.0 / 1.25)));
    close(small.size, (1092.8, 614.4), "1366x768@125%");
    assert_eq!(small.min, MIN, "可用区仍 ≥ MIN ⇒ 最小尺寸不许动：{small:?}");
    assert_eq!(small.reason, FitReason::Shrunk);
    assert!(small.reason.adjusts(), "shrunk 必须真去动窗口");

    // ④ 1366×768 @150%：桌面逻辑 = 910.67×512 ⇒ **高度比 MIN 还小**（560）
    //    结论 below-min：宽 910.67 ≥ 880 保持不变，高放宽到 512 —— 固守 560 的话窗口必然超屏
    let tight = fit_to_work_area(DEFAULT, MIN, Some((1366.0 / 1.5, 768.0 / 1.5)));
    close(tight.size, (910.666_7, 512.0), "1366x768@150% 尺寸");
    close(tight.min, (880.0, 512.0), "1366x768@150% 最小尺寸");
    assert_eq!(tight.reason, FitReason::BelowMin);

    // ⑤ 1024×600 @100%：默认窗溢出 156×160，按边缩到 1024×600；MIN 刚好放得下
    let netbook = fit_to_work_area(DEFAULT, MIN, Some((1024.0, 600.0)));
    assert_eq!(netbook.size, (1024.0, 600.0));
    assert_eq!(netbook.min, MIN);
    assert_eq!(netbook.reason, FitReason::Shrunk);

    // ⑥ 1024×600 @125%：桌面逻辑 = 819.2×480 ⇒ 两维都小于 MIN ⇒ 可用区说了算
    let tiny = fit_to_work_area(DEFAULT, MIN, Some((1024.0 / 1.25, 600.0 / 1.25)));
    close(tiny.size, (819.2, 480.0), "1024x600@125% 尺寸");
    close(tiny.min, (819.2, 480.0), "1024x600@125% 最小尺寸");
    assert_eq!(tiny.reason, FitReason::BelowMin);

    // ⑦ 没有可用区（拿不到显示器 / 值非法）⇒ 不动，但**不假装 fits**（日志要能分开读）
    for area in [
        None,
        Some((0.0, 0.0)),
        Some((-100.0, 600.0)),
        Some((f32::NAN, 600.0)),
        Some((f32::INFINITY, 600.0)),
    ] {
        let unknown = fit_to_work_area(DEFAULT, MIN, area);
        assert_eq!(unknown.size, DEFAULT, "{area:?}");
        assert_eq!(unknown.min, MIN, "{area:?}");
        assert_eq!(unknown.reason, FitReason::Unknown, "{area:?}");
        assert!(!unknown.reason.adjusts(), "{area:?} 不该触发 resize");
    }
}

/// **mission 标题的那条不变量**：收缩之后窗口必须放得下可用区，且最小尺寸不许比尺寸大
/// （否则 winit 会把 resize 夹回去，等于空转）。
#[test]
fn fitted_window_never_exceeds_the_work_area() {
    let mut cases = 0;
    for width in [640_u16, 800, 1024, 1092, 1180, 1366, 1920, 2560] {
        for height in [480_u16, 512, 600, 760, 768, 1080, 1440] {
            let area = (f32::from(width), f32::from(height));
            let fit = fit_to_work_area(DEFAULT, MIN, Some(area));
            assert!(fit.size.0 <= area.0 + 0.01, "{area:?} → {fit:?}");
            assert!(fit.size.1 <= area.1 + 0.01, "{area:?} → {fit:?}");
            assert!(fit.min.0 <= fit.size.0 + 0.01, "{area:?} → {fit:?}");
            assert!(fit.min.1 <= fit.size.1 + 0.01, "{area:?} → {fit:?}");
            assert!(
                fit.size.0 <= DEFAULT.0 + 0.01 && fit.size.1 <= DEFAULT.1 + 0.01,
                "{area:?} 只许缩不许放大（放大是另一件事，没做过就是没做过）：{fit:?}"
            );
            cases += 1;
        }
    }
    assert_eq!(cases, 56);
}

/// **回归口径写成断言**：100% DPI / 大屏（可用区 ≥ 1180×760）下开窗尺寸与今天**逐位相同**。
///
/// 抓得住：把 `fits` 分支改成「等比缩到 95%」这类「顺手优化」——大屏上尺寸就变了。
/// 抓不住：真窗口是否真按这个尺寸建（那要真开窗量，见 `DevDocs/artifacts/winshrink`）。
#[test]
fn large_screens_keep_todays_window_size_bit_for_bit() {
    for area in [
        (1180.0, 760.0),
        (1280.0, 800.0),
        (1366.0, 768.0),
        (1920.0, 1080.0),
        (2560.0, 1440.0),
        (3840.0, 2160.0),
    ] {
        let fit = fit_to_work_area(DEFAULT, MIN, Some(area));
        assert_eq!(fit.size, DEFAULT, "{area:?} 上开窗尺寸必须与今天完全相同");
        assert_eq!(fit.min, MIN, "{area:?} 上最小尺寸必须与今天完全相同");
        assert_eq!(fit.reason, FitReason::Fits, "{area:?}");
        assert!(!fit.reason.adjusts(), "{area:?} 上不许发 resize");
    }
}

/// `ULL_WINDOW_SIZE` 的既有语义**逐字不变**：它仍然只被夹到最小尺寸，并且**不走**屏幕收缩
/// （矩阵的「宽屏」档靠它，宿主屏不该改写脚本的请求）。
#[test]
fn the_window_size_override_still_wins_verbatim() {
    let (size, source) = super::super::window::resolve_window_size(Some("1520x900"), DEFAULT, MIN);
    assert_eq!(size, (1520.0, 900.0));
    assert_eq!(source, "ULL_WINDOW_SIZE");
    assert_eq!(
        fit_skip_reason(source, false),
        Some("skipped-source"),
        "env 覆盖档必须被收缩跳过"
    );

    let (clamped, source) =
        super::super::window::resolve_window_size(Some("640x480"), DEFAULT, MIN);
    assert_eq!(clamped, MIN);
    assert_eq!(source, "ULL_WINDOW_SIZE:clamped-to-min");
    assert_eq!(fit_skip_reason(source, false), Some("skipped-source"));

    // 自动档才做收缩；Termux 那条自己算过尺寸，同样不算「自动档」
    assert_eq!(fit_skip_reason("default", false), None);
    assert_eq!(
        fit_skip_reason("termux-screen", false),
        Some("skipped-source")
    );
    // 取证启动一律不收缩（D30 的图必须只由参数决定）
    assert_eq!(fit_skip_reason("default", true), Some("skipped-capture"));
    assert_eq!(
        fit_skip_reason("ULL_WINDOW_SIZE", true),
        Some("skipped-capture"),
        "取证优先：同时成立时报这次进程的性质"
    );
}

/// 可用区来源必须自证（`win32-rcwork` / `monitor` / `monitor:probe-failed` / `env` / `env:invalid` /
/// `none`）—— 把「宿主显示器」写成「可用区」是这条 mission 最容易犯的谎（M371 把真工作区加进来源族，
/// 但**没有**改变任何一档的优先级判据）。
#[test]
fn work_area_source_is_labelled_and_never_invented() {
    let monitor = Some((1920.0, 1080.0));
    let work = Some((1920.0, 1040.0));

    // M371：Windows 的真工作区（rcWork）优先于显示器上界
    assert_eq!(
        work_area_from(None, work, monitor, false),
        (work, "win32-rcwork"),
        "有真工作区时不许退回显示器尺寸"
    );
    // macOS / Linux（probe 恒 None）：仍是 M362 的行为，来源必须如实写 monitor
    assert_eq!(
        work_area_from(None, None, monitor, false),
        (monitor, "monitor")
    );
    // Windows 上 probe **失败**：与「本平台没有」分开，否则真机排障看不出这条链断在哪
    assert_eq!(
        work_area_from(None, None, monitor, true),
        (monitor, "monitor:probe-failed"),
        "有探测能力却没拿到 ⇒ 来源必须点名 probe-failed"
    );
    // 拿到了工作区时 probe 当然没失败：真值来源不受这个标记影响
    assert_eq!(
        work_area_from(None, work, monitor, true),
        (work, "win32-rcwork")
    );
    // 覆盖永远最高：它印 env，**永远不能被读成真显示器**
    assert_eq!(
        work_area_from(Some("1093x614"), work, monitor, true),
        (Some((1093.0, 614.0)), "env"),
        "覆盖生效时来源必须是 env（不是 win32-rcwork / monitor）"
    );
    // 覆盖写坏了：留痕，然后退回真工作区（比显示器上界更准）
    assert_eq!(
        work_area_from(Some("wide-ish"), work, monitor, false),
        (work, "env:invalid"),
        "覆盖解析不了要留痕，退回的必须是真工作区"
    );
    assert_eq!(
        work_area_from(Some("wide-ish"), None, monitor, false),
        (monitor, "env:invalid")
    );
    assert_eq!(
        work_area_from(None, None, None, true),
        (None, "none"),
        "连显示器都没有时 probe-failed 也无处可用"
    );
    assert_eq!(
        work_area_from(Some("wide-ish"), None, None, false),
        (None, "env:invalid")
    );
}

/// M371 的验收判据 ④ 写成断言：日志里 `work_area=` 与 `monitor=` 必须**分开读**，
/// 差值恰好是任务栏（本机实测 2632×1272 → 2632×1224，48 物理 px @100%）。
///
/// 抓得住：把 `work_area_source` 写成 `monitor`、或把 `work_area=` 换成显示器尺寸
/// （那等于退回 M362，谁读日志都会以为任务栏已经扣过）。
/// 抓不住：这两个数是否真是这台机器读到的（那要真开窗，见 `DevDocs/artifacts/workarea`）。
#[test]
fn the_fit_line_separates_the_work_area_from_the_monitor() {
    let work = (2632.0, 1224.0);
    let monitor = (2632.0, 1272.0);
    let (area, source) = work_area_from(None, Some(work), Some(monitor), false);
    let fit = fit_to_work_area(DEFAULT, MIN, area);
    let line = fit_line(DEFAULT, &fit, area, source, Some(monitor));
    assert_eq!(
        line,
        "[window] fit size_source=screen-fit requested=1180x760 size=1180x760 \
         min=880x560 work_area=2632x1224 work_area_source=win32-rcwork monitor=2632x1272 \
         reason=fits"
    );
    assert!(
        area.expect("有真工作区").1 < monitor.1,
        "可用区必须严格小于显示器尺寸"
    );
}

/// `[window] fit` 一行必须能一眼看出三件事：因屏幕收缩到多少、可用区多大、为什么。
#[test]
fn the_fit_line_names_the_size_the_area_and_the_reason() {
    let area = (1366.0 / 1.25, 768.0 / 1.25);
    let fit = fit_to_work_area(DEFAULT, MIN, Some(area));
    let line = fit_line(DEFAULT, &fit, Some(area), "monitor", Some(area));
    assert_eq!(
        line,
        "[window] fit size_source=screen-fit requested=1180x760 size=1092.8x614.4 \
         min=880x560 work_area=1092.8x614.4 work_area_source=monitor monitor=1092.8x614.4 \
         reason=shrunk"
    );

    // 没拿到可用区：写 `<none>`，不许编一个 0x0 冒充观测值
    let unknown = fit_to_work_area(DEFAULT, MIN, None);
    let line = fit_line(DEFAULT, &unknown, None, "none", None);
    assert!(line.contains("work_area=<none>"), "{line}");
    assert!(line.contains("monitor=<none>"), "{line}");
    assert!(line.contains("reason=unknown"), "{line}");
    assert!(line.contains("size=1180x760"), "{line}");
}

/// 收缩被跳过时也要留一行（否则读日志的人分不清「没收缩」与「没跑」）。
#[test]
fn the_skip_line_says_which_source_held_the_size() {
    let line = skip_line((1520.0, 900.0), MIN, "ULL_WINDOW_SIZE", "skipped-source");
    assert_eq!(
        line,
        "[window] fit size_source=ULL_WINDOW_SIZE requested=1520x900 size=1520x900 \
         min=880x560 work_area=<none> work_area_source=skipped monitor=<none> \
         reason=skipped-source"
    );
    let capture = skip_line(DEFAULT, MIN, "default", "skipped-capture");
    assert!(capture.contains("reason=skipped-capture"), "{capture}");
    assert!(capture.contains("work_area_source=skipped"), "{capture}");
}

/// M372 的**定位判据表**：给定 `(窗口尺寸, 可用区矩形)` → 期望 `(x, y)`（每格算式写在行注释里）。
#[test]
fn placement_table_over_size_and_work_rect() {
    // ① 本机大屏 2632×1272 @100%，任务栏 48 ⇒ rcWork = (0,0)+2632×1224：(726, 232)
    let big = ((0.0, 0.0), (2632.0, 1224.0));
    assert_eq!(place_in_work_area(DEFAULT, big), Some((726.0, 232.0)));
    // ② 同一块屏 `--scale-factor 2`：可用区 1316×612，窗口 fit 到 1180×612 ⇒ (68, 0)（贴边不溢出）
    let small = ((0.0, 0.0), (1316.0, 612.0));
    assert_eq!(
        place_in_work_area((1180.0, 612.0), small),
        Some((68.0, 0.0))
    );
    // ③ 副屏（起点 1920、scale 1）：1920 + (1920−1180)/2 = 2290；丢掉起点会算成 (370, …) = 搬到主屏
    let secondary = ((1920.0, 0.0), (1920.0, 1040.0));
    assert_eq!(
        place_in_work_area(DEFAULT, secondary),
        Some((2290.0, 140.0))
    );
    // ④ 窗口比可用区还大（fit 之后理论上不会发生）：钉在起点，不许算出屏外的负坐标
    let tiny = ((0.0, 0.0), (800.0, 500.0));
    assert_eq!(place_in_work_area(DEFAULT, tiny), Some((0.0, 0.0)));
    let offset = ((100.0, 50.0), (800.0, 500.0));
    assert_eq!(place_in_work_area(DEFAULT, offset), Some((100.0, 50.0)));
    // ⑤ 奇数空档四舍五入到整逻辑 px：(613−612)/2 = 0.5 ⇒ 1，且 1+612 ≤ 613
    let odd = ((0.0, 0.0), (1316.0, 613.0));
    assert_eq!(place_in_work_area((1180.0, 612.0), odd), Some((68.0, 1.0)));
    // ⑥ 不可用输入一律 None（**不编坐标**）：尺寸 / 起点 / 可用区各自的零值与非有限值
    for (size, work) in [
        (DEFAULT, ((0.0, 0.0), (0.0, 612.0))),
        (DEFAULT, ((0.0, 0.0), (1316.0, 0.0))),
        (DEFAULT, ((0.0, 0.0), (-100.0, 612.0))),
        (DEFAULT, ((0.0, 0.0), (f32::NAN, 612.0))),
        (DEFAULT, ((f32::NAN, 0.0), (1316.0, 612.0))),
        (DEFAULT, ((f32::INFINITY, 0.0), (1316.0, 612.0))),
        ((0.0, 760.0), ((0.0, 0.0), (1316.0, 612.0))),
        ((f32::NAN, f32::NAN), ((0.0, 0.0), (1316.0, 612.0))),
    ] {
        assert_eq!(
            place_in_work_area(size, work),
            None,
            "size={size:?} work={work:?}"
        );
    }
}

/// **M372 的真不变量**（验收判据 ②）：定位之后「顶边 ≥ 可用区顶边」且「底边 ≤ 可用区底边」。
///
/// 抓得住：`centered` 不夹边界（窗口 > 可用区时出界）、或纯函数漏掉起点（副屏整块红）。
/// 抓不住：真窗口是否真挪到那个坐标（那要真开窗量，见 `DevDocs/artifacts/winpos`）。
#[test]
fn placed_window_never_leaves_the_work_rect() {
    let rects = [
        ((0.0, 0.0), (2632.0, 1224.0)),
        ((0.0, 0.0), (1316.0, 612.0)),
        ((1920.0, 0.0), (1920.0, 1040.0)),
        ((0.0, 0.0), (819.2, 480.0)),
        ((100.0, 50.0), (800.0, 500.0)),
    ];
    let mut cases = 0;
    for ((left, top), (width, height)) in rects {
        for size in [DEFAULT, (1180.0, 612.0), (1024.0, 600.0), (819.2, 480.0)] {
            let fit = fit_to_work_area(size, MIN, Some((width, height)));
            let (x, y) = place_in_work_area(fit.size, ((left, top), (width, height)))
                .unwrap_or_else(|| panic!("{size:?} 在 {width}x{height} 上必须算得出位置"));
            assert!(
                x >= left && y >= top,
                "顶边/左边越界：{fit:?} → ({x}, {y})，可用区起点 ({left}, {top})"
            );
            // 窗口比可用区大时只保证「钉在起点」，所以按 min(窗口, 可用区) 判可达范围
            let (reach_x, reach_y) = (x + fit.size.0.min(width), y + fit.size.1.min(height));
            assert!(
                reach_x <= left + width + 0.01 && reach_y <= top + height + 0.01,
                "底边/右边溢出：{fit:?} → ({x}, {y})，可用区 {width}x{height}+({left}, {top})"
            );
            cases += 1;
        }
    }
    assert_eq!(cases, 20);
}

/// 判据 ② 的那一格单独钉一次：`--scale-factor 2` 下**底边 ≤ 可用区底边 且 顶边 ≥ 可用区顶边**。
///
/// 修复前窗口落在级联位（物理 y=260 ⇒ 逻辑 y=130）：底边 130+612 = 742 > 612，溢出 130 逻辑 px
/// （= 物理 260），其中 24 逻辑 px（可用区底边 612..显示器底边 636）正落在任务栏带里。
#[test]
fn the_small_screen_case_stays_inside_vertically() {
    let area = (1316.0, 612.0);
    let fit = fit_to_work_area(DEFAULT, MIN, Some(area));
    assert_eq!(fit.size, (1180.0, 612.0), "尺寸仍由 M362 的纯函数定");
    let (_, y) = place_in_work_area(fit.size, ((0.0, 0.0), area)).expect("小屏必须算得出位置");
    assert!(
        y + fit.size.1 <= area.1 + 0.01,
        "底边溢出：{y} + {}",
        fit.size.1
    );
    assert!(y >= 0.0, "顶边不许在可用区上面：{y}");
    // 反例：把修复前的级联位（物理 260 = 逻辑 130）代进同一判据，必须溢出
    assert!(130.0 + fit.size.1 > area.1, "级联位必须溢出");
}

/// `[window] place` 一行必须能一眼看出「定位到哪、依据哪块矩形、为什么没定位」。
#[test]
fn the_place_line_says_where_and_why() {
    let work = ((0.0, 0.0), (1316.0, 612.0));
    let pos = place_in_work_area((1180.0, 612.0), work);
    assert_eq!(
        place_line((1180.0, 612.0), Some(work), pos, PLACE_CENTERED),
        "[window] place size=1180x612 work_rect=0,0+1316x612 pos=68,0 reason=centered"
    );
    // 没定位：`pos=<none>` 与真实的 `pos=0,0` 必须能分开读
    for reason in [PLACE_NO_WORK_AREA, PLACE_PROBE_FAILED, PLACE_UNKNOWN_AREA] {
        assert_eq!(
            place_line(DEFAULT, None, None, reason),
            format!("[window] place size=1180x760 work_rect=<none> pos=<none> reason={reason}")
        );
    }
    let skipped = place_line(DEFAULT, None, None, "skipped-capture");
    assert!(
        skipped.ends_with("pos=<none> reason=skipped-capture"),
        "{skipped}"
    );
}
