//! 导出的用例：真的落盘、真的报字节数、真的在有截断时说一句。
//!
//! 从 `tests.rs` 拆出（那个文件贴着 400 行硬线）。共用父模块的 `report()` 夹具。

use unlucky_i18n::Lang;

use super::report;
use crate::app::dialog::export;

/// 导出：真的落盘、真的报出路径与字节数（**没有 crash-report 也要能导出**）。
#[test]
fn export_writes_a_bundle_even_without_a_crash_report() {
    let root = std::env::temp_dir().join(format!("ull-m187-export-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("logs")).expect("建目录");
    std::fs::write(root.join("logs").join("launcher.log"), "[launch] boom\n").expect("写日志");

    let mut spec_report = report(false, vec!["Error: boom".to_string()]);
    spec_report.launcher_logs = Some(root.join("logs"));
    spec_report.session_log = None;
    spec_report.run_dir = None;
    spec_report.version_json = None;

    let done = export::export(&spec_report, Lang::ZhCn);
    let text = done.text.clone();
    assert!(done.ok, "{text}");
    assert_eq!(done.truncated, 0);
    assert!(text.contains("已导出"), "{text}");
    assert!(text.contains("字节"), "{text}");

    let exports = root.join("logs").join("exports");
    let files: Vec<_> = std::fs::read_dir(&exports)
        .expect("导出目录")
        .filter_map(Result::ok)
        .collect();
    assert_eq!(files.len(), 1, "一次导出只该落一个包");
    let size = files[0].metadata().expect("元数据").len();
    assert!(size > 0, "包不该是空文件");
    assert!(
        text.contains(&size.to_string()),
        "报出来的字节数要与磁盘一致：{text} / {size}"
    );
}

/// P3-4：`truncated > 0` 时**界面上那一句**真的出现（r2 复审登记的那条没有用例）。
///
/// 抓得住：把 `Exported.text` 里「有文件被截断」那句删掉/短路（变异证明见报告）。
/// 抓不住：那句话排版好不好看（视觉）。
#[test]
fn a_truncated_export_says_so_in_the_notice() {
    use unlucky_core::launch::crash_bundle::MAX_ENTRY_BYTES;

    let root = std::env::temp_dir().join(format!("ull-m187-trunc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("logs")).expect("建目录");
    std::fs::create_dir_all(root.join("game").join("logs")).expect("建目录");
    std::fs::write(root.join("logs").join("launcher.log"), "boot\n").expect("写日志");

    // 略超上限的 latest.log：必须用**真实日志行**填充 —— 同一字符的 4 MiB 会被
    // 「长不透明串」脱敏规则整段替换成占位符，长度就不超限了（那测的是脱敏不是截断）。
    let line = "2026-11-08T15:30:12.441Z [Render thread/INFO]: loading chunk\n";
    let mut big = String::new();
    while big.len() < usize::try_from(MAX_ENTRY_BYTES).expect("常数") + 256 {
        big.push_str(line);
    }
    std::fs::write(root.join("game").join("logs").join("latest.log"), &big).expect("写大日志");

    let mut report = report(false, Vec::new());
    report.launcher_logs = Some(root.join("logs"));
    report.run_dir = Some(root.join("game"));
    report.session_log = None;
    report.version_json = None;

    let done = export::export(&report, Lang::ZhCn);
    assert!(done.ok, "{}", done.text);
    assert_eq!(
        done.truncated, 1,
        "这份 latest.log 必须真的被截断（>4 MiB）"
    );
    assert!(
        done.text.contains("已截断"),
        "界面上没有说「有文件被截断」：{}",
        done.text
    );
}
