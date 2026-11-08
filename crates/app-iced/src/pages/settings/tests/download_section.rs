//! 「Downloads」分组自己的断言（M100 **新增**：`settings/tests.rs` 那 7 条全是跨 section 的，
//! 一条 section 断言都没有；这 3 条是新增，不是搬移）。
//!
//! **归 download 线**。接真控件那天要改**两个**文件（M101 r2 删掉了原先的第 3 处跨 section 绊线）：
//! `settings/download.rs` + 本文件 `the_downloads_group_is_still_demo_rows`。
//!
//! 别把它和下载页（`pages/download*`）搞混：那是「装哪个版本」，这一组是「怎么下」的默认值。

use super::super::download;
use unlucky_design::icons::IconKind;

/// 这一组的行集合（标签 + 值）。
///
/// 抓得住：① 四条线并行合并时把别人的行粘到本文件（四份 `GROUP` 长得几乎一样，最容易整块贴错）；
/// ② 漏掉/多出一条行；③ 标签与值错位（`("Concurrency", "8 threads")` 被写成 `"Off"`）。
/// 抓不住：值对不对（`Automatic (region)` 是不是本机真选中的源）——那是接线后真数据源的事。
#[test]
fn the_downloads_group_shows_source_concurrency_and_checksums() {
    let rows: Vec<(&str, &str)> = download::GROUP.rows.to_vec();
    assert_eq!(
        rows,
        vec![
            ("Preferred source", "Automatic (region)"),
            ("Concurrency", "8 threads"),
            ("Verify checksums", "On"),
        ]
    );
}

/// 左列图标（`IconKind::Download`）。与导航轨的「下载」同族，换掉就与 rail 语汇不一致。
#[test]
fn the_downloads_group_keeps_its_icon() {
    assert_eq!(download::GROUP.icon, IconKind::Download);
}

/// **绊线**：这一组还没接真控件，画的是演示行。
///
/// 抓得住：download 线接上了 `body` 却没更新这里的期望（那会让本文件静默变成假绿）。
/// 抓不住：界面上真控件长什么样。
///
/// 接真控件时：把 `is_none()` 改成 `is_some()`，**只改这一行**（M101 r2 起不必再去
/// `tests/mod.rs` 报到：那条跨 section 聚合绊线已删）。
#[test]
fn the_downloads_group_is_still_demo_rows() {
    assert!(
        download::GROUP.body.is_none(),
        "Downloads 还没接真控件：接了就改本断言"
    );
}
