//! 宿主侧失败那一态（M336）：导入**根本没进管线**，目标实例不可用。
//!
//! ## 这一态为什么值得单独出图
//!
//! 台账 P31 finding② 说的是这一行：产品以前把宿主自己拼的**硬编码英文** + **内部码**一起交给
//! 界面，画成 `Target instance is not usable · import_host`——中文界面里混英文，尾巴上挂着一串
//! 用户看不懂、也照它做不了任何事的 key。
//!
//! 修法是把「谁产文案」掰回正轨（宿主给码与细节、目录给句子，见
//! `presenter/.../pack/import.rs::TargetProblem` 与 `catalog/pack_host.rs`），**画法本身没变**：
//! 致命块还是「标题 / 大类名 / 具体是什么」三行。所以要一张中英对照的图，证明的是同一件事：
//! 那两行的字**来自目录**，中文界面里一个英文词都没有、也没有内部码。
//!
//! ## 夹具只造「产品会造的东西」
//!
//! 这一份 `ImportReportView` 的形状与 `presenter::convert::host_failed_report` 逐字段对齐：
//! 码是 `import_host_*`，`label` 与 `code` 同值（宿主侧不产散文），`message` 只有**细节**（那个
//! 已经存在且不空的目录）。夹具不翻译、不拼句——翻译发生在 `packui/model/report.rs::host_text`，
//! 与产品同一个函数。

use std::path::PathBuf;

use unlucky_presenter::dto::{
    ImportCancelView, ImportErrorView, ImportFailureStageView, ImportReportView, ImportStatusView,
};

/// 目标实例不可用（「目录已存在且不空」那一档）。
pub(super) fn occupied() -> ImportReportView {
    ImportReportView {
        status: ImportStatusView::Failed,
        // 没进管线 ⇒ 连格式都没探（`failed_report_with` 的 `format: None`）。
        format: None,
        pack_name: Some("Better MC".to_string()),
        pack_version: None,
        game_version: None,
        loader: None,
        loader_installed: false,
        processors: None,
        version_id: None,
        // 目标目录不可用时宿主**如实给空路径**（不把包路径冒充成实例目录）。
        target: PathBuf::new(),
        files: Vec::new(),
        conflicts: Vec::new(),
        missing: Vec::new(),
        rejected: Vec::new(),
        warnings: Vec::new(),
        error: Some(ImportErrorView {
            code: "import_host_occupied".to_string(),
            // 宿主侧没有人读名可给：这一格与 `code` 同值，界面按码取目录文案。
            label: "import_host_occupied".to_string(),
            // 只有细节：哪个目录。句子由目录给（`PackHostOccupied` 的 `{detail}`）。
            message: "/home/player/.local/share/unlucky/instances/Better MC".to_string(),
            hint: None,
        }),
        registered: None,
        bytes_written: 0,
        rolled_back: true,
        // 预检/装配阶段就失败：一个字节都没写（不是「写完又回滚」）。
        failure_stage: Some(ImportFailureStageView::NotStarted),
        cancel: ImportCancelView::NotRequested,
    }
}
