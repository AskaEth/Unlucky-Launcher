//! 导出（M113）：**预检不写盘**，写盘是另一次调用，两次用同一套判据。
//!
//! 预检的全部依据就是 core 的 `scan_instance` + 「没有远程目录 ⇒ 一律内联」：
//! `RemoteCatalog` 目前只有 `OfflineCatalog`，任何文件都证明不了可被远程替代。所以
//! 「将内联 N / 无法导出 M」里的 N 是扫到的文件数、M 是 `SkippedLocal`（符号链接 / 路径不安全 /
//! 读不出来）。写盘走 `export_instance`（同样注入离线目录），
//! `tests::preview_counts_match_the_written_report` 用真管线钉住两边相等。

use unlucky_core::addon::export::{
    export_instance, scan_instance, ExportRequest, OfflineCatalog, DEFAULT_CONTENT_ROOTS,
};

use crate::dto::{
    ExportClassView, ExportFileView, ExportPreviewView, ExportReasonView, ExportRequestView,
    PackEvent,
};

use super::super::super::env::HostEnv;
use super::super::super::job::{spawn, JobSender};
use super::convert::{class_view, done_view, reason_view};
use super::send;

/// 起一个后台预检线程（**不写盘**）。
pub(super) fn spawn_preview(env: HostEnv, tx: JobSender, request: ExportRequestView, epoch: u64) {
    spawn("pack-export-preview", move || {
        let root = match super::instance_root(&env, &request.instance) {
            Ok(root) => root,
            Err(reason) => {
                send(&tx, epoch, PackEvent::ExportFailed(reason));
                return;
            }
        };
        send(
            &tx,
            epoch,
            PackEvent::Preview(Box::new(preview_of(&root, &env, &request))),
        );
    });
}

/// 预检的**纯函数部分**（不起线程、不写盘）：扫描 → 三分类计数。
///
/// 抽出来是为了能测：`tests::preview_counts_match_the_written_report` 用它和
/// `export_instance` 的真实报告对数字，等式不成立就红。
pub(super) fn preview_of(
    root: &std::path::Path,
    env: &HostEnv,
    request: &ExportRequestView,
) -> ExportPreviewView {
    let target = request
        .target
        .clone()
        .unwrap_or_else(|| super::default_export_path(env, request));
    let roots: Vec<String> = DEFAULT_CONTENT_ROOTS
        .iter()
        .map(|entry| (*entry).to_string())
        .collect();
    let (files, skipped) = scan_instance(root, &roots);

    let mut rows: Vec<ExportFileView> = files
        .iter()
        .map(|file| ExportFileView {
            path: file.path.clone(),
            class: ExportClassView::Inline,
            reason: ExportReasonView::NoRemoteMatch,
            bytes: file.bytes,
            disabled: file.disabled,
        })
        .collect();
    rows.extend(skipped.iter().map(|file| ExportFileView {
        path: file.path.clone(),
        class: class_view(file.reason.class()),
        reason: reason_view(file.reason),
        bytes: 0,
        disabled: false,
    }));
    // 与 `ExportReport::files` 同一条顺序（按路径）
    rows.sort_by(|left, right| left.path.cmp(&right.path));

    ExportPreviewView {
        format: request.format,
        instance: request.instance.clone(),
        name: request.name.clone(),
        version: request.version.clone(),
        target,
        files: rows,
        notes: vec![
            "No remote catalogue yet: every file has to be inlined into overrides/".to_string(),
            "Unreadable files, symlinks and unsafe paths are listed below, one by one".to_string(),
        ],
    }
}

/// 起一个后台写盘线程。
pub(super) fn spawn_write(env: HostEnv, tx: JobSender, request: ExportRequestView, epoch: u64) {
    spawn("pack-export", move || {
        let root = match super::instance_root(&env, &request.instance) {
            Ok(root) => root,
            Err(reason) => {
                send(&tx, epoch, PackEvent::ExportFailed(reason));
                return;
            }
        };
        let format = match super::format_of(request.format) {
            Ok(format) => format,
            Err(reason) => {
                send(&tx, epoch, PackEvent::ExportFailed(reason));
                return;
            }
        };
        let target = request
            .target
            .clone()
            .unwrap_or_else(|| super::default_export_path(&env, &request));
        let snapshot = super::snapshot_of(&env, &request.instance);
        let mut core_request = ExportRequest::new(&root, &target, format)
            .with_name(request.name.clone())
            .with_version(request.version.clone())
            .with_catalog(&OfflineCatalog);
        if let Some(snapshot) = &snapshot {
            core_request = core_request.with_snapshot(snapshot);
        }
        match export_instance(&core_request) {
            Ok(report) => send(
                &tx,
                epoch,
                PackEvent::Exported(Box::new(done_view(&report))),
            ),
            Err(error) => send(&tx, epoch, PackEvent::ExportFailed(format!("{error}"))),
        }
    });
}
