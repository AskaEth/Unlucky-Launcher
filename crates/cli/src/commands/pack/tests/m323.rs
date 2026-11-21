//! M323：人读输出里**不许出现 core 的内部 key**（`pack.import.…`）。
//!
//! M322 复审核实的那条（判 low、已开 finding）就是本文件要钉死的：`pack.rs` 把
//! `message_key` 原样印进括号（`（pack.import.missing.fetch_failed：…）`），冲突行、
//! 未支持 component 行、`lines.rs` 的告警行同病。事实（路径 / 引擎原话）当时都在，
//! 多出来的是一个用户读不懂、GUI 也不会显示的内部标识。
//!
//! 两条判据，缺一不可：
//!
//! 1. **不许有内部前缀**：整份人读输出里一个 `pack.import.` 都不许有；
//! 2. **事实不能丢**：同一条缺失的路径与引擎原话 `detail` 改完必须**逐字**还在，冲突的成因与
//!    处置、被拒 component 的 uid/版本同样。
//!
//! 变异判据：任一处改回直接印 `message_key` ⇒ ①红；取文案改成空串 / 占位 ⇒ ②红。
//! 稳定 key 没有消失，它在 `--json`（`warnings[].message_key` / `missing[].message_key`）里
//! 给脚本查表用——本文件顺手钉一句，免得下一个人"清理噪音"时把机器通道也删了。

use super::super::outcome;
use super::{ctx_at, TempDir};

use std::path::PathBuf;

use unlucky_core::addon::import::{
    keys, ConflictChoice, ConflictFile, ConflictReason, ImportIssue, ImportReport, MissingFile,
    MissingReason, RejectedComponent, MISSING_OFFLINE_DETAIL,
};

use crate::cli::PackImportArgs;

/// 引擎原话（真实形状：`FetchError::message()`，`phase.rs` 把它原样写进 `detail`）。
const DETAIL_404: &str = "HTTP 404 Not Found（https://cdn.example.com/mods/ae2.jar）";
/// 离线那一档的 `detail`：**取 core 那句常量**，不在这里另抄一份（M325 把「离线」从那句里去掉时，
/// 抄一份的后果就是这条用例悄悄变成「钉住一句 core 已经不说的话」）。
const DETAIL_OFFLINE: &str = MISSING_OFFLINE_DETAIL;
/// 补不出地址那一档的 `detail`（core 的 `missing_for(true)`）。
const DETAIL_NO_SOURCE: &str = "清单里没有可用下载地址，或 CurseForge 元数据补不出来（不猜 URL）";

fn args() -> PackImportArgs {
    PackImportArgs {
        path: PathBuf::from("pack.mrpack"),
        target: None,
        name: None,
        overwrite: false,
        no_register: false,
    }
}

/// 一条三档齐全的「搞不到」：真下过但失败 / 离线 / 补不出地址。
fn missing(display: &str, reason: MissingReason, detail: &str) -> MissingFile {
    MissingFile {
        path: Some(display.to_string()),
        display: display.to_string(),
        project_id: None,
        file_id: None,
        reason,
        message_key: reason.message_key(),
        detail: detail.to_string(),
    }
}

/// 一份**离线**报告：不需要网络、不需要真包，全部是报告条目（M323 的判据只看渲染）。
fn report() -> ImportReport {
    let mut report = ImportReport::empty(PathBuf::from("C:/tmp/instance"));
    report.missing = vec![
        missing("mods/ae2.jar", MissingReason::FetchFailed, DETAIL_404),
        missing("mods/offline.jar", MissingReason::Offline, DETAIL_OFFLINE),
        missing("curse:1234:5678", MissingReason::NoSource, DETAIL_NO_SOURCE),
    ];
    // 冲突：成因 `reason` 在 CLI 侧唯一的人话表达就是那个 key 对应的文案，
    // `ConflictFile` 没有 `detail` 字段——删 key 而不取文案就是丢事实。
    report.conflicts.push(ConflictFile {
        path: "config/a.toml".to_string(),
        reason: ConflictReason::UserModified,
        choice: ConflictChoice::KeepUser,
        message_key: ConflictReason::UserModified.message_key(),
        pack_sha1: "a".repeat(40),
        pack_size: 12,
        base_sha1: None,
        disk_sha1: None,
    });
    report.rejected_components.push(RejectedComponent {
        uid: "org.lwjgl3".to_string(),
        version: Some("3.3.1".to_string()),
        message_key: keys::REJECTED_COMPONENT,
    });
    report.warnings.push(ImportIssue::warning(
        keys::NOTE_LOADER_WARNING,
        "loader 库缺下载地址，已降级",
    ));
    report
}

fn human_lines() -> Vec<String> {
    let dir = TempDir::new("m323-keys");
    let ctx = ctx_at(dir.path());
    let outcome = outcome(
        &ctx,
        &report(),
        &PathBuf::from("C:/tmp/instance"),
        &[],
        &args(),
        None,
    );
    outcome.report.human
}

/// ① 整份人读输出里**一个** `pack.import.` 都没有。
///
/// 变异：`missing_lines` / `conflict_lines` / `rejected_lines` / `warnings_lines` 任一处
/// 改回 `{message_key}` ⇒ 红（断言里会指出是哪一行）。
#[test]
fn the_human_output_never_prints_core_message_keys() {
    let human = human_lines();
    for line in &human {
        assert!(
            !line.contains("pack.import."),
            "人读输出里泄漏了内部 key：{line}\n整份：{human:#?}"
        );
    }
}

/// ② 事实逐字还在：三档缺失的 `detail` 原话 + 路径、冲突的路径/成因/处置、
/// 被拒 component 的 uid/版本、告警原文。
///
/// 变异：把某一档的 `detail` 从渲染里丢掉（例如只印成因短语）⇒ 红；把成因渲染成空串 ⇒ 红。
#[test]
fn every_fact_survives_the_key_removal_verbatim() {
    let human = human_lines();
    let has = |needle: &str| human.iter().any(|line| line.contains(needle));

    // 三档缺失：路径 + 引擎/管线原话，逐字。
    for (display, detail) in [
        ("mods/ae2.jar", DETAIL_404),
        ("mods/offline.jar", DETAIL_OFFLINE),
        ("curse:1234:5678", DETAIL_NO_SOURCE),
    ] {
        assert!(
            human
                .iter()
                .any(|line| line.contains(display) && line.contains(detail)),
            "{display} 那一行丢了事实（路径或 detail 原话）：{human:#?}"
        );
    }
    // 冲突：路径 + 成因（目录里那句）+ 处置（目录里那句）。
    assert!(
        human
            .iter()
            .any(|line| line.contains("config/a.toml") && line.contains("你改过")),
        "冲突行丢了成因：{human:#?}"
    );
    assert!(
        human
            .iter()
            .any(|line| line.contains("config/a.toml") && line.contains("保留我的")),
        "冲突行丢了处置：{human:#?}"
    );
    // 未支持 component：uid + 版本仍在，括号里换成目录那句「客户端不支持」。
    assert!(
        human
            .iter()
            .any(|line| line.contains("org.lwjgl3") && line.contains("3.3.1")),
        "被拒 component 的 uid/版本丢了：{human:#?}"
    );
    assert!(
        has("客户端不支持"),
        "被拒 component 的成因没取文案：{human:#?}"
    );
    // 告警：原文照旧。
    assert!(has("loader 库缺下载地址，已降级"), "{human:#?}");
}

/// 机器通道不受影响：稳定 key 仍在 `--json` 里（脚本按它查表）。
///
/// 变异：为了「清理噪音」把 `message_key` 从报告里删掉 / 不序列化 ⇒ 红。
#[test]
fn the_stable_keys_stay_in_the_json_channel() {
    let dir = TempDir::new("m323-json");
    let ctx = ctx_at(dir.path());
    let outcome = outcome(
        &ctx,
        &report(),
        &PathBuf::from("C:/tmp/instance"),
        &[],
        &args(),
        None,
    );
    let data = &outcome.report.data;
    assert_eq!(
        data["missing"][0]["message_key"],
        keys::MISSING_FETCH_FAILED,
        "{data}"
    );
    assert_eq!(
        data["conflicts"][0]["message_key"],
        keys::CONFLICT_USER_MODIFIED,
        "{data}"
    );
    assert_eq!(
        data["warnings"][0]["message_key"],
        keys::NOTE_LOADER_WARNING,
        "{data}"
    );
}
