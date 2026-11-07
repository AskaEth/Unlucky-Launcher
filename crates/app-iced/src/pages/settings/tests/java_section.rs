//! 「Java」分组自己的断言（M100 **新增**：`settings/tests.rs` 里一条 section 断言都没有，
//! 那 7 条全是跨 section 的；所以这 3 条是新增，不是搬移）。
//!
//! **归 java 线**。接真控件那天要改**两个**文件（M101 r2 删掉了原先的第 3 处跨 section 绊线，
//! 见 `tests/mod.rs` 的 `every_line_can_be_wired_alone` 说明）：
//! 1. `settings/java.rs`（`body = Some(java::body)` + 渲染器）；
//! 2. 本文件 `the_java_group_is_still_demo_rows`（把 `is_none()` 改成 `is_some()`）。
//!
//! download / general 同型。
//!
//! 判据（M100 交付里正面回答的那条）：给 java 加一条**新断言** = **只改本文件**。

use super::super::java;
use unlucky_design::icons::IconKind;

/// 这一组的行集合（标签 + 值）。改文案就是改观感，所以逐条钉住，而不是「非空就行」。
///
/// 抓得住：① 四条线并行合并时把别人的行粘到本文件（四份 `GROUP` 长得几乎一样，最容易整块贴错）；
/// ② 漏掉/多出一条行；③ 标签与值错位（`("Default memory", "4 GiB")` 被写成 `"8 threads"`）。
/// 抓不住：值对不对（`Temurin 21.0.4` 是不是本机真检测到的运行时）——那是接线后真数据源的事。
#[test]
fn the_java_group_shows_runtime_memory_and_jvm_arguments() {
    let rows: Vec<(&str, &str)> = java::GROUP.rows.to_vec();
    assert_eq!(
        rows,
        vec![
            ("Runtime", "Temurin 21.0.4"),
            ("Default memory", "4 GiB"),
            ("JVM arguments", "Automatic"),
        ]
    );
}

/// 左列图标必须是 `IconKind::Folder`。
///
/// 抓得住：谁把它换回 `IconKind::Play`——那是视觉复审 P2-3 判过的（`Play` 是填充字形，
/// 和同屏其它描边图标语汇打架；Java 组讲的是运行时安装位置，目录图标语义才对得上）。
/// 抓不住：图标画出来的大小/线宽（那是 tokens 层的事）。
#[test]
fn the_java_group_keeps_the_folder_icon_from_the_visual_review() {
    assert_eq!(java::GROUP.icon, IconKind::Folder);
}

/// **绊线**：这一组还没接真控件，画的是演示行。
///
/// 抓得住：java 线接上了 `body` 却没更新这里的期望（那会让本文件静默变成假绿）。
/// 抓不住：界面上真控件长什么样。
///
/// 接真控件时：把 `is_none()` 改成 `is_some()`，**只改这一行**（M101 r2 起不必再去
/// `tests/mod.rs` 报到：那条跨 section 聚合绊线已删，见那里的 `every_line_can_be_wired_alone`）。
#[test]
fn the_java_group_is_still_demo_rows() {
    assert!(
        java::GROUP.body.is_none(),
        "Java 还没接真控件：接了就改本断言"
    );
}
