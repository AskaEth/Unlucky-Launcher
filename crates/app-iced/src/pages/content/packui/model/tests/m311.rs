//! M311：报告里的**两件补漏**——「解到哪个目录 / 登记的实例名」与「这个版本的加载器还没装」。
//!
//! 从 `tests.rs` 拆出（那边贴着 400 行硬线）：夹具（`catalog` / `minimal_report`）仍在父模块，
//! 本文件只管这两条判据。变异证明写在每条用例的文档里。

use std::path::PathBuf;

use unlucky_presenter::dto::{
    ImportCancelView, ImportErrorView, ImportReportView, ImportStatusView, PackFormatView,
};

use super::super::report::report_of;
use super::{catalog, PackPageState};

/// 报告要能说清**解到哪个目录 / 登记的实例名**（M311 GUI 补漏②）。
///
/// 变异证明：把 `report_of` 里 `target_line` / `registered_line` 两行删掉 ⇒ 本用例红
/// （DTO 一直有这两个字段，是页面视图模型以前没接）。
#[test]
fn the_report_says_where_it_landed_and_what_the_instance_is_called() {
    let mut report = minimal_report(ImportStatusView::Success);
    report.target = PathBuf::from("D:/ULL/instances/better-mc");
    report.registered = Some("Better MC".to_string());

    let block = report_of(&report, &PackPageState::default(), catalog());
    assert!(
        block
            .target_line
            .as_deref()
            .is_some_and(|line| line.contains("D:/ULL/instances/better-mc")),
        "要显示解到哪个目录：{:?}",
        block.target_line
    );
    assert_eq!(
        block.registered_line.as_deref(),
        Some(
            catalog()
                .render(
                    unlucky_i18n::Key::PackReportRegistered,
                    &[("name", "Better MC")]
                )
                .as_str()
        ),
        "要显示登记的实例名"
    );

    // 没登记就不画那一行（不留一句空话）。
    report.registered = None;
    let block = report_of(&report, &PackPageState::default(), catalog());
    assert!(block.registered_line.is_none());
}

/// 没有目标目录 ⇒ **「解到 …」那一整句不出现**（M336 r1 复审 nit 2）。
///
/// 以前 `target_line` 是无条件 `catalog.render(..)`：宿主侧失败那几档宿主**如实给空路径**
/// （不把包路径冒充成实例目录），于是中文界面画出一行只有标签的「解到 」、英文画
/// 「Unpacked into 」——一个没有值的标签是坏掉的行。
///
/// 变异证明：把 `report_of` 里那句 `(!report.target.as_os_str().is_empty()).then(..)` 改回
/// 无条件 `Some(catalog.render(..))` ⇒ 本用例红（`Some("解到 ")`）。
#[test]
fn an_empty_target_drops_the_whole_line_instead_of_a_bare_label() {
    let zh = unlucky_i18n::Lang::ZhCn.catalog();
    let mut report = minimal_report(ImportStatusView::Failed);
    report.target = PathBuf::new();
    let block = report_of(&report, &PackPageState::default(), zh);
    assert!(
        block.target_line.is_none(),
        "没有目录就不许画那一行（标签也不画）：{:?}",
        block.target_line
    );

    // 有目录时照常画（别把上面那条修成「永远不画」）。
    report.target = PathBuf::from("D:/ULL/instances/better-mc");
    let block = report_of(&report, &PackPageState::default(), zh);
    assert!(
        block
            .target_line
            .as_deref()
            .is_some_and(|line| line.contains("D:/ULL/instances/better-mc")),
        "有目录时必须画：{:?}",
        block.target_line
    );
}

/// 「这个版本的加载器还没装」提示（M311 GUI 补漏①，与 CLI `pack import` 同一条判据）。
///
/// 三档各验一次：声明了但没装 ⇒ 有提示；真装上了 ⇒ 没有；失败 ⇒ 不重复提示（致命块已说清）。
#[test]
fn the_loader_hint_only_appears_when_it_is_declared_but_not_installed() {
    let mut report = minimal_report(ImportStatusView::Success);
    report.loader = Some("fabric-loader 0.15.11".to_string());
    report.loader_installed = false;
    let block = report_of(&report, &PackPageState::default(), catalog());
    assert_eq!(
        block.loader_line.as_deref(),
        Some(catalog().text(unlucky_i18n::Key::PackLoaderNotInstalled)),
        "清单要了加载器却没装，必须提示"
    );

    report.loader_installed = true;
    let block = report_of(&report, &PackPageState::default(), catalog());
    assert!(block.loader_line.is_none(), "真装上了就不该再提示");

    // 装不上是致命失败：原因在致命块里（`import_loader`），这里不重复说一句"还没装"。
    let mut failed = minimal_report(ImportStatusView::Failed);
    failed.loader = Some("fabric-loader 0.15.11".to_string());
    failed.loader_installed = false;
    failed.error = Some(ImportErrorView {
        code: "import_loader".to_string(),
        label: "加载器安装失败".to_string(),
        message: "加载器 Fabric 0.15.11 没装上".to_string(),
        hint: Some("装好再重导一次".to_string()),
    });
    let block = report_of(&failed, &PackPageState::default(), catalog());
    assert!(block.loader_line.is_none(), "失败态由致命块说，不重复提示");
    assert!(block.fatal.is_some());
}

/// 一份最小报告（只填 M311 两条要用的字段，其余留空）。
fn minimal_report(status: ImportStatusView) -> ImportReportView {
    ImportReportView {
        status,
        format: Some(PackFormatView::Modrinth),
        pack_name: Some("Better MC".to_string()),
        pack_version: None,
        game_version: Some("1.20.1".to_string()),
        loader: None,
        loader_installed: false,
        // M328：这条用例只看 M311 的信号行，处理器段显式表态「这一态没有」（不许 `..Default`）。
        processors: None,
        version_id: None,
        target: PathBuf::from("C:/tmp"),
        files: Vec::new(),
        conflicts: Vec::new(),
        missing: Vec::new(),
        rejected: Vec::new(),
        warnings: Vec::new(),
        error: None,
        registered: None,
        bytes_written: 0,
        rolled_back: false,
        failure_stage: None,
        cancel: ImportCancelView::NotRequested,
    }
}
