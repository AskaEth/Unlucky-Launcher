//! 列表列的两条判据：**单行**（四样挤得下）与**搜索框真的在**（不是摆样子）。

use crate::pages::layout::rails::{
    version_list_min_width, version_row_min_width, LIST_DATE_WIDTH, LIST_KIND_WIDTH, ROW_GAP,
    ROW_ICON_BUTTON,
};

/// 行宽下限 424 之下，四样东西仍在一行里（并留出够放 `1.21.9-pre3` 的版本号格）。
///
/// 抓得住：有人把类型 / 日期 / 按钮的格子改宽（改到 424 放不下时这条红），或者把行宽下限调小。
/// 抓不住：真的渲染出来会不会折行（`ListItem` 的 `Wrapping::None` 是组件层的事，成图在报告里）。
#[test]
fn the_version_row_still_fits_on_one_line_at_the_floor() {
    let floor = version_list_min_width();
    assert_eq!(floor, 424.0, "行宽下限就是既有的 424");

    let needed = version_row_min_width();
    assert!(
        needed <= floor,
        "四样固定格加起来要 {needed}，超过行宽下限 {floor} 就得折行"
    );

    // 版本号那一格在最窄情况下还剩多少
    let label_room = floor - LIST_KIND_WIDTH - LIST_DATE_WIDTH - ROW_ICON_BUTTON - 3.0 * ROW_GAP.dp();
    assert!(
        label_room >= 176.0,
        "版本号那一格只剩 {label_room}，`1.21.9-pre3`（11 字符）会被切"
    );
}

/// 搜索条出现在列表列里（源码层面钉住：「有搜索框」这件事不能悄悄被删掉）。
///
/// 抓得住：有人把搜索条从列表列里拿掉（那一整块需求就没了）。
/// 抓不住：搜索条是否**生效**（效果由 `download/tests.rs` 的跨类型命中数断言与出图负责）。
#[test]
fn the_search_bar_is_actually_mounted() {
    let source = include_str!("../rows.rs");
    assert!(source.contains("search_bar("), "搜索条没装");
    assert!(source.contains("Message::SetVersionQuery"), "搜索条不产生查询消息");
    assert!(source.contains("Message::ClearVersionQuery"), "搜索条没有清空入口");
}
