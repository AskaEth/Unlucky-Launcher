//! M352：**宿主侧自己造的两条** `ImportIssue`（`instance_bound` / `instance_binding_failed`）的级别。
//!
//! 这两条不在 core：`pack_dir::bind_created_instance` 只给结论，**级别给在宿主侧**
//! （`import.rs` 里那个 `match`），所以 `crates/core` 的级别守门够不到它们。
//!
//! # 依据（不是「代码里写着 Note」）
//!
//! - 绑成功 = 事实确认：实例在列表里、内容在目录里、运行目录也指对了，用户没有待办 ⇒ **注记**；
//! - 绑失败 = 东西用不上：档案在、内容在，只有运行目录没指过来，于是启动时这批 mod 一条都不
//!   生效——用户会以为「导入失败了」，得自己再绑一次 ⇒ **告警**。
//!
//! # 两档的覆盖程度不一样（如实写清）
//!
//! - 成功档走**真管线**（MultiMC 目录包 → 真登记 → 真绑定），这里钉得住；
//! - 失败档**没在这里钉**：要让它红，得在 `register` 成功之后、`bind_instance_dir` 之前把
//!   `instances/<id>/instance.json` 弄坏，而合成夹具无法在同一次 `run` 中间插一脚（没有注入缝）。
//!   这一条在 mission M352 的报告里**登记为已知空洞**，不硬造夹具。
//!
//! 变异：把 `import.rs` 里 `instance_bound` 的 `note` 改成 `warning` ⇒ 本条红。

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::dto::{
    ConflictDecisionsView, ImportIssueView, ImportReportView, ImportRequestView, PackEvent,
};

/// 临时目录（用完删）。
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ull-m352-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// 写文件（自动建父目录）。
fn write(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent");
    }
    std::fs::write(path, bytes).expect("write");
}

/// 跑一次真导入，取回转换成视图的报告（`run` 是同步的，与 `pack/tests.rs` 同一个用法）。
fn run_import(env: &crate::host::HostEnv, pack: &Path, name: &str) -> ImportReportView {
    let (tx, rx) = crate::host::job::channel();
    let request = ImportRequestView {
        pack: pack.to_path_buf(),
        instance: None,
        instance_name: Some(name.to_string()),
        decisions: ConflictDecisionsView::default(),
    };
    super::run(env, &tx, &request, 7, Arc::new(AtomicBool::new(false)));
    drop(tx);
    let mut report = None;
    for task in rx.iter() {
        if let crate::host::job::HostTask::Pack { event, .. } = task {
            if let PackEvent::Report(view) = *event {
                report = Some(*view);
            }
        }
    }
    report.expect("必须有报告事件")
}

/// 从视图里挑出一句话开头是 `lead` 的那条（DTO 不带 key，只能按句子认）。
fn find_note<'a>(warnings: &'a [ImportIssueView], lead: &str) -> &'a ImportIssueView {
    warnings
        .iter()
        .find(|issue| issue.message.starts_with(lead))
        .unwrap_or_else(|| {
            let seen: Vec<&str> = warnings
                .iter()
                .map(|issue| issue.message.as_str())
                .collect();
            panic!("报告里没有以 {lead:?} 开头的注记：{seen:?}")
        })
}

/// 绑定成功是注记：运行目录已指过来，用户没有待办。
#[test]
fn a_bound_instance_is_a_note() {
    let dir = temp_dir("bind-ok");
    let source = dir.join("Better MC");
    // 声明加载器要用网络（第六跳），这里刻意只留原版组件：本用例钉的是绑定那一档。
    write(
        &source.join("mmc-pack.json"),
        br#"{"formatVersion": 1, "components": [
            {"uid": "net.minecraft", "version": "1.20.1"}]}"#,
    );
    write(&source.join(".minecraft/mods/sodium.jar"), &[0x11; 32]);
    let data = dir.join("data");
    std::fs::create_dir_all(&data).expect("data root");
    let env = crate::host::HostEnv::at_root(&data);

    let report = run_import(&env, &source, "Imported Pack");
    assert_ne!(
        report.status,
        crate::dto::ImportStatusView::Failed,
        "{:?}",
        report.error
    );
    let issue = find_note(&report.warnings, "运行目录已指向");
    assert_eq!(
        issue.level,
        crate::dto::ImportIssueLevelView::Note,
        "绑定成功是事实确认（实例列表里已经有了），用户没有待办：{}",
        issue.message
    );

    let _ = std::fs::remove_dir_all(&dir);
}
