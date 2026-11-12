//! 三个**写操作**的宿主实现（M247）：重命名 / 删除 / 备份导出。
//!
//! 从 `host/saves.rs` 拆出（那个文件加完在飞闸门与写失败映射之后越过 400 行硬线）。
//! 判据与读路径共用的部分留在父模块（[`super::target`] / [`super::run_dir`] / [`super::snapshot`] /
//! [`super::failure`]）——**派生运行目录只有一条路**，写路径不另算一份。
//!
//! | 函数 | 干什么 |
//! |---|---|
//! | [`spawn_rename`] / [`spawn_delete`] / [`spawn_backup`] | 起后台线程跑完再把**新快照 + 回执**送回去 |
//! | `rename` / `delete` / `backup` | 前台实现（测试直接调它们，不必绕线程） |
//! | [`export_dir`] | 备份的默认落点 `<data>/exports/saves/` |
//! | [`write_failure`] | core 的八类写失败 → DTO 的八类（一一对应，不折叠） |
//!
//! 三个 `spawn_*` 都**先跑 core 的写**再 `snapshot`：界面画的永远是磁盘上的真相，
//! 回执只说这件事动的是哪一条（见父模块的「三个写操作的回程形状」）。

use std::path::PathBuf;

use unlucky_core::game::saves::{self, SavesWriteError, SavesWriteKind};

use crate::dto::{
    InstanceId, SavesFailureKind, SavesFailureOp, SavesFailureView, SavesListView, SavesOped,
};

use super::super::env::HostEnv;
use super::super::job::{spawn, JobSender};
use super::{failure, snapshot, target, Report};

/// 后台重命名一个世界，**并把刷新后的快照与回执一起带回来**。
pub fn spawn_rename(env: HostEnv, tx: JobSender, id: InstanceId, dir: String, to: String) {
    spawn("saves-rename", move || {
        let report = Report {
            instance: id.clone(),
            list: rename(&env, &id, &dir, &to),
        };
        let _ = tx.send(crate::host::HostTask::Saves(Box::new(report)));
    });
}

/// 后台删除一个世界（**UI 已经二次确认过**；host 侧照样重新过边界校验）。
pub fn spawn_delete(env: HostEnv, tx: JobSender, id: InstanceId, dir: String) {
    spawn("saves-delete", move || {
        let report = Report {
            instance: id.clone(),
            list: delete(&env, &id, &dir),
        };
        let _ = tx.send(crate::host::HostTask::Saves(Box::new(report)));
    });
}

/// 后台把一个世界备份成 zip（落点见 [`export_dir`]），并带回新快照。
pub fn spawn_backup(
    env: HostEnv,
    tx: JobSender,
    id: InstanceId,
    dir: String,
    dest: Option<String>,
) {
    spawn("saves-backup", move || {
        let report = Report {
            instance: id.clone(),
            list: backup(&env, &id, &dir, dest.as_deref()),
        };
        let _ = tx.send(crate::host::HostTask::Saves(Box::new(report)));
    });
}

/// 重命名 → 再列一遍（回执里记「谁改成了什么」）。
pub(super) fn rename(
    env: &HostEnv,
    id: &str,
    from: &str,
    to: &str,
) -> Result<SavesListView, SavesFailureView> {
    let (instance, run_dir) = target(env, id)?;
    saves::rename_world(&run_dir, from, to)
        .map_err(|error| write_failure(&instance.id, SavesFailureOp::Rename, from, &error))?;
    snapshot(
        &instance,
        &run_dir,
        Some(SavesOped::Renamed {
            from: from.to_string(),
            to: to.to_string(),
        }),
    )
}

/// 删除 → 再列一遍（回执里记被删掉的那个名字）。
pub(super) fn delete(
    env: &HostEnv,
    id: &str,
    dir: &str,
) -> Result<SavesListView, SavesFailureView> {
    let (instance, run_dir) = target(env, id)?;
    saves::delete_world(&run_dir, dir)
        .map_err(|error| write_failure(&instance.id, SavesFailureOp::Delete, dir, &error))?;
    snapshot(
        &instance,
        &run_dir,
        Some(SavesOped::Deleted {
            dir: dir.to_string(),
        }),
    )
}

/// 备份 → 再列一遍（回执里记落点路径 / 字节数 / 被跳过的链接数）。
pub(super) fn backup(
    env: &HostEnv,
    id: &str,
    dir: &str,
    dest: Option<&str>,
) -> Result<SavesListView, SavesFailureView> {
    let (instance, run_dir) = target(env, id)?;
    let dest = dest.map_or_else(|| export_dir(env), PathBuf::from);
    let done = saves::backup_world(&run_dir, dir, &dest)
        .map_err(|error| write_failure(&instance.id, SavesFailureOp::Backup, dir, &error))?;
    let file = done
        .path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    snapshot(
        &instance,
        &run_dir,
        Some(SavesOped::BackedUp {
            dir: dir.to_string(),
            file,
            path: done.path.display().to_string(),
            bytes: done.bytes,
            skipped: done.skipped,
        }),
    )
}

/// 备份的默认落点：`<data>/exports/saves/`（见模块文档「备份落点」一节）。
fn export_dir(env: &HostEnv) -> PathBuf {
    env.layout.data().join("exports").join("saves")
}

/// core 的写失败 → 具名类别 + 「路径：原文」（**不翻译**，翻译在 app 层）。
///
/// 八个类别一一对应，没有一个落到「未知」兜底里：兜底会变成静默（用户只看到"失败了"）。
pub(super) fn write_failure(
    instance: &str,
    op: SavesFailureOp,
    file: &str,
    error: &SavesWriteError,
) -> SavesFailureView {
    let kind = match error.kind {
        SavesWriteKind::UnsafeName => SavesFailureKind::WriteUnsafeName,
        SavesWriteKind::SymbolicLink => SavesFailureKind::WriteSymlink,
        SavesWriteKind::NotFound => SavesFailureKind::WriteMissing,
        SavesWriteKind::NotADirectory => SavesFailureKind::WriteNotADirectory,
        SavesWriteKind::Conflict => SavesFailureKind::WriteConflict,
        SavesWriteKind::Boundary => SavesFailureKind::WriteBoundary,
        SavesWriteKind::TooLarge => SavesFailureKind::WriteTooLarge,
        SavesWriteKind::Io => SavesFailureKind::WriteFailed,
    };
    let mut failure = failure(
        instance,
        kind,
        format!("{}：{}", error.path.display(), error.detail),
    );
    failure.file = file.to_string();
    failure.op = Some(op);
    failure
}
