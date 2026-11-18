//! 三个**写操作**与「游戏在跑」闸门的用例（M247）——从 `host/saves/tests.rs` 拆出
//! （那边加完 7 条之后越过 400 行硬线；判据一条不改）。
//!
//! | 用例 | 抓得住 |
//! |---|---|
//! | [`rename_round_trips_on_a_real_root`] / [`delete_round_trips_on_a_real_root`] | 真盘改动 + 快照刷新 + 回执对得上 |
//! | [`backup_round_trips_to_the_exports_dir`] | zip 落到 `<data>/exports/saves/`，回执带路径/字节/跳过数；显式落点也照用 |
//! | [`every_write_failure_keeps_its_own_kind`] | core 八类 → DTO 八类一一对应（不折叠），带世界名与操作 |
//! | [`write_commands_do_not_touch_the_disk_while_the_game_is_running`] | **在飞时一条都不落 core**：盘上指纹不变、世界还在、备份落点没建、回 `GameRunning` |
//! | [`write_commands_run_when_the_game_is_not_running`] | 闸门没有把写操作一并关死 |
//! | [`read_commands_are_not_blocked_while_the_game_is_running`] | 读路径与打开文件夹不受影响 |
//! | [`every_command_names_its_instance`] | 五条命令都答得上「哪个实例」（闸门靠它算在不在跑） |

use super::super::write::{backup, delete, rename, write_failure};
use super::super::*;
use super::{fingerprint, fixture, world, ID};
use crate::dto::{SavesCommand, SavesFailureOp, SavesFailureView, SavesOped};
use unlucky_core::game::saves::SavesWriteKind;

/// **端到端**：重命名 → 真盘改名 + 新快照 + 回执。
///
/// 抓得住：host 用错运行目录、写完成却忘了重新列（界面还画旧名字）、回执与快照对不上。
#[test]
fn rename_round_trips_on_a_real_root() {
    let (_root, env, run) = fixture("saves-rename");
    world(&run, "Old World");
    world(&run, "Other");

    let listed = rename(&env, ID, "Old World", "New World").expect("改名");
    assert!(!run.join("saves/Old World").exists(), "旧目录还在");
    assert!(
        run.join("saves/New World/level.dat").exists(),
        "新目录里没有内容"
    );
    let names: Vec<&str> = listed
        .worlds
        .iter()
        .map(|world| world.dir.as_str())
        .collect();
    assert_eq!(names, ["New World", "Other"], "快照没跟着刷新");
    assert_eq!(
        listed.last_op,
        Some(SavesOped::Renamed {
            from: "Old World".to_string(),
            to: "New World".to_string()
        })
    );
}

/// **端到端**：删除 → 真盘没了 + 新快照 + 回执。
#[test]
fn delete_round_trips_on_a_real_root() {
    let (_root, env, run) = fixture("saves-delete");
    world(&run, "Doomed");
    world(&run, "Kept");

    let listed = delete(&env, ID, "Doomed").expect("删除");
    assert!(!run.join("saves/Doomed").exists(), "目录还在");
    let names: Vec<&str> = listed
        .worlds
        .iter()
        .map(|world| world.dir.as_str())
        .collect();
    assert_eq!(names, ["Kept"], "别的世界被连带删了 / 快照没刷新");
    assert_eq!(
        listed.last_op,
        Some(SavesOped::Deleted {
            dir: "Doomed".to_string()
        })
    );
}

/// **端到端**：备份 → zip 落在启动器自己的 `exports/saves/`，回执带路径与字节数。
#[test]
fn backup_round_trips_to_the_exports_dir() {
    let (_root, env, run) = fixture("saves-backup");
    world(&run, "My World");

    let listed = backup(&env, ID, "My World", None).expect("备份");
    let exports = env.layout.data().join("exports").join("saves");
    let Some(SavesOped::BackedUp {
        dir,
        file,
        path,
        bytes,
        skipped,
    }) = listed.last_op.clone()
    else {
        panic!("备份成功却没有回执：{:?}", listed.last_op);
    };
    assert_eq!(dir, "My World");
    assert!(
        file.starts_with("My World-"),
        "文件名要带世界名与时戳：{file}"
    );
    assert!(file.ends_with(".zip"), "文件名要带 .zip：{file}");
    assert_eq!(std::path::PathBuf::from(&path), exports.join(&file));
    assert!(std::path::Path::new(&path).exists(), "zip 没真的落盘");
    assert!(bytes > 0);
    assert_eq!(skipped, 0);
    // 备份不改世界本身：列表还是一行。
    assert_eq!(listed.world_count(), 1);
    // 显式落点也要照用（将来接了目录选择框就走这条）。
    let custom = backup(
        &env,
        ID,
        "My World",
        Some(&run.join("elsewhere").display().to_string()),
    )
    .expect("备份到指定目录");
    assert!(matches!(custom.last_op, Some(SavesOped::BackedUp { .. })));
    assert!(run.join("elsewhere").is_dir(), "显式落点没被采用");
}

/// 八种写失败各给**自己的**类别（没有一个落到「未知」兜底）。
#[test]
fn every_write_failure_keeps_its_own_kind() {
    let (_root, env, run) = fixture("saves-write-kinds");
    world(&run, "Taken");
    world(&run, "Other");

    let cases = [
        ("Other", "Taken", SavesFailureKind::WriteConflict),
        ("Other", "..", SavesFailureKind::WriteUnsafeName),
        ("Nope", "Fresh", SavesFailureKind::WriteMissing),
    ];
    for (from, to, expected) in cases {
        let failure = rename(&env, ID, from, to).expect_err("改成功了");
        assert_eq!(failure.kind, expected, "{from} → {to}：{}", failure.detail);
    }

    let failure = delete(&env, ID, "Nope").expect_err("删成功了");
    assert_eq!(failure.kind, SavesFailureKind::WriteMissing);
    // 空 / 相对的落点目录：会落到进程当前目录，所以是「越界」而不是「IO 失败」。
    let failure = backup(&env, ID, "Taken", Some("")).expect_err("备份到空目录居然成功了");
    assert_eq!(
        failure.kind,
        SavesFailureKind::WriteBoundary,
        "{}",
        failure.detail
    );

    // core 的八个类别 → DTO 八个类别（一一对应，不折叠）。
    let pairs = [
        (
            SavesWriteKind::UnsafeName,
            SavesFailureKind::WriteUnsafeName,
        ),
        (SavesWriteKind::SymbolicLink, SavesFailureKind::WriteSymlink),
        (SavesWriteKind::NotFound, SavesFailureKind::WriteMissing),
        (
            SavesWriteKind::NotADirectory,
            SavesFailureKind::WriteNotADirectory,
        ),
        (SavesWriteKind::Conflict, SavesFailureKind::WriteConflict),
        (SavesWriteKind::Boundary, SavesFailureKind::WriteBoundary),
        (SavesWriteKind::TooLarge, SavesFailureKind::WriteTooLarge),
        (SavesWriteKind::Io, SavesFailureKind::WriteFailed),
    ];
    for (core, expected) in pairs {
        let error = unlucky_core::game::saves::SavesWriteError {
            path: PathBuf::from("C:/mc/saves/x"),
            kind: core,
            detail: "原文".to_string(),
        };
        let failure = write_failure(ID, SavesFailureOp::Backup, "Taken", &error);
        assert_eq!(failure.kind, expected, "{core:?}");
        assert!(
            failure.detail.contains("saves"),
            "原文丢了：{}",
            failure.detail
        );
        assert_eq!(failure.instance, ID);
        assert_eq!(failure.file, "Taken", "失败那句话要指名是哪一行");
        assert_eq!(failure.op, Some(SavesFailureOp::Backup));
    }
}

/// **在飞闸门**：三条写命令一条都不落 core —— 盘上指纹前后一致、世界还在、备份落点没被建，
/// 而且回的是 `GameRunning`（不是"写盘失败"）。
///
/// 抓得住：把闸门放回 UI（只灰按钮）——那时 `dispatch(..., running = true)` 会真的删掉世界，
/// 这条用例立刻红（指纹变了 / 世界不在 / `kind` 不是 `GameRunning`）。
#[test]
fn write_commands_do_not_touch_the_disk_while_the_game_is_running() {
    let (_root, env, run) = fixture("saves-running");
    world(&run, "Doomed");
    let before = fingerprint(&run);
    let (tx, rx) = super::super::super::job::channel();

    let commands = [
        SavesCommand::RenameWorld {
            id: ID.to_string(),
            dir: "Doomed".to_string(),
            to: "Renamed".to_string(),
        },
        SavesCommand::DeleteWorld {
            id: ID.to_string(),
            dir: "Doomed".to_string(),
        },
        SavesCommand::BackupWorld {
            id: ID.to_string(),
            dir: "Doomed".to_string(),
            dest: None,
        },
    ];
    for command in commands {
        let expected = match &command {
            SavesCommand::RenameWorld { .. } => SavesFailureOp::Rename,
            SavesCommand::DeleteWorld { .. } => SavesFailureOp::Delete,
            _ => SavesFailureOp::Backup,
        };
        dispatch(env.clone(), tx.clone(), command, true);
        let task = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("在飞时 host 没有回结果");
        let crate::host::HostTask::Saves(report) = task else {
            panic!("回程不是存档事件");
        };
        assert_eq!(report.instance, ID);
        let failure = report.list.expect_err("在飞时居然写成功了");
        assert_eq!(
            failure.kind,
            SavesFailureKind::GameRunning,
            "{}",
            failure.detail
        );
        assert_eq!(failure.file, "Doomed", "失败那句话要指名哪一行");
        assert_eq!(failure.op, Some(expected));
    }

    assert_eq!(fingerprint(&run), before, "在飞时写了盘");
    assert!(
        run.join("saves/Doomed/level.dat").exists(),
        "世界被删/被改名了"
    );
    assert!(
        !env.layout.data().join("exports").exists(),
        "在飞时备份落了盘"
    );
}

/// 反向：不在飞时同一条 `dispatch` 该真的干（闸门不是把写操作一并关死）。
#[test]
fn write_commands_run_when_the_game_is_not_running() {
    let (_root, env, run) = fixture("saves-idle-dispatch");
    world(&run, "Doomed");
    let (tx, rx) = super::super::super::job::channel();

    dispatch(
        env.clone(),
        tx.clone(),
        SavesCommand::DeleteWorld {
            id: ID.to_string(),
            dir: "Doomed".to_string(),
        },
        false,
    );
    let task = rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("host 没有回结果");
    let crate::host::HostTask::Saves(report) = task else {
        panic!("回程不是存档事件");
    };
    let listed = report.list.expect("不在飞时该照常删");
    assert!(!run.join("saves/Doomed").exists(), "闸门把写操作一并关死了");
    assert!(matches!(listed.last_op, Some(SavesOped::Deleted { .. })));
}

/// 读路径**不受**在飞闸门影响（它不改盘）。
#[test]
fn read_commands_are_not_blocked_while_the_game_is_running() {
    let (_root, env, run) = fixture("saves-running-read");
    world(&run, "Kept");
    let (tx, rx) = super::super::super::job::channel();

    dispatch(
        env.clone(),
        tx.clone(),
        SavesCommand::List(ID.to_string()),
        true,
    );
    let task = rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("host 没有回结果");
    let crate::host::HostTask::Saves(report) = task else {
        panic!("回程不是存档事件");
    };
    let listed = report.list.expect("在飞时读列表不该被拒");
    assert_eq!(listed.world_count(), 1);
    assert!(listed.last_op.is_none());
}

/// 每一层都认得同一个实例 id（闸门用它算「在不在跑」——漏一条分支就等于那条命令没有闸门）。
#[test]
fn every_command_names_its_instance() {
    let commands = [
        SavesCommand::List(ID.to_string()),
        SavesCommand::OpenFolder { id: ID.to_string() },
        SavesCommand::RenameWorld {
            id: ID.to_string(),
            dir: "A".to_string(),
            to: "B".to_string(),
        },
        SavesCommand::DeleteWorld {
            id: ID.to_string(),
            dir: "A".to_string(),
        },
        SavesCommand::BackupWorld {
            id: ID.to_string(),
            dir: "A".to_string(),
            dest: None,
        },
    ];
    assert_eq!(commands.len(), 5, "新增了命令就要在这里补一条");
    for command in &commands {
        assert_eq!(instance_of(command), ID, "{command:?}");
    }
}

/// 找不到实例的写操作 ⇒ 同一个具名失败（不是"写成功了个空的"）。
#[test]
fn write_ops_on_a_vanished_instance_are_named() {
    let (_root, env, _run) = fixture("saves-write-unknown");
    for failure in [
        rename(&env, "gone", "A", "B").expect_err("改名成功了"),
        delete(&env, "gone", "A").expect_err("删除成功了"),
        backup(&env, "gone", "A", None).expect_err("备份成功了"),
    ] {
        assert_eq!(failure.kind, SavesFailureKind::InstanceUnavailable);
        assert_eq!(failure.instance, "gone");
    }
    // 这个用例只借 `SavesFailureView` 的类型做一次构造检查：字段齐了才算「结构化失败」。
    let _probe = SavesFailureView {
        instance: "gone".to_string(),
        file: String::new(),
        op: None,
        kind: SavesFailureKind::InstanceUnavailable,
        detail: String::new(),
    };
}
