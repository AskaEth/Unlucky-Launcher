//! M336：宿主侧失败那一行**来自目录**（台账 P31 finding②）。
//!
//! # 改之前屏上是什么
//!
//! `presenter` 把 `"Target instance is not usable"`（硬编码英文）+ `"import_host"`（内部码）一起
//! 塞进 `ImportErrorView`，界面按 `format!("{} · {}", label, code)` 画成
//! `Target instance is not usable · import_host`：中文界面里混一句英文，尾巴上挂着一串用户看不懂、
//! 也照它做不了任何事的内部 key。
//!
//! # 现在钉住什么
//!
//! | 判据 | 抓得住 | 抓不住 |
//! |---|---|---|
//! | ①中文界面那两行**不含** `import_host` / `Target instance is not usable` | 宿主层把散文或码又漏回 `label`/`message`；模型层把码画上屏 | 那两句以外还有没有别的英文（`Demo` 夹具的其它态是另一笔账） |
//! | ②英文界面那两行**不出现中文** | 目录某一列漏译成中文、或 `render` 落回中文 | 专有名词（品牌）按设计不翻译 |
//! | ③中文界面那行**没有 markdown 反引号** | 谁把路径包成 `\`{detail}\``（纯文本控件会把反引号原样画出来） | 目录表里其它域的文案（`catalog/tests.rs` 的全表守卫管） |
//! | ④码**不上屏** | 有人把 `code` 塞回 `Fatal` 再画出来 | 截图上的版式（那靠 `DevDocs/artifacts/m336/` 的图） |
//!
//! 变异：把 `model/report.rs::host_text` 里任一条 `Key::PackHost*` 删掉（落到 DTO 原文）⇒
//! ①红（`label` 变成 `import_host_occupied`）；把某个中文译文改成英文 ⇒ ②红。

use unlucky_presenter::dto::{
    ImportCancelView, ImportErrorView, ImportFailureStageView, ImportReportView, ImportStatusView,
};

use super::super::report::Fatal;
use super::*;

/// 宿主侧失败的 DTO 形状（与 `presenter::convert::host_failed_report` 逐字段对齐：
/// 码 = label、message 只是细节）。夹具不翻译，翻译在 `report::host_text` 里。
fn host_failure(code: &str, detail: &str) -> ImportReportView {
    ImportReportView {
        status: ImportStatusView::Failed,
        format: None,
        pack_name: Some("Better MC".to_string()),
        pack_version: None,
        game_version: None,
        loader: None,
        loader_installed: false,
        processors: None,
        version_id: None,
        target: PathBuf::new(),
        files: Vec::new(),
        conflicts: Vec::new(),
        missing: Vec::new(),
        rejected: Vec::new(),
        warnings: Vec::new(),
        error: Some(ImportErrorView {
            code: code.to_string(),
            label: code.to_string(),
            message: detail.to_string(),
            hint: None,
        }),
        registered: None,
        bytes_written: 0,
        rolled_back: true,
        failure_stage: Some(ImportFailureStageView::NotStarted),
        cancel: ImportCancelView::NotRequested,
    }
}

/// 四条成因（与 `presenter::import::TargetProblem` 的四个码一一对应）。
const CASES: [(&str, &str); 4] = [
    (
        "import_host_occupied",
        "/home/player/.local/share/unlucky/instances/Better MC",
    ),
    ("import_host_unreadable", "Access is denied. (os error 5)"),
    (
        "import_host_create_failed",
        "The system cannot find the path specified. (os error 3)",
    ),
    ("import_host_instance_gone", "instance not found: survival"),
];

/// 有没有中日韩字符（判据 ② 用；`bridge` 里那份是私有实现，这里只服务断言）。
fn has_cjk(text: &str) -> bool {
    text.chars()
        .any(|ch| matches!(ch as u32, 0x2E80..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF))
}

/// 屏上那一块的全部文字（标题 / 大类 / 具体 / 建议）。
fn drawn(fatal: &Fatal) -> String {
    format!(
        "{} {} {} {}",
        fatal.title,
        fatal.label,
        fatal.message,
        fatal.hint.as_deref().unwrap_or("")
    )
}

#[test]
fn the_host_failure_label_and_detail_come_from_the_catalog() {
    let zh = Lang::ZhCn.catalog();
    let en = Lang::En.catalog();

    for (code, detail) in CASES {
        let report = host_failure(code, detail);

        // ①③ 中文界面：两句人话 + 细节，且没有内部码、没有反引号。
        let zh_block = report_of(&report, &PackPageState::default(), zh);
        let zh_fatal = zh_block.fatal.expect("致命块必须存在");
        assert_eq!(zh_fatal.label, "目标实例不可用", "{code}");
        assert!(
            !zh_fatal.label.contains('`') && !zh_fatal.message.contains('`'),
            "中文界面那行不许有 markdown 反引号：{} / {}",
            zh_fatal.label,
            zh_fatal.message
        );
        // ② 英文界面：两句英文，一个中文字都没有。
        let en_block = report_of(&report, &PackPageState::default(), en);
        let en_fatal = en_block.fatal.expect("致命块必须存在");
        assert_eq!(en_fatal.label, "Target instance is not usable", "{code}");
        assert!(
            !has_cjk(&en_fatal.label) && !has_cjk(&en_fatal.message),
            "英文界面那两行出现了中文：{} / {}",
            en_fatal.label,
            en_fatal.message
        );
        // 英文那一列不是把中文列抄过去（同一句话两种语言都得真的不一样）。
        assert_ne!(zh_fatal.label, en_fatal.label, "{code} 的译文没分语言");

        // 细节进句子：路径 / OS 原文要真的被带出来（带不出来等于丢事实）。
        if code == "import_host_instance_gone" {
            assert!(
                !zh_fatal.message.contains(detail) && !en_fatal.message.contains(detail),
                "这一档的细节是上游英文散文，只进 `--json`，不许上屏：{}",
                zh_fatal.message
            );
        } else {
            assert!(
                zh_fatal.message.contains(detail) && en_fatal.message.contains(detail),
                "{code} 的细节没进句子：{} / {}",
                zh_fatal.message,
                en_fatal.message
            );
        }
        assert_eq!(
            zh_fatal.kind,
            crate::pages::content::packui::model::FatalKind::NotStarted,
            "没进管线 ⇒ 一个字节都没写"
        );
    }
}

/// ①④ 内部码不上屏：哪一档都不许把码画进任何一格。
#[test]
fn the_stable_code_never_reaches_the_screen() {
    let zh = Lang::ZhCn.catalog();
    let en = Lang::En.catalog();
    for (code, detail) in CASES {
        let report = host_failure(code, detail);
        for catalog in [zh, en] {
            let block = report_of(&report, &PackPageState::default(), catalog);
            let fatal = block.fatal.expect("致命块必须存在");
            let text = drawn(&fatal);
            assert!(
                !text.contains(code),
                "内部码 {code} 上屏了（{}）：{text}",
                catalog.lang().tag()
            );
            assert!(
                !text.contains("import_host"),
                "内部码前缀 `import_host` 上屏了：{text}"
            );
            assert!(
                !text.contains("Target instance is not usable ·"),
                "又拼回「大类 · 码」那种尾巴了：{text}"
            );
        }
    }
}

/// 不是这一族（core 的 `pack_*` / `import_*`）时照旧画 DTO 原文——本函数不替 core 兜底。
///
/// core 那几档的 `label`/`message` 是中文散文（`ImportErrorKind::label` 与各处的
/// `ImportError::new(message)`），英文界面里会露出中文：那是**另一笔债**（core 没有结构化出口，
/// 见 `unlucky_i18n::bridge` 的 R2/R3），本用例只钉「宿主那一族改坏了不会顺带把 core 那几档弄丢」。
#[test]
fn a_core_code_still_draws_the_dto_text() {
    let zh = Lang::ZhCn.catalog();
    let mut report = host_failure("pack_traversal", "../etc/passwd");
    // core 的失败不是「码 = label」：它的 label 是 core 自己给的人读名（中文散文）。
    if let Some(error) = report.error.as_mut() {
        error.label = "路径穿越".to_string();
    }
    let block = report_of(&report, &PackPageState::default(), zh);
    let fatal = block.fatal.expect("致命块必须存在");
    assert_eq!(
        fatal.label, "路径穿越",
        "不认识的码 ⇒ 原文照画（不猜、不吞）"
    );
    assert_eq!(fatal.message, "../etc/passwd");
}
