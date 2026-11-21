//! 导入/导出区的守门（M113）：四种出图态都装得满、入口文案都在。
//!
//! 出图（D30）靠这几条夹具：任何一态装不满，截图就会是一块空容器，而不是「有话说」的界面。
//! 视图模型的判定在 `model/tests.rs`，这里只管**夹具本身**。
//!
//! ## M333 起多一条腿：段被画出来的**结构**断言
//!
//! 原先「报告里那一段真的画出来了」只能靠截图人眼比对——把 `view/report.rs` 里
//! `block.push(processor_block(..))` 那行删掉，用例一条都不红（`Element` 不透出内容；文案虽然
//! 躺在控件的 `Tree::state` 里，却是 `paragraph::Plain` 的私有字段、没有访问器）。现在段自己
//! 往 `Tree::state` 交一份 [`marker::SectionMark`]（见 `packui/marker.rs`），用例顺着**真控件树**读回来：
//!
//! | 用例 | 抓得住 | 抓不住 |
//! |---|---|---|
//! | [`a_report_with_a_processor_chain_really_paints_that_section`] | 这一段**被 `push` 进了结果块**（删那行即红）、段挂了几条；没有链的态里一段都没有（标记跟着段走，不是恒有常量） | 段头那句**文案**、位置与像素（`DevDocs/artifacts/m333/` 的图守） |
//! | [`an_empty_processor_chain_still_paints_the_section_and_says_so`] | **零步骤**那一支（`rows == 0`）：段仍然画出来、且「没有步骤」那句真的挂上了（树里少一行即红） | 那句的文案、版式与像素 |
//! | [`the_processors_state_carries_the_processor_section`] | `processors` 出图态的**夹具**真的带着处理器段（不再只断「非 idle」：夹具丢了这一段，截图会静静退回另一种形态） | 它画出来对不对 |

use unlucky_design::color::SchemeFlavor;
use unlucky_design::typography::Platform;
use unlucky_i18n::Lang;

use crate::theme::{Families, Ui};

use super::demo::pack_in;
use super::marker::{self, PaintedSection};
use super::model::Pack;

mod m341;
mod m345;

/// 出图态：导入中 / **导入前冲突清单（M160）** / **多注记压力态（M345）** / 部分成功 /
/// 导入后冲突账 / **加载器安装步骤（M328）** / **宿主侧失败（M336）** / 导出预检。
const STATES: [&str; 8] = [
    "importing",
    "pick",
    "pick_many",
    "partial",
    "conflict",
    "processors",
    "host_failed",
    "export",
];

#[test]
fn every_screenshot_state_fills_something() {
    for state in STATES {
        let (model, _page) = pack_in(state, Lang::En);
        assert!(
            !model.is_idle(),
            "{state} 这一态什么都没画（截图会是一块空容器）"
        );
        assert_eq!(model.targets.len(), 2, "{state} 的实例候选不足");
        assert!(!model.drop_hint.is_empty());
        assert!(
            !model.no_dialog_note.is_empty(),
            "没有系统文件框这件事必须写在界面上"
        );
    }
}

#[test]
fn the_pick_state_shows_a_paginated_conflict_list() {
    // M160 的那一屏：只画一页、三个入口都在、计数与逐项状态一致。
    let (model, page) = pack_in("pick", Lang::En);
    let pick = model.pick.expect("pick 态必须有冲突清单");
    // 一屏只画一页：行数上限来自 `model::pick::PAGE_SIZE`（40）。
    assert_eq!(pick.rows.len(), 40, "一屏一页（PAGE_SIZE）");
    assert_eq!(pick.pages, 8, "312 条 / 40 条一页");
    assert_eq!(pick.page, 1, "夹具停在第 2 页（0 起）");
    assert!(pick.can_prev && pick.can_next);
    assert_eq!(pick.overwrite_count, 2, "夹具逐项点了两条覆盖");
    assert!(
        pick.keep_all_active && !pick.overwrite_all_active,
        "整批档停在缺省那一档（全部保留）"
    );
    assert_eq!(
        pick.rows[0].choice,
        unlucky_presenter::dto::ConflictChoiceView::KeepUser,
        "没点过的那一行必须是保留"
    );
    // 页面自留的选择与模型一致（逐项那条显示成覆盖）
    assert_eq!(
        page.pick_choice("config/generated/module-041.toml"),
        unlucky_presenter::dto::ConflictChoiceView::Overwrite
    );
}

#[test]
fn importing_shows_the_shared_install_panel() {
    let (model, _page) = pack_in("importing", Lang::En);
    let running = model.running.expect("导入中要有进度");
    // 进度形状来自下载页那块面板（`Download.install`），不是本区另造的
    assert!(
        running
            .panel
            .as_ref()
            .is_some_and(|panel| panel.install.is_some()),
        "进度必须走下载页的安装面板视图模型"
    );
    assert!(
        running.stage_line.contains("2/6"),
        "解包阶段是第 2/6 步：{}",
        running.stage_line
    );
    assert!(model.report.is_none(), "导入中不该同时有结果");
}

#[test]
fn partial_keeps_the_tolerable_rows_one_by_one() {
    let (model, _page) = pack_in("partial", Lang::En);
    let report = model.report.expect("部分成功要有报告");
    assert!(report.installed(), "Partial 仍是「装上了」");
    assert!(report.fatal.is_none(), "部分成功不是致命失败");
    assert_eq!(report.missing.len(), 3, "缺文件要逐条列出");
    assert!(report.missing_title.is_some());
    assert!(!report.rejected.is_empty(), "被拒 component 也要逐条");
}

#[test]
fn conflict_state_defaults_to_keeping_the_user_copy_and_expands_one_row() {
    let (model, page) = pack_in("conflict", Lang::En);
    let report = model.report.expect("冲突态要有报告");
    assert_eq!(report.conflicts.len(), 3);
    assert!(
        report.conflicts.iter().all(|row| !row.expanded)
            || report.conflicts.iter().filter(|row| row.expanded).count() == 1,
        "只展开用户点过的那一条"
    );
    assert_eq!(
        report.conflicts.iter().filter(|row| row.expanded).count(),
        1,
        "夹具要展开一条，截图才看得见三方哈希"
    );
    assert!(page.expanded.is_some());
    assert!(report.can_overwrite);
    assert!(report.overwrite_note.is_some());
}

#[test]
fn export_state_has_a_preview_but_has_not_written_anything() {
    let (model, _page) = pack_in("export", Lang::En);
    assert!(model.export.open);
    let preview = model.export.preview.expect("预检要有结果");
    assert!(
        preview.head.contains('2'),
        "内联数要进主结论：{}",
        preview.head
    );
    assert!(!preview.skipped.is_empty(), "无法导出的项要逐条");
    assert!(!preview.complete, "有跳过项 ⇒ 包不完整");
    assert!(!preview.notes.is_empty(), "已知损失要列出来");
    assert!(
        model.export.done.is_none(),
        "预检阶段不许出现「已写出」（那是写盘之后的事）"
    );
}

#[test]
fn default_state_offers_the_entry_with_nothing_else() {
    let (model, page): (Pack, _) = pack_in("", Lang::En);
    assert!(model.is_idle());
    assert!(!model.export.open);
    assert!(page.draft.is_empty());
}

/// 测试用的 `Ui`（与页面其它守门同一档：浅色 + Windows + 本机字体）。
const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

fn ui() -> Ui {
    Ui::with_platform(SchemeFlavor::Light, Platform::Windows, FAMILIES)
}

/// `state` 那一态**画出来**的控件树里带着哪几段（走产品路径 `view::section`，不是单独调某一段）。
fn painted_sections(state: &str) -> Vec<PaintedSection> {
    let (model, _page) = pack_in(state, Lang::En);
    painted_model(&model)
}

/// 已造好的模型 → 那一帧画出来的段。
///
/// 存在的理由（M341 r1 复审 p2-2）：断言「这一屏里一段都没有」之前，必须先证明**这一屏真的建出来了**，
/// 否则「模型是空的 ⇒ 天然没有段」会让那条判据恒真。拿同一个模型去画，两件事说的才是同一屏。
fn painted_model(model: &Pack) -> Vec<PaintedSection> {
    let ui = ui();
    let element = super::section(&ui, Lang::En.catalog(), model);
    marker::painted(&element)
}

/// 结构级断言（M333 P33-1）：报告里有处理器链 ⇒ 这一帧的树里**真的**带着那一段。
///
/// **变异证明**：删掉 `view/report.rs` 里 `block = block.push(processor_block(ui, section));`
/// 那一行 ⇒ `processor_block` 再也不被调用 ⇒ 树里一个 [`marker::SectionMark`] 都没有，本用例第一段
/// 立刻红（`marks` 为空表）。反过来，标记是**跟着段走**的、不是恒有的常量：`partial` 那一态
/// 没有处理器链，同一份走法必须读到空表（否则上面那条第一段就永远绿）。
#[test]
fn a_report_with_a_processor_chain_really_paints_that_section() {
    let marks = painted_sections("processors");
    assert_eq!(
        marks.len(),
        1,
        "报告里有处理器段 ⇒ 树里正好一段（零 = 那行 `push` 没接线）：{marks:?}"
    );
    assert_eq!(marks[0].mark.name, marker::PROCESSORS, "段的身份");
    assert_eq!(marks[0].mark.rows, 3, "夹具是两跑一跳：三段都要进树");
    assert_eq!(
        marks[0].lines, 4,
        "有步骤时挂四行：段头 / Java 行 / 汇总 / 步骤列表"
    );

    assert!(
        painted_sections("partial").is_empty(),
        "没有处理器链的那一态，树里一段都不许有：标记必须跟着段走"
    );
}

/// **空处理器链**（`rows == 0`：链跑了却零步骤）那一支也走产品路径出结论（M337）。
///
/// 这一段以前只有「人工看图」守着：`view/report_processor.rs` 的 `body` 里
/// `if let Some(empty) = &section.empty` 那一支画的是「没有步骤」那句，删掉它或者把整段不画，
/// 没有任何用例会红。现在断言两件事，都是**从真控件树读回来的**：
///
/// 1. **段还在**（树里能收集到这一段、`rows == 0`）——整段不画的话这里收集不到；
/// 2. **那一句画了**（这一段挂 4 行：段头 / Java 行 / 汇总 /「没有步骤」）——删掉那句的 `push`
///    就只剩 3 行。
///
/// 抓不住：那一句的**文案**（`text` 的状态是私有的 `paragraph::Plain`，没有访问器）；版式与像素。
#[test]
fn an_empty_processor_chain_still_paints_the_section_and_says_so() {
    let painted = painted_sections("processors_empty");
    assert_eq!(
        painted.len(),
        1,
        "零步骤也是「这一段画出来了」：整段不画就会收集不到：{painted:?}"
    );
    assert_eq!(painted[0].mark.name, marker::PROCESSORS, "段的身份");
    assert_eq!(
        painted[0].mark.rows, 0,
        "链跑了却一步都没列 ⇒ 条数是 0（界面据此走 `empty` 那一句）"
    );
    assert_eq!(
        painted[0].lines, 4,
        "零步骤时挂四行：段头 / Java 行 / 汇总 /「没有步骤」那句；只挂 3 行 = 那句没画"
    );

    // 对照：同一份夹具非空时也是 4 行，但第四行是步骤列表——所以上面那条断言抓住的是
    // 「空态那一支没画」，不是「这里恒为 4」。
    let with_steps = painted_sections("processors");
    assert_eq!(with_steps[0].lines, 4);
    assert_eq!(with_steps[0].mark.rows, 3);
}

/// 出图态（M333 P33-3）：`processors` 这一态的**夹具**真的带着处理器段。
///
/// 不再只断「非 idle」：夹具哪天丢了 `processors`，截图会静静退回另一种形态（报告还在、只是
/// 少了这一段），而 `every_screenshot_state_fills_something` 一条都不会红。
#[test]
fn the_processors_state_carries_the_processor_section() {
    let (model, _page) = pack_in("processors", Lang::En);
    let report = model.report.expect("processors 这一态要有报告");
    let section = report
        .processor
        .as_ref()
        .expect("processors 这一态的夹具必须带着处理器段（否则截图静静退回另一形态）");
    assert_eq!(section.steps.len(), 3, "夹具是两跑一跳");
    assert!(section.empty.is_none(), "有步骤就不该走空态那一句");
    assert!(!section.title.is_empty(), "段头要有话");
    assert!(!section.java_line.is_empty(), "用哪份 Java 要印出来");
    assert!(!section.summary.is_empty(), "跑几步/跳几步要印出来");
}

/// M336：`host_failed` 这一态是「目标实例不可用」那一屏的**唯一**出图证据。
///
/// 真实路径上要撞一个「已存在且不空」的目录才出现，截图夹具够不着；夹具因此按
/// `presenter::convert::host_failed_report` 的 DTO 形状造一份（码 = label、message 只是细节）。
///
/// 抓得住：夹具里那句人话确实来自目录（中文那两行无内部码、无反引号；英文那两行没有中文）——
/// 谁把 `demo/host.rs` 改成自己编一句英文，本用例红。
/// 抓不住：版式与像素（`DevDocs/artifacts/m336/` 的图守）。
#[test]
fn the_host_failure_state_draws_the_catalog_line() {
    let (zh, _) = pack_in("host_failed", Lang::ZhCn);
    let fatal = zh
        .report
        .expect("host_failed 这一态要有报告")
        .fatal
        .expect("宿主侧失败是致命块");
    assert_eq!(fatal.label, "目标实例不可用");
    assert!(
        !fatal.label.contains("import_host") && !fatal.message.contains("import_host"),
        "中文界面那两行不许出现内部码：{} / {}",
        fatal.label,
        fatal.message
    );
    assert!(
        !fatal.label.contains('`') && !fatal.message.contains('`'),
        "反引号会原样上屏：{} / {}",
        fatal.label,
        fatal.message
    );

    let (en, _) = pack_in("host_failed", Lang::En);
    let fatal_en = en
        .report
        .expect("host_failed 这一态要有报告")
        .fatal
        .expect("宿主侧失败是致命块");
    assert_eq!(fatal_en.label, "Target instance is not usable");
    assert!(
        fatal_en.label.is_ascii() && fatal_en.message.is_ascii(),
        "英文界面那两行不许出现中文：{} / {}",
        fatal_en.label,
        fatal_en.message
    );
}

/// M336：`partial` 那一态里的注记是 **core 的原话**，不是夹具自己编的英文。
///
/// 以前夹具写 `` "`server-overrides/` was not unpacked (client import)" ``——产品从来不会画这句
/// （core 给的是中文原话），于是那张图既示范了一句不存在的文案、又示范了一对反引号。
///
/// 抓得住：有人把夹具改回自编英文（含反引号）。抓不住：core 改了那句措辞（跨 crate 抄写，要
/// `plan.rs` 与这里同步——见 `demo.rs` 里那条注释）。
#[test]
fn the_partial_state_keeps_cores_real_note() {
    let (model, _) = pack_in("partial", Lang::ZhCn);
    let report = model.report.expect("partial 要有报告");
    // M339：core 那句 `NOTE_SERVER_OVERRIDES_SKIPPED` 是 `Note` 档 ⇒ 落在 `notes`，不进 `warnings`。
    assert!(
        report
            .warnings
            .iter()
            .all(|line| !line.contains("server-overrides/")),
        "注记档的东西不许混进告警段：{:?}",
        report.warnings
    );
    assert_eq!(report.notes.len(), 1, "夹具只有一条注记");
    let note = report.notes[0].as_str();
    assert!(
        note.contains("server-overrides/ 里的 7 个文件"),
        "注记要用 core 真产出的那句（`plan.rs` 的 `NOTE_SERVER_OVERRIDES_SKIPPED`）：{note}"
    );
    assert!(!note.contains('`'), "反引号会原样上屏：{note}");
    assert!(
        !note.contains("was not unpacked"),
        "夹具不许自己编一句英文：{note}"
    );
}

/// **M339 判据①②**：报告的两段按 **core 给的级别**切，计数各自算。
///
/// 夹具里一告警（`ImportIssueLevelView::Warning`）+ 一注记（`Note`）：
/// - ① 告警那条必须出现在「告警」段，且标题里的数字是**告警 1 条**（不是注记；
///   `告警：1 条` 与 `注记：1 条` 是两句不同的话，所以错档必红）；
/// - ② 注记那条必须出现在「注记」段。
///
/// 抓得住（变异）：把 `model/report_issue.rs` 的 `filter(|issue| issue.level == level)` 改成
/// 不筛（或把 `Warning`/`Note` 两处取反）⇒ 告警段里会同时出现注记那句、或两段互换 ⇒ 本条红。
/// 抓不住：core 在构造点把某条 issue 判错档（跨 crate；那四条由
/// `unlucky-core` 的 `addon::import::report::issue::tests` 钉住，
/// 其中 `loader_installed_is_a_note_but_not_installed_is_a_warning` 直接钉「加载器没装 = 告警」）。
#[test]
fn the_report_splits_issues_by_level_and_counts_each() {
    let (model, _) = pack_in("partial", Lang::ZhCn);
    let report = model.report.expect("partial 要有报告");
    assert_eq!(
        report.warnings_title.as_deref(),
        Some("要处理：1 条"),
        "告警段标题要按告警条数算（文案在目录 `PackReportWarnings`）"
    );
    assert_eq!(
        report.notes_title.as_deref(),
        Some("只是告知：1 条"),
        "注记段标题要按注记条数算（文案在目录 `PackReportNotes`）"
    );
    assert_eq!(report.warnings.len(), 1, "告警段只有夹具那一条");
    assert!(
        report.warnings[0].contains("缺下载地址，已降级"),
        "告警段要收 `Warning` 档那条：{:?}",
        report.warnings
    );
    assert!(
        report.notes[0].contains("server-overrides/"),
        "注记段要收 `Note` 档那条：{:?}",
        report.notes
    );
}
