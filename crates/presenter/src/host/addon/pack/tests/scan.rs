//! 预检与决策通道（M160）：DTO ↔ core 的翻译、以及"缺省不覆盖"在 presenter 这一侧的表现。
//!
//! 这里**不重算**任何判定（判定在 core）：只钉两件事——
//!
//! 1. 三条入口（全部保留 / 全部覆盖 / 逐项）翻成 core 的 `ConflictDecisions` 之后语义不变，
//!    尤其"没点名的路径 = 保留"；
//! 2. 预检清单逐字段搬进 DTO（来源、成因、两侧体积与哈希、计数摘要）。

use std::path::PathBuf;

use super::harness::{temp_dir, write};
use crate::dto::{
    ConflictChoiceView, ConflictDecisionView, ConflictDecisionsView, ConflictReasonView,
    ConflictSourceView,
};
use unlucky_core::addon::import::{
    ConflictChoice, ConflictReason, FileSource, ImportConflict, ImportScan,
};

/// 造一份 core 侧的预检结果（只填这条用例要看的字段）。
fn scan_fixture() -> ImportScan {
    ImportScan {
        target: PathBuf::from("instances/survival/.minecraft"),
        format: Some("modrinth".to_string()),
        pack_name: Some("Better MC".to_string()),
        pack_version: Some("1.2.0".to_string()),
        game_version: Some("1.20.1".to_string()),
        loader: None,
        conflicts: vec![ImportConflict {
            path: "config/sodium-options.json".to_string(),
            source: FileSource::ClientOverrides,
            reason: ConflictReason::UserModified,
            disabled: false,
            pack_sha1: "aa".repeat(20),
            pack_size: 4_200,
            disk_sha1: "bb".repeat(20),
            disk_size: 5_100,
            base_sha1: Some("cc".repeat(20)),
        }],
        unknown_remote: 2,
        notes: Vec::new(),
        error: None,
    }
}

#[test]
fn scan_view_carries_every_field_and_the_counts() {
    let scan = scan_fixture();
    let view = super::super::convert::scan_view(&scan);

    assert_eq!(view.format, Some(crate::dto::PackFormatView::Modrinth));
    assert_eq!(view.pack_name.as_deref(), Some("Better MC"));
    assert_eq!(view.unknown_remote, 2, "判不了的远端条目要如实计数");
    assert!(view.needs_decision(), "有一条冲突就要问用户");
    let conflict = &view.conflicts[0];
    assert_eq!(conflict.path, "config/sodium-options.json");
    assert_eq!(
        conflict.source,
        ConflictSourceView::ClientOverrides,
        "叠层胜者那一侧的来源"
    );
    assert_eq!(conflict.reason, ConflictReasonView::UserModified);
    assert_eq!(conflict.pack_size, 4_200);
    assert_eq!(conflict.disk_size, 5_100, "目标现有体积要出现在清单里");
    assert_eq!(conflict.disk_sha1.len(), 40, "目标现有哈希也要有");
    assert_eq!(view.lost_bytes, 5_100, "覆盖会丢掉的字节");
    assert_eq!(view.incoming_bytes, 4_200);
}

#[test]
fn a_clean_scan_is_not_a_reason_to_bother_the_user() {
    let mut scan = scan_fixture();
    scan.conflicts.clear();
    let view = super::super::convert::scan_view(&scan);
    assert!(!view.needs_decision());
    assert!(view.is_clean(), "没有冲突也没有错误 ⇒ 可以直接导入");
}

#[test]
fn decisions_are_keep_all_by_default() {
    // 缺省 = 全部保留：页面不发决策、或发默认值，结果都必须是"一个文件都不动"。
    let view = ConflictDecisionsView::default();
    assert_eq!(view, ConflictDecisionsView::KeepAll);
    assert!(!view.overwrites_everything());
    let core = super::super::convert::decisions_of(&view);
    assert_eq!(core.choice_for("config/a.toml"), ConflictChoice::KeepUser);
    assert_eq!(core.default_choice(), ConflictChoice::KeepUser);
    assert!(core.is_empty());
}

#[test]
fn the_three_channels_map_one_to_one() {
    // 全部覆盖
    let all = super::super::convert::decisions_of(&ConflictDecisionsView::OverwriteAll);
    assert_eq!(all.choice_for("anything.toml"), ConflictChoice::Overwrite);
    assert_eq!(all.default_choice(), ConflictChoice::Overwrite);

    // 逐项：点名的照点名走，没点名的落到"保留"。
    let view = ConflictDecisionsView::per_item(
        ConflictChoiceView::KeepUser,
        vec![
            ConflictDecisionView {
                path: "mods/pin.jar".to_string(),
                choice: ConflictChoiceView::Overwrite,
            },
            ConflictDecisionView {
                path: "config/keep.toml".to_string(),
                choice: ConflictChoiceView::KeepUser,
            },
        ],
    );
    assert_eq!(view.overwrite_count(), 1);
    assert_eq!(view.keep_count(), 1);
    assert_eq!(view.len(), 2);
    assert!(!view.overwrites_everything(), "逐项不等于整批覆盖");

    let core = super::super::convert::decisions_of(&view);
    assert_eq!(core.choice_for("mods/pin.jar"), ConflictChoice::Overwrite);
    assert_eq!(
        core.choice_for("config/keep.toml"),
        ConflictChoice::KeepUser
    );
    assert_eq!(
        core.choice_for("mods/unlisted.jar"),
        ConflictChoice::KeepUser,
        "没点名的路径不许被逐项表顺手覆盖"
    );
    assert_eq!(core.overwrite_count(), 1);
}

#[test]
fn the_scan_runs_without_writing_and_says_nothing_when_there_is_no_conflict() {
    // 真跑一次后台预检：全新目标（什么都没有）⇒ 没有冲突，且**目标目录没被建出来**。
    let dir = temp_dir("scan-fresh");
    let source = dir.join("Better MC");
    write(
        &source.join("mmc-pack.json"),
        br#"{"formatVersion": 1, "components": [
            {"uid": "net.minecraft", "version": "1.20.1"}]}"#,
    );
    write(&source.join(".minecraft/mods/sodium.jar"), &[0x11; 64]);

    let data = dir.join("data");
    std::fs::create_dir_all(&data).expect("data root");
    let env = crate::host::HostEnv::at_root(&data);
    let (tx, rx) = crate::host::job::channel();
    let request = crate::dto::ImportRequestView {
        pack: source,
        instance: None,
        instance_name: Some("Scan Only".to_string()),
        decisions: ConflictDecisionsView::default(),
    };
    super::super::import::spawn_scan(
        env,
        tx,
        request,
        9,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );

    let mut scan = None;
    for task in rx.iter() {
        if let crate::host::job::HostTask::Pack { event, epoch } = task {
            assert_eq!(epoch, 9, "代次必须原样带回");
            if let crate::dto::PackEvent::Scan(view) = *event {
                scan = Some(*view);
            }
        }
    }
    let scan = scan.expect("必须回一条预检事件");
    assert!(scan.error.is_none(), "{:?}", scan.error);
    assert!(
        !scan.needs_decision(),
        "空目标上不该有冲突：{:?}",
        scan.conflicts
    );
    assert_eq!(scan.format, Some(crate::dto::PackFormatView::Multimc));
    assert!(
        !scan.target.exists(),
        "预检只读：目标目录不许被建出来（{}）",
        scan.target.display()
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// 事件序列的名字（断言"用户到底能不能看见"用）。
fn kind_of(event: &crate::dto::PackEvent) -> &'static str {
    match event {
        crate::dto::PackEvent::Progress(_) => "progress",
        crate::dto::PackEvent::Scan(scan) if scan.error.is_some() => "scan-with-error",
        crate::dto::PackEvent::Scan(_) => "scan",
        crate::dto::PackEvent::Report(report) if report.error.is_some() => "report-failed",
        crate::dto::PackEvent::Report(_) => "report",
        crate::dto::PackEvent::Probe(_) => "probe",
        crate::dto::PackEvent::Preview(_) => "preview",
        crate::dto::PackEvent::Exported(_) => "exported",
        crate::dto::PackEvent::ExportFailed(_) => "export-failed",
        crate::dto::PackEvent::Cancelled => "cancelled",
        crate::dto::PackEvent::Cleared => "cleared",
    }
}

/// 跑一次 `spawn_scan`，把事件按顺序收回来。
fn run_scan(source: std::path::PathBuf, data: &std::path::Path) -> Vec<crate::dto::PackEvent> {
    let env = crate::host::HostEnv::at_root(data);
    let (tx, rx) = crate::host::job::channel();
    let request = crate::dto::ImportRequestView {
        pack: source,
        instance: None,
        instance_name: Some("Scan Rework".to_string()),
        decisions: ConflictDecisionsView::default(),
    };
    super::super::import::spawn_scan(
        env,
        tx,
        request,
        3,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    let mut events = Vec::new();
    for task in rx.iter() {
        if let crate::host::job::HostTask::Pack { event, epoch } = task {
            assert_eq!(epoch, 3, "代次必须原样带回");
            events.push(*event);
        }
    }
    events
}

#[test]
fn a_failed_scan_still_reaches_the_screen_as_a_report() {
    // 复审 r1 P2-1 的回归钉：**这条用例验的是"用户看得见"，不是"字段被填了"**。
    //
    // 达标形状（旧行为 / M160 之前）：`Import` 失败 → `PackEvent::Report` → 结果块画「已回滚」+ 原因。
    // 出错形状（本 diff 引入的回归）：只发 `Scan(error)` ⇒ 页面 pick 为空、report 被清空 ⇒ 入口态，
    // 屏幕上什么都不发生。所以这里逐一断言**事件序列**，而不只看 `scan.error`。
    //
    // 可达面：detect 认得、read 才失败的包。这里用最普通的一种——MultiMC 的
    // `formatVersion: 2`（Prism 自己也会整份丢弃，不是刻意构造的畸形包）。
    let dir = temp_dir("scan-error");
    let source = dir.join("Prism 2");
    write(
        &source.join("mmc-pack.json"),
        br#"{"formatVersion": 2, "components": [
            {"uid": "net.minecraft", "version": "1.20.1"}]}"#,
    );
    write(&source.join(".minecraft/mods/sodium.jar"), &[0x11; 32]);
    let data = dir.join("data");
    std::fs::create_dir_all(&data).expect("data root");

    let events = run_scan(source, &data);
    let kinds: Vec<&str> = events.iter().map(kind_of).collect();
    assert_eq!(
        kinds,
        vec!["scan-with-error", "report-failed"],
        "预检自己失败时必须再发一条结果事件（否则页面静默空转）"
    );

    let crate::dto::PackEvent::Scan(scan) = &events[0] else {
        panic!("第一条必须是预检结果");
    };
    let error = scan.error.as_ref().expect("预检要带出错误");
    assert_eq!(
        error.code, "pack_metadata",
        "formatVersion != 1 是清单自身非法"
    );
    assert!(
        error.message.contains("formatVersion"),
        "原因要能读：{}",
        error.message
    );

    // 结果事件：三态是 Failed、原因与预检那份同源（code/label/message 都搬过来）。
    let crate::dto::PackEvent::Report(report) = &events[1] else {
        panic!("第二条必须是结果");
    };
    assert_eq!(report.status, crate::dto::ImportStatusView::Failed);
    let reported = report.error.as_ref().expect("报告要带出原因");
    assert_eq!(reported.code, error.code);
    assert_eq!(reported.message, error.message);
    assert!(!reported.label.is_empty(), "人读大类名也要有");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_clean_scan_does_not_emit_a_report() {
    // 对照：干净预检只发一条 Scan（多发 Report 会把页面从"清单"顶到"结果"）。
    let dir = temp_dir("scan-clean");
    let source = dir.join("Vanilla");
    write(
        &source.join("mmc-pack.json"),
        br#"{"formatVersion": 1, "components": [{"uid": "net.minecraft", "version": "1.20.1"}]}"#,
    );
    write(&source.join(".minecraft/mods/sodium.jar"), &[0x11; 32]);
    let data = dir.join("data");
    std::fs::create_dir_all(&data).expect("data root");

    let kinds: Vec<&str> = run_scan(source, &data).iter().map(kind_of).collect();
    assert_eq!(kinds, vec!["scan"], "干净预检只发一条");
    let _ = std::fs::remove_dir_all(&dir);
}
