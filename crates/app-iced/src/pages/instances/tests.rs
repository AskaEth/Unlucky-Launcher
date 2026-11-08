//! `instances` 的断言：演示数据、窄窗退化，以及**页面摆法**（源码结构断言）。
use super::*;

/// M120 的文案守门（中文无英文词 / 切语言真的变 / 页面里不许写死英文案·白名单）。
mod i18n;

/// M157 的门限判据（三态 chips 在版本不够时谁可点）。
mod launch_args;

use super::super::layout::source;
use super::super::layout::{frame_metrics, measure_cap_px, PaneKit, WidthClass};

/// **M102 几何判据**：列表可视区正好放下整数行——末行不会被卡片底边切半行。
///
/// 算式里的三个页面真源**直接引用**（不写字面量），所以改页面常量会让这条立刻红：
/// [`LIST_BOTTOM_INSET`]（滚动区底部内缩）、[`ROW_SPACING`]（页级行距，`None` = 0）。
///
/// ```text
/// 卡片高 = 760 − 标题栏 48 − 两行页头 84 − 2×gutter 24      = 580
/// 列头   = 上 12 + LabelMedium 16 + 下 4                     = 32
/// 内缩   = LIST_BOTTOM_INSET = Space::X2                     = 8
/// 可视区 = 580 − 32 − 8                                      = 540
/// 行高   = ListItem 两行档 44 + 16                           = 60
/// pitch  = 行高 + ROW_SPACING（None ⇒ 0）                    = 60
/// 整行数 = 540 / 60                                          = 9 行整
/// ```
///
/// **抓得住**：页面把内缩换档（`LIST_BOTTOM_INSET` → `X1` 立刻除不尽）、行距真源被改、
/// 列头算错、演示数据被缩到不够滚（独立滚动条消失）；
/// **抓不住**：`list_column()` 里被另写一层 `.spacing(…)`（那一条由
/// `list_column_spaces_rows_only_from_the_shared_constant` 扫源码守）。
#[test]
fn list_viewport_fits_whole_rows_and_still_overflow() {
    use unlucky_design::spacing::LayoutMetrics;
    use unlucky_design::typography::TypeRole;

    let lm = LayoutMetrics::DESKTOP;
    let gutter = WidthClass::Expanded.gutter();

    let card = 760.0 - lm.titlebar_height - lm.header_height(true) - 2.0 * gutter;
    assert_eq!(card, 580.0, "1180×760 下列表卡片的内部高度");

    let head = Space::X3.dp() + TypeRole::LabelMedium.style().line_height_px + Space::X1.dp();
    let viewport = card - head - LIST_BOTTOM_INSET.dp();
    assert_eq!(viewport, 540.0, "列表滚动区高度必须以页面的内缩真源算");

    // 页级行距的**唯一真源**：None = 这一层不加间距
    let gap = ROW_SPACING.map_or(0.0, Space::dp);
    let row = lm.row_height + Space::X4.dp();
    let pitch = row + gap;
    assert_eq!(pitch, 60.0, "行节奏 = ListItem 两行档 60（不留页级行距）");
    assert_eq!(
        viewport % pitch,
        0.0,
        "可视区 {viewport} 不是 {pitch} 的整数倍 ⇒ 末行会被切半行（ROW_SPACING={ROW_SPACING:?}）"
    );

    // 同时必须还能滚：12 行内容高过可视区，截图里才看得到独立滚动条
    let content = DEMO.len() as f32 * pitch;
    assert!(
        content > viewport,
        "列表内容 {content} 不比可视区 {viewport} 高，截图里看不到独立滚动条"
    );
}

/// **源码结构**：`list_column()` 里对行容器的间距**只能**来自 [`ROW_SPACING`]。
///
/// 为什么读源码：变异实测（M102 r1）把 `.spacing(ui.space(Space::X1))` 加回行容器后，
/// 几何断言照样绿——它算的是常量，看不见页面又叠了一层。iced 的 Element 又没有内省接口，
/// 所以只能断言这段代码的形状。
///
/// ## 两层判据（r4 复审的 M4 订正）
///
/// | 层 | 判据 | 抓的是 |
/// |---|---|---|
/// | ① 构建链 | `let mut rows = column(…);` 这条语句链上不许有 `.spacing(` | 直接写在链上的间距（变异 D） |
/// | ② 语句序列 | `list_column()` 里**每一处** `rows.spacing(…)` 的实参都要**顺绑定追回 `ROW_SPACING`** | **另起一条语句**再叠间距（变异 M4，含「先把令牌挪进一条 `let`」的写法） |
///
/// 为什么②要追绑定而不是比字面：合法写法有几种形状——`rows = rows.spacing(ui.space(spacing))`
/// 直接取自 `if let Some(spacing) = ROW_SPACING`，或先 `let spacing = ROW_SPACING;` 再中转
/// （变异 H3）。`layout/source.rs::derived_from` 顺 `let` / `Some(…) =` 两种绑定往回追最多三跳，
/// 于是「真源」放行、「中转了一层 `Space::X1`」判否。
///
/// **抓得住**：构建链上的字面量间距（D）、另起语句的行距（M4）、`ROW_SPACING` / `LIST_BOTTOM_INSET`
/// 两个真源被摘掉；`column![head, scroller]` 上的合法间距不受影响（变异 J，绿）；
/// **抓不住**：把间距算进别处再以非令牌形式传进 `list_row()`（那要改组件几何，代码评审能看出来）、
/// 以及绕过 `rows.spacing(…)` 这个调用形状的写法。
#[test]
fn list_column_spaces_rows_only_from_the_shared_constant() {
    let file = include_str!("../instances.rs");
    let body = source::function_region(file, "fn list_column");
    let rows_chain = source::statement(file, "let mut rows = column(");

    assert!(
        body.contains("ROW_SPACING"),
        "列表行距必须从 ROW_SPACING 取，不许另写：\n{body}"
    );
    assert!(
        body.contains("LIST_BOTTOM_INSET"),
        "底部内缩必须从 LIST_BOTTOM_INSET 取：\n{body}"
    );

    // ① 行容器的构建链上不许叠间距（`column![head, scroller]` 上的合法间距不在这条链里）
    assert!(
        !rows_chain.contains(".spacing("),
        "行容器的构建链上又出现了间距（M102 ② 的页级附加量）：\n{rows_chain}"
    );

    // ② 语句序列：每一处 `rows.spacing(…)` 的实参都必须**顺绑定追回 `ROW_SPACING`**
    //    （直接用它、或 `let spacing = ROW_SPACING` 再中转都算；先 `let extra = Space::X1;`
    //     再 `rows.spacing(ui.space(extra))` 则红——r4 复审的变异 M4）
    let applied = source::call_arguments(&body, "rows.spacing(");
    assert!(
        !applied.is_empty(),
        "`list_column()` 里没有把 ROW_SPACING 应用到行容器的调用：\n{body}"
    );
    for argument in &applied {
        let ident = argument
            .split_once("ui.space(")
            .map(|(_, rest)| rest)
            .unwrap_or(argument);
        let ident: String = ident
            .chars()
            .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
            .collect();

        assert!(
            argument.contains("ui.space("),
            "行容器的间距实参必须走令牌栅格（`ui.space(…)`），收到 {argument:?}"
        );
        assert!(
            !argument.contains("Space::"),
            "行容器的间距实参里直接写了令牌档位：{argument:?}\n{body}"
        );
        assert!(
            source::derived_from(&body, &ident, "ROW_SPACING"),
            "行容器的间距不是从 ROW_SPACING 来的（`{ident}` 追不到 ROW_SPACING）——\
             M102 ② 的页级附加量不许另起一条语句塞回来：{argument:?}\n{body}"
        );
    }
}

/// **散文接线**：详情页的正文段必须进 `frame::prose`（424），不许直接塞进 `frame::body`（1044）。
///
/// M102 把「块宽」与「散文列宽」拆成两个量之后，接错一行代码就会让 361 字符的正文在 1180 窗宽下
/// 排成 170 字符一行，而不红任何测试。这条断言把接线本身钉住。
#[test]
fn the_detail_note_goes_through_the_prose_box() {
    let body = source::function_body(include_str!("../instances/detail.rs"), "fn push_text");

    assert!(
        body.contains("frame::prose("),
        "正文段没有走 frame::prose：\n{body}"
    );
    assert!(
        !body.contains("frame::body("),
        "正文段被塞进了整页块（会失去 72 字符行宽承诺）：\n{body}"
    );
}

/// 元信息卡必须装得下**卡内最长的那条值**（视觉 r2：136 漏算 padding，版本串折行让卡底不齐）。
#[test]
fn meta_card_is_wide_enough_for_the_longest_value() {
    use super::super::layout::{measure_cap_px, META_CARD_MIN_WIDTH, META_GAP};
    use unlucky_design::spacing::LayoutMetrics;

    // 实测（Segoe UI 14px）：demo 数据里最宽的元信息值是加载器版本串
    const LONGEST_VALUE: &str = "NeoForge 20.4.237";
    const LONGEST_VALUE_PX: f32 = 118.0;

    let padding = LayoutMetrics::DESKTOP.card_padding;
    assert_eq!(padding, 20.0, "卡内边距来自令牌");

    // 夹具与 demo 数据不许脱节：这个值必须真在 DEMO 里
    assert!(
        DEMO.iter().any(|entry| entry.loader == LONGEST_VALUE),
        "夹具 {LONGEST_VALUE:?} 已经不在 DEMO 里了，请重新量最长值"
    );

    // ① 单卡外框减掉两侧内边距，必须放得下这条值（否则 text 会按词折行 → 卡底不齐）
    let inner = META_CARD_MIN_WIDTH - 2.0 * padding;
    assert!(
        inner >= LONGEST_VALUE_PX,
        "卡内可用 {inner}px 装不下 {LONGEST_VALUE_PX}px 的 {LONGEST_VALUE:?}（外框 {META_CARD_MIN_WIDTH} − 2×{padding}）"
    );

    // ② 实际版面里每张卡的内宽（2 列时）也要够
    let two_columns = (measure_cap_px() - META_GAP) / 2.0;
    assert!(
        two_columns - 2.0 * padding >= LONGEST_VALUE_PX,
        "两列布局下卡内可用 {}px 装不下 {LONGEST_VALUE_PX}px",
        two_columns - 2.0 * padding
    );
}

#[test]
fn every_demo_entry_has_name_version_and_loader() {
    for entry in &DEMO {
        assert!(!entry.name.is_empty());
        assert!(!entry.version.is_empty());
        assert!(!entry.loader.is_empty());
    }
    assert!(SELECTED < DEMO.len());
}

#[test]
fn detail_paragraph_is_long_enough_to_wrap() {
    // 行宽上限 ≈ 72 字符/行；写够几行才看得出断行位置
    assert!(NOTE.len() > 200, "正文档太短，看不出行宽上限");
}

#[test]
fn narrow_widths_switch_the_page_to_one_column() {
    // 1180 / 900 → 两列；700 → 单列（整页切换）
    assert_eq!(
        frame_metrics(PaneKit::ListDetail, 1180.0).layout,
        PageLayout::ListDetail
    );
    assert_eq!(
        frame_metrics(PaneKit::ListDetail, 700.0).layout,
        PageLayout::SingleColumn
    );
    // 窄窗下详情不吃满窗口：正文列仍 ≤ 行宽上限
    let narrow = frame_metrics(PaneKit::ListDetail, 700.0);
    assert!(narrow.column <= measure_cap_px() + f32::EPSILON);
    assert!(narrow.secondary.is_none());
}
