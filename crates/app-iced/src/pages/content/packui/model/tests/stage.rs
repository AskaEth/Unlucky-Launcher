//! M161：导入失败的**两种阶段**不许共用一句话。
//!
//! 从 `tests.rs` 拆出（那边贴着 400 行硬线）：本文件只守「预检没过（一个字节都没写）」与
//! 「已回滚」的措辞/图标分野。夹具在父模块（`use super::*`）。

use unlucky_presenter::dto::{ImportCancelView, ImportFailureStageView};

use super::*;

/// **「预检没过」与「已回滚」必须各说各的话**（M161 收的债）。
///
/// 此前两者共用 `rolled_back: true`，于是预检失败（一个字节都没写）也被写成
/// 「已回滚 —— 目标目录零残留」——一个没发生的回滚。现在标题、图标、底色三处都分开，
/// 且 `NotStarted` 的标题**不许**出现「回滚 / rolled back」这类词（那才是真正的谎）。
#[test]
fn the_two_failure_stages_never_share_one_sentence() {
    fn failed(stage: Option<ImportFailureStageView>) -> ImportReportView {
        ImportReportView {
            status: ImportStatusView::Failed,
            format: Some(PackFormatView::Modrinth),
            pack_name: Some("Broken".to_string()),
            pack_version: None,
            game_version: None,
            loader: None,
            version_id: None,
            target: PathBuf::from("C:/tmp"),
            files: Vec::new(),
            conflicts: Vec::new(),
            missing: Vec::new(),
            rejected: Vec::new(),
            warnings: Vec::new(),
            error: Some(ImportErrorView {
                code: "pack_traversal".to_string(),
                label: "path traversal".to_string(),
                message: "../etc/passwd".to_string(),
                hint: None,
            }),
            registered: None,
            bytes_written: 0,
            rolled_back: true,
            failure_stage: stage,
            cancel: ImportCancelView::NotRequested,
        }
    }

    let zh = Lang::ZhCn.catalog();
    let not_started = report_of(
        &failed(Some(ImportFailureStageView::NotStarted)),
        &PackPageState::default(),
        zh,
    );
    let rolled_back = report_of(
        &failed(Some(ImportFailureStageView::RolledBack)),
        &PackPageState::default(),
        zh,
    );
    let incomplete = report_of(
        &failed(Some(ImportFailureStageView::RollbackIncomplete)),
        &PackPageState::default(),
        zh,
    );

    // 三种标题互不相同（三态压力：谁把它们折叠回同一句，这里就红）。
    let titles = [
        not_started.title.as_str(),
        rolled_back.title.as_str(),
        incomplete.title.as_str(),
    ];
    assert_eq!(
        titles
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        3,
        "三种失败阶段的标题撞了：{titles:?}"
    );

    // 「预检没过」那一句里不许出现「回滚」（那是没发生的事）。
    let fatal = not_started.fatal.expect("致命块");
    assert_eq!(
        fatal.kind,
        crate::pages::content::packui::model::FatalKind::NotStarted
    );
    assert!(
        !fatal.title.contains("回滚"),
        "预检失败不该说「回滚」：{}",
        fatal.title
    );
    assert!(!not_started.title.contains("回滚"), "{}", not_started.title);
    // 已回滚那一句必须说清「回滚」这件事。
    assert!(
        rolled_back.title.contains("回滚"),
        "已回滚那句要说明回滚：{}",
        rolled_back.title
    );
    // 图标分开：预检没过不是「错误」字形（目录没动过），回滚不完整才是。
    assert_ne!(
        crate::pages::content::packui::model::FatalKind::NotStarted.icon(),
        crate::pages::content::packui::model::FatalKind::RolledBack.icon(),
        "两种阶段的图标不许一样"
    );

    // 成功/部分成功没有「失败阶段」可言。
    let mut ok = failed(None);
    ok.status = ImportStatusView::Success;
    ok.error = None;
    assert!(report_of(&ok, &PackPageState::default(), zh)
        .fatal
        .is_none());
}
