//! 窗口**定位**的判据表（M372 / M386）：纯函数 [`place_in_work_area`] + `[window] place` 日志行。
//!
//! 从 `fit_tests.rs` **纯搬移**（那边当时 396 行、余量 4；M386 要加「负起点」与「第五档 `no-window`」
//! 两条，加不进去）。每格的算式写在行注释里，与 `place.rs` 的模块文档逐格对得上。
//!
//! 本文件**不碰**收缩（尺寸）：那部分判据仍在 `fit_tests.rs`。

use super::fit::fit_to_work_area;
use super::place::{
    place_in_work_area, place_line, PLACE_CENTERED, PLACE_NO_WINDOW, PLACE_NO_WORK_AREA,
    PLACE_PROBE_FAILED, PLACE_UNKNOWN_AREA,
};

/// D30 的验收尺寸 / 最小尺寸（`app/mod.rs` 的 `WINDOW_SIZE` / `MIN_SIZE`）。
const DEFAULT: (f32, f32) = (1180.0, 760.0);
const MIN: (f32, f32) = (880.0, 560.0);

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
        // M386：左置副屏（起点是负数）也在这条不变量里——判据只要求「待在这块矩形里」
        ((-1920.0, 0.0), (1920.0, 1040.0)),
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
    assert_eq!(cases, 24);
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

/// **I7-a：负起点（左置副屏）不许把窗口拉回主屏**。
///
/// 左置副屏在本机 Win32 上的样子是 `rcWork = (-1920, 0, 0, 1040)`（`MONITORINFO` 的坐标可以整体
/// 在负半轴）。scale 1 ⇒ 逻辑矩形 `((-1920, 0), (1920, 1040))`，`DEFAULT` 窗口的居中位是
/// `-1920 + (1920 − 1180)/2 = -1550`、`0 + (1040 − 760)/2 = 140`。
///
/// 抓得住：把 `centered` 的 `clamp(start, max)` 换成 `clamp(0.0, max)`（**这是最像样的那种改法**：
/// 「负坐标=屏外」的直觉）、或把 `place_in_work_area` 的起点判据写成 `left >= 0.0`（直接给 `None`
/// ⇒ 那一屏永远不定位）。两者都会让这条红。
/// 抓不住：真机上窗口是否真落在负坐标（那要真开窗 + 独立读 `GetWindowRect`，见 report 的未验项）。
#[test]
fn a_window_on_a_left_side_screen_stays_on_that_screen() {
    let left_screen = ((-1920.0, 0.0), (1920.0, 1040.0));
    let (x, y) = place_in_work_area(DEFAULT, left_screen).expect("负起点也是有效矩形");
    assert_eq!((x, y), (-1550.0, 140.0), "居中位必须落在副屏自己的坐标系里");
    assert!(x < 0.0, "窗口被拉回主屏了：x={x}（负起点被当成屏外）");
    assert!(
        x >= -1920.0 && x + DEFAULT.0 <= 0.0,
        "整块窗口必须待在这块屏里：{x} … {}",
        x + DEFAULT.0
    );

    // 同一块屏上「窗口比可用区还大」：钉在负起点，**不是**钉在 0
    let squeezed = place_in_work_area((2000.0, 760.0), left_screen).expect("仍算得出位置");
    assert_eq!(squeezed.0, -1920.0, "钉起点，且起点是负的：{squeezed:?}");

    // 起点只在一条轴上是负数（屏在主屏正左、与主屏同高）也不影响另一轴
    let same_height = ((-1920.0, 0.0), (1920.0, 1224.0));
    let (x, y) = place_in_work_area(DEFAULT, same_height).expect("负起点也是有效矩形");
    assert_eq!((x, y), (-1550.0, 232.0), "y 与主屏同款（1224 高的可用区）");

    // 尺寸侧与起点无关：同一份可用区尺寸，不管在正半轴还是负半轴都是 `fits`
    assert_eq!(
        fit_to_work_area(DEFAULT, MIN, Some((1920.0, 1040.0))).size,
        DEFAULT,
        "定位起点不许影响收缩判据"
    );
}

/// **I7-b：`window::latest()==None` 单开第五档 `no-window`**，与「有矩形但算不出坐标」分开读。
///
/// 抓得住：把 `reason` 写回 `unknown-area`（两个完全不同的现场又合成一个词）、或两档常量取了
/// 同一个字符串（`reason=` 失去分辨力）。
/// 抓不住：`window::latest()` 真的返回 `None` 时那行日志有没有打出来（那要真开窗，见
/// `window_fit` 的 tests 模块文档：这条链没有可断言的返回值）。
#[test]
fn a_missing_window_has_its_own_reason() {
    assert_eq!(PLACE_NO_WINDOW, "no-window");
    // 五档取值必须互不相同：合并任意两档，读日志的人都分不清「还没窗口」与「矩形算不出来」
    let mut all = [
        PLACE_CENTERED,
        PLACE_NO_WINDOW,
        PLACE_UNKNOWN_AREA,
        PLACE_PROBE_FAILED,
        PLACE_NO_WORK_AREA,
    ];
    all.sort_unstable();
    let mut unique = all.to_vec();
    unique.dedup();
    assert_eq!(unique.len(), 5, "五档里出现了重复取值：{all:?}");
    // 那一行照实写出来（`<none>` 与真实的 `0,0` 仍能分开读）
    assert_eq!(
        place_line(DEFAULT, None, None, PLACE_NO_WINDOW),
        "[window] place size=1180x760 work_rect=<none> pos=<none> reason=no-window"
    );
}

/// **`no-window` 的接线判据**（r1 复审非-finding ①）：常量本身对上不算数，要钉住**接线**
/// ——`window_fit::task` 的 `window::latest()==None` 那一臂**真的**传的是 `PLACE_NO_WINDOW`。
///
/// 为什么只能扫源码：那一臂跑不到（`window::latest()` 返回 `None` 的时机不可控，`Task` 也没有
/// 可断言的返回值），仓内同形制见 `app/logs_folder.rs::the_logs_folder_opens_through_the_quiet_opener`。
///
/// 抓得住：把那一臂改回 `PLACE_UNKNOWN_AREA`（I7-b 要消灭的正是这个归类）。
/// 抓不住：那一行日志运行时有没有真打出来（那要真开窗，`window_fit` 的 tests 文档已声明）。
#[test]
fn the_no_window_arm_reports_its_own_code() {
    let source = include_str!("../../app/boot/window_fit.rs");
    let subject = source.split("#[cfg(test)]").next().unwrap_or(source);
    let code: Vec<&str> = subject
        .lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .collect();
    let wired = code
        .iter()
        .filter(|line| line.contains("report_place(") && line.contains("PLACE_NO_WINDOW"))
        .count();
    assert_eq!(
        wired, 1,
        "`window_fit::task` 的 None 臂必须把 PLACE_NO_WINDOW 交给 report_place（实测 {wired} 处）"
    );
    for line in &code {
        assert!(
            !(line.contains("report_place(") && line.contains("PLACE_UNKNOWN_AREA")),
            "None 臂不许再报 unknown-area（那是「有矩形却算不出坐标」）：{line}"
        );
    }
}

#[test]
fn the_place_line_says_where_and_why() {
    let work = ((0.0, 0.0), (1316.0, 612.0));
    let pos = place_in_work_area((1180.0, 612.0), work);
    assert_eq!(
        place_line((1180.0, 612.0), Some(work), pos, PLACE_CENTERED),
        "[window] place size=1180x612 work_rect=0,0+1316x612 pos=68,0 reason=centered"
    );
    // 没定位：`pos=<none>` 与真实的 `pos=0,0` 必须能分开读
    for reason in [
        PLACE_NO_WORK_AREA,
        PLACE_PROBE_FAILED,
        PLACE_UNKNOWN_AREA,
        PLACE_NO_WINDOW,
    ] {
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
