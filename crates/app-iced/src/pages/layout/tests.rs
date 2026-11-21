//! `layout` 的断言：断点表、版面决议、行宽上限、列宽几何。
//!
//! 这些是**门禁级**断言：阈值改错必须立刻红（tower 会故意改错来验）。
use super::*;

/// 断点表：官方 600 / 840 两个硬值，边界闭开区间逐点钉死。
#[test]
fn breakpoints_are_the_official_600_and_840() {
    assert_eq!(BREAKPOINT_MEDIUM, 600.0, "官方 medium 断点");
    assert_eq!(BREAKPOINT_EXPANDED, 840.0, "官方 expanded 断点");

    for (width, expected) in [
        (320.0, WidthClass::Compact),
        (599.0, WidthClass::Compact),
        (599.9, WidthClass::Compact),
        (600.0, WidthClass::Medium),
        (720.0, WidthClass::Medium),
        (839.0, WidthClass::Medium),
        (839.9, WidthClass::Medium),
        (840.0, WidthClass::Expanded),
        (900.0, WidthClass::Expanded),
        (1180.0, WidthClass::Expanded),
        (2560.0, WidthClass::Expanded),
    ] {
        assert_eq!(
            WidthClass::of(width),
            expected,
            "{width} 宽应该判为 {expected:?}"
        );
    }
}

/// 版面决议表（tower 会照这张表对文档）：一列写死「宽度 → 版面」。
#[test]
fn layout_table_matches_the_documented_thresholds() {
    // (窗口宽度, 实例页, 设置页, 下载/日志页)
    let table = [
        (
            320.0,
            PageLayout::SingleColumn,
            PageLayout::SingleColumn,
            PageLayout::SingleColumn,
        ),
        (
            599.0,
            PageLayout::SingleColumn,
            PageLayout::SingleColumn,
            PageLayout::SingleColumn,
        ),
        (
            700.0,
            PageLayout::SingleColumn,
            PageLayout::SingleColumn,
            PageLayout::SingleColumn,
        ),
        (
            720.0,
            PageLayout::SingleColumn,
            PageLayout::SupportingPane,
            PageLayout::SingleColumn,
        ),
        (
            800.0,
            PageLayout::SingleColumn,
            PageLayout::SupportingPane,
            PageLayout::SingleColumn,
        ),
        (
            839.0,
            PageLayout::SingleColumn,
            PageLayout::SupportingPane,
            PageLayout::SingleColumn,
        ),
        (
            840.0,
            PageLayout::ListDetail,
            PageLayout::SupportingPane,
            PageLayout::SingleColumn,
        ),
        (
            900.0,
            PageLayout::ListDetail,
            PageLayout::SupportingPane,
            PageLayout::SingleColumn,
        ),
        (
            1180.0,
            PageLayout::ListDetail,
            PageLayout::SupportingPane,
            PageLayout::SingleColumn,
        ),
        (
            1920.0,
            PageLayout::ListDetail,
            PageLayout::SupportingPane,
            PageLayout::SingleColumn,
        ),
    ];

    for (width, instances, settings, single) in table {
        assert_eq!(
            resolve(PaneKit::ListDetail, width),
            instances,
            "{width} 宽实例页版面不对"
        );
        assert_eq!(
            resolve(PaneKit::Supporting, width),
            settings,
            "{width} 宽设置页版面不对"
        );
        assert_eq!(
            resolve(PaneKit::Single, width),
            single,
            "{width} 宽单列页版面对不"
        );
    }
}

#[test]
fn pane_thresholds_are_the_sum_of_their_columns() {
    // 336 + 320 + 24 + 2×24 + 88 = 816
    assert_eq!(list_detail_min_window(), 816.0);
    // 240 + 320 + 24 + 2×24 + 88 = 720
    assert_eq!(supporting_pane_min_window(), 720.0);
    // 官方 expanded 断点必须**宽于** list-detail 的几何门槛，否则「840 起用 list-detail」是空话
    assert!(
        BREAKPOINT_EXPANDED >= list_detail_min_window(),
        "840 档必须放得下 336 + 24 + 320 的三列和"
    );
    // 设置页在 medium 档里就有能用的窗口宽度（720 < 840）
    assert!(supporting_pane_min_window() < BREAKPOINT_EXPANDED);
    assert!(WidthClass::of(supporting_pane_min_window()) == WidthClass::Medium);
}

/// 实测样本：**实例页详情那段正文**（`pages/instances/data.rs` 的 `NOTE`）。
///
/// 口径（`layout` 模块文档 §3）：Segoe UI Regular @14px、无 letter-spacing、
/// FreeType `getlength` 求整段排版宽。**这是外部实测数据，不是从常量推出来的**——
/// 下面的断言拿它反算「一行能放几个字符」，所以把 `CHAR_ADVANCE_EM` 改回估算值会立刻红。
const MEASURE_SAMPLE_CHARS: f32 = 361.0;
const MEASURE_SAMPLE_WIDTH_PX: f32 = 2185.0;

/// 样本的实测平均步进（px/字符）。
fn measured_advance_px() -> f32 {
    MEASURE_SAMPLE_WIDTH_PX / MEASURE_SAMPLE_CHARS
}

/// 给定列宽，按**实测步进**能放多少字符。
fn chars_per_line(width: f32) -> f32 {
    width / measured_advance_px()
}

/// **逐行**折行模拟结果（FreeType `getlength` 量宽 + 贪心词折行，脚本见模块文档 §3）。
///
/// 四个数都是外部实测，口径要分清（视觉 r3 提醒过这里容易写糊）：
/// - `SAFE_MAX_CHARS` / `SAFE_MAX_CHARS_INK`：cap=424 时**字符最多**的那一行 —— 67 字符，ink **402px**；
/// - `SAFE_WIDEST_INK`：cap=424 时**墨迹最宽**的那一行 —— **411px**（不是同一行！402 那行字符多但字形窄）；
/// - `BREAK_INK` / `BREAK_CHARS`：第一档会冒出 73 字符行的宽度 428px（那行 ink 428px）。
///
/// 有了它，Rust 侧不用字体引擎也能把「上限必须夹在这些数之间」写成断言。
const SIM_SAFE_MAX_CHARS: f32 = 67.0;
const SIM_SAFE_MAX_CHARS_INK: f32 = 402.0;
const SIM_SAFE_WIDEST_INK: f32 = 411.0;
const SIM_BREAK_INK: f32 = 428.0;
const SIM_BREAK_CHARS: f32 = 73.0;

/// 行宽上限必须真的停在 72 字符：名字、常量、**逐行模拟**三者一致。
#[test]
fn line_length_is_capped_at_72_characters() {
    // ① 常量对得上实测步进（14px 下的平均步进）
    let body_size = TypeRole::BodyMedium.style().size_px;
    assert_eq!(body_size, 14.0);
    assert!(
        (measured_advance_px() - body_size * CHAR_ADVANCE_EM).abs() < 0.02,
        "CHAR_ADVANCE_EM = {CHAR_ADVANCE_EM} 与实测步进 {:.4}px/字符对不上",
        measured_advance_px()
    );
    assert_eq!(measure_floor_px(), 272.0, "floor(14 × 0.43233 × 45)");

    // ② 上限是**逐行**模拟出来的 424，不是「平均步进 × 72」的算术值 435
    assert_eq!(measure_cap_px(), 424.0, "逐行安全宽（4px 栅格）");
    assert_eq!(
        measure_average_bound_px(),
        435.0,
        "floor(14 × 0.43233 × 72)"
    );
    assert!(
        measure_cap_px() < measure_average_bound_px(),
        "平均口径上界必须比逐行安全宽**更宽**——这说明 cap 不是拿平均算出来的"
    );

    // ③ 交付宽度下两条实测线都装得下：字符最多那行（402px）与墨迹最宽那行（411px）
    assert!(
        SIM_SAFE_MAX_CHARS_INK <= measure_cap_px(),
        "字符最多那行 {}px 超过上限 {}px",
        SIM_SAFE_MAX_CHARS_INK,
        measure_cap_px()
    );
    assert!(
        SIM_SAFE_WIDEST_INK <= measure_cap_px(),
        "墨迹最宽那行 {}px 超过上限 {}px（余量只有 {}px）",
        SIM_SAFE_WIDEST_INK,
        measure_cap_px(),
        measure_cap_px() - SIM_SAFE_WIDEST_INK
    );
    const { assert!(SIM_SAFE_MAX_CHARS <= MAX_MEASURE_CHARS) };

    // ④ **两侧夹逼**：这是本条真正的护栏。
    //    放大到 SIM_BREAK_INK（428）逐行模拟就会重新出现 73 字符行；
    //    压到 SAFE 以下则纯属白丢宽度。两个方向都会红。
    assert!(
        measure_cap_px() < SIM_BREAK_INK,
        "上限 ≥{SIM_BREAK_INK}px 时逐行模拟会再次出现 {SIM_BREAK_CHARS} 字符行——请重跑折行模拟"
    );
    assert!(
        measure_cap_px() >= SIM_SAFE_WIDEST_INK,
        "上限 {}px 比模拟出来的最宽行 {}px 还窄，白丢宽度",
        measure_cap_px(),
        SIM_SAFE_WIDEST_INK
    );

    // ⑤ 平均口径的回归护栏：旧 0.5em 的 504px 在实测步进下能放 83 字符
    assert!(
        chars_per_line(504.0) > MAX_MEASURE_CHARS + 10.0,
        "504px 在实测步进下应能放 >82 字符（旧口径的破绽），现在算出来 {:.1}",
        chars_per_line(504.0)
    );

    for width in [600.0, 840.0, 900.0, 1180.0, 1440.0, 2560.0] {
        let column = content_column_width(width);
        // 平均口径只是次要护栏（逐行口径见上面的模拟常数）
        assert!(
            chars_per_line(column) <= MAX_MEASURE_CHARS,
            "{width} 宽窗口的平均行宽 {:.1} 字符，超过 {MAX_MEASURE_CHARS}",
            chars_per_line(column)
        );
        assert!(column <= measure_cap_px() + f32::EPSILON);
    }

    // 1180 —— M68 就是在这条上翻的车（712px），现在必须是 424（= measure_cap_px()）
    let wide = content_column_width(1180.0);
    assert_eq!(wide, measure_cap_px());
    assert!(wide < 712.0, "不许回到 M68 的全宽单卡（712px）");
}

#[test]
fn line_length_tracks_width_until_it_hits_the_cap() {
    // 窄窗：正文列跟着可用宽度走（480 宽还在上限以内），撞上上限就锁死
    let narrow = content_column_width(480.0);
    assert!(narrow < measure_cap_px(), "480 宽时正文列应窄于上限");
    assert_eq!(narrow, available_width(480.0));

    // 单调不减：窗口变宽，正文列不变窄
    let mut previous = 0.0;
    for width in [320.0, 480.0, 600.0, 720.0, 840.0, 1024.0, 1180.0, 1600.0] {
        let column = content_column_width(width);
        assert!(column >= previous, "{width} 宽的正文列比更窄时还小");
        previous = column;
    }

    // 一旦撞上上限就锁死（宽窗不会继续变宽）
    assert_eq!(content_column_width(1180.0), measure_cap_px());
    assert_eq!(content_column_width(1920.0), measure_cap_px());
}

#[test]
fn list_detail_geometry_keeps_the_list_column_in_range() {
    let metrics = frame_metrics(PaneKit::ListDetail, 1180.0);
    assert_eq!(metrics.layout, PageLayout::ListDetail);

    let list = metrics.secondary.expect("list-detail 必须有列表列");
    assert!(
        (320.0..=360.0).contains(&list),
        "列表列 {list} 不在 320~360 参考区间"
    );
    assert_eq!(list, LIST_COLUMN_WIDTH);

    // 详情正文列：≤ 行宽上限，且明显窄于「窗口全宽」
    assert!(metrics.column <= measure_cap_px() + f32::EPSILON);
    assert!(metrics.column < metrics.available, "详情列必须窄于可用宽度");

    // 列表列 + 间隙 + 详情 pane = 可用宽度（不留缝）
    assert_eq!(
        list + PANES_GAP + metrics.main_pane,
        metrics.available,
        "两列几何必须铺满可用宽度"
    );
}

#[test]
fn list_detail_degrades_to_one_column_below_the_threshold() {
    for width in [320.0, 599.0, 700.0, 816.0, 839.0] {
        assert_eq!(
            frame_metrics(PaneKit::ListDetail, width).layout,
            PageLayout::SingleColumn,
            "{width} 宽应该退化成单列"
        );
        assert!(frame_metrics(PaneKit::ListDetail, width)
            .secondary
            .is_none());
    }

    // 840 = 官方 expanded 断点，list-detail 从这里成立
    assert_eq!(
        frame_metrics(PaneKit::ListDetail, 840.0).layout,
        PageLayout::ListDetail
    );
    // 840 宽下详情正文列仍要够宽（≥ 下限），否则等于假分栏
    let at_breakpoint = frame_metrics(PaneKit::ListDetail, 840.0);
    assert!(
        at_breakpoint.column >= measure_floor_px(),
        "840 宽下详情列 {:?} 低于行宽下限",
        at_breakpoint.column
    );
}

#[test]
fn settings_pane_keeps_the_drawer_width_and_a_readable_form_column() {
    let metrics = frame_metrics(PaneKit::Supporting, 1180.0);
    assert_eq!(metrics.layout, PageLayout::SupportingPane);
    assert_eq!(metrics.secondary, Some(SIDE_NAV_WIDTH));
    assert_eq!(SIDE_NAV_WIDTH, 240.0, "D33：展开态是 240 的 drawer");
    assert_eq!(metrics.column, measure_cap_px(), "表单列吃满行宽上限");
    assert_eq!(measure_cap_px(), 424.0);

    let narrow = frame_metrics(PaneKit::Supporting, 700.0);
    assert_eq!(narrow.layout, PageLayout::SingleColumn);
    assert!(narrow.secondary.is_none());
}

#[test]
fn meta_grid_drops_columns_as_the_detail_narrows() {
    // 160 宽卡：3 列要 3×160 + 2×12 = 504（超出行宽上限），所以上限处就是 2 列；2 列要 332
    assert_eq!(meta_columns(measure_cap_px()), 2, "2×160 + 12 = 332");
    assert_eq!(meta_columns(340.0), 2);
    assert_eq!(meta_columns(330.0), 1);

    // 1180 与 900 都是 2 列（160 宽卡三列塞不进行宽上限），差别落在详情列宽
    let wide = frame_metrics(PaneKit::ListDetail, 1180.0);
    let mid = frame_metrics(PaneKit::ListDetail, 900.0);
    assert_eq!(wide.meta_columns, 2);
    assert_eq!(mid.meta_columns, 2);
    assert!(mid.column < wide.column, "900 宽下详情列必须真的变窄");
}

#[test]
fn gutters_follow_the_class_and_are_on_the_grid() {
    assert_eq!(WidthClass::Compact.gutter(), 16.0);
    assert_eq!(WidthClass::Medium.gutter(), 24.0);
    assert_eq!(WidthClass::Expanded.gutter(), 24.0);
    for class in [
        WidthClass::Compact,
        WidthClass::Medium,
        WidthClass::Expanded,
    ] {
        assert_eq!(class.gutter() % 4.0, 0.0, "{class:?} 的页边距不在 4px 栅格");
    }
}

#[test]
fn page_layout_names_are_stable() {
    assert_eq!(PageLayout::SingleColumn.name(), "single-column");
    assert_eq!(PageLayout::SupportingPane.name(), "supporting-pane");
    assert_eq!(PageLayout::ListDetail.name(), "list-detail");
    assert!(!PageLayout::SingleColumn.is_two_pane());
    assert!(PageLayout::ListDetail.is_two_pane());
}
