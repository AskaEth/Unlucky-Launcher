//! M339：core `ImportIssueLevel` → DTO `ImportIssueLevelView` 这条**接缝**的守门用例。
//!
//! 为什么单列一条：core 侧的用例只证 core 判对了档，app-iced 侧的夹具直接造 DTO 级别——
//! 于是 `convert.rs` 里那两个 match 分支写反（`Warning → Note`）**两层各自全绿**，用户在界面上
//! 看到的分档却反了。跨这条缝的用例只有这一份，必须走真函数 [`report_view`]。

use std::path::{Path, PathBuf};

use unlucky_core::addon::import::{keys, ImportIssue, ImportReport};

use crate::dto::ImportIssueLevelView;

use super::report_view;

/// 级别与**顺序**都照 core 那份搬（顺序错 = 界面上分档段落里的条目归错类）。
///
/// 变异：把 `convert.rs` 那两个分支互换 ⇒ 第 2 条断言红（`warnings[0].level` 成了 `Note`）。
/// 变异：把 `.map(...)` 换成只搬 `message` 的旧写法 ⇒ 编译不过（`warnings` 不再是 `Vec<String>`）。
#[test]
fn report_view_carries_the_level_and_keeps_cores_order() {
    let target = PathBuf::from("C:/tmp/instance");
    let mut report = ImportReport::empty(target.clone());
    report.warnings.push(ImportIssue::warning(
        keys::NOTE_LOADER_NOT_INSTALLED,
        "清单要求 fabric 0.15.11，但这次导入没装加载器",
    ));
    report.warnings.push(ImportIssue::note(
        keys::NOTE_REGISTERED,
        "已登记实例 \"Survival\"",
    ));

    let view = report_view(&report, Path::new("C:/tmp/instance"), false);

    assert_eq!(view.warnings.len(), 2, "两条都要搬过来");
    assert_eq!(
        view.warnings[0].level,
        ImportIssueLevelView::Warning,
        "第一条是 core 的告警（加载器没装）"
    );
    assert_eq!(
        view.warnings[1].level,
        ImportIssueLevelView::Note,
        "第二条是 core 的注记（登记成功）"
    );
    // 机器通道：`--json` 与日志靠 slug 认档，别只在枚举层对。
    assert_eq!(view.warnings[0].level.slug(), "warning");
    assert_eq!(view.warnings[1].level.slug(), "note");
    // 原文照搬（这一层不重算任何结论，只翻译级别）。
    assert!(view.warnings[0].message.contains("没装加载器"));
    assert!(view.warnings[1].message.contains("已登记实例"));
}
