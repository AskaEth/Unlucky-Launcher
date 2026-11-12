//! 「Java」分组自己的断言（M100 起归 java 线；M175 重写）。
//!
//! M101 的三条里有两条在 M175 换掉了：
//!
//! | 旧断言 | M175 之后 |
//! |---|---|
//! | `the_java_group_shows_runtime_memory_and_jvm_arguments`（逐字钉住 `Temurin 21.0.4` / `4 GiB`） | 换成「演示行必须是空的」——那三行假数据正是用户实测投诉的源头 |
//! | `the_java_group_is_still_demo_rows`（断言 `body.is_none()`） | 保留（`body` 仍是 `None`：本组走按下标分发，见 `settings/java.rs` 的文件头） |
//! | `the_java_group_keeps_the_folder_icon_from_the_visual_review` | 不动 |
//!
//! 真数据源的断言在 `pages/settings/java/tests.rs`（扫描 / 校验 / 落盘 / 真命令行），
//! 这里只守「左列那一格」——两处的分工与 M100 的 section 划分一致。

use super::super::java;
use unlucky_design::icons::IconKind;

/// 这一组**不再有演示行**：三个假值（运行时 / 内存 / JVM 参数）已经换成真控件。
///
/// 抓得住：谁把演示行加回来（那会让 harness 路径又画出一份「看起来像真检测」的假数据）；
/// 也抓得住「接真控件却把 `rows` 留着」这种半吊子状态。
/// 抓不住：真控件里画了什么（那是 `pages/settings/java/tests.rs` 的事）。
#[test]
fn the_java_group_has_no_demo_rows_left() {
    assert!(
        java::GROUP.rows.is_empty(),
        "Java 组不该再有演示行（真值来自扫描 / 手动路径 / launcher.json）：{:?}",
        java::GROUP.rows
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

/// `body` 槽位仍是 `None`：本组靠 `settings/mod.rs` 的按下标分发（[`java::JAVA_GROUP`]）画正文。
///
/// 抓得住：谁把 `body: Some(..)` 挂上来（M179 之后槽位自带 `Lang`，所以「挂了 body 却没语言」
/// 这种漂移不再可能——挂上来就必然拿到语言；这条守的是**分发形状**别变）。
/// 抓不住：分发那一行本身有没有被删（`settings/mod.rs` 的 `form` 里有 `else if`；删了这组就空白）。
#[test]
fn the_java_group_is_dispatched_by_index_not_by_body() {
    assert!(
        java::GROUP.body.is_none(),
        "Java 组走按下标分发（要 Lang），body 必须保持 None"
    );
}
