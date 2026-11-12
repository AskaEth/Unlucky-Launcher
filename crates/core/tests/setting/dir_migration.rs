//! 数据目录迁移：计划校验、逐分区搬动、收据 + 指针、失败逆序回滚。

use std::path::{Path, PathBuf};

use unlucky_core::platform::data_dir::{DataMode, ResolvedData};
use unlucky_core::platform::dir_migration::{self, DirMigrationOptions};
use unlucky_core::platform::error::PlatformError;
use unlucky_core::platform::layout::{LauncherLayout, Partition};
use unlucky_core::platform::receipt::{self, ReceiptKind};
use unlucky_core::platform::redirect;

use crate::common::TempDir;

fn resolved(home: &Path) -> ResolvedData {
    ResolvedData {
        mode: DataMode::Custom,
        launcher_home: home.to_path_buf(),
        game_home: home.join(".minecraft"),
        redirected_from: None,
    }
}

fn seed(home: &Path) {
    let layout = LauncherLayout::new(home.to_path_buf());
    layout.ensure().expect("建四分区");
    std::fs::create_dir_all(layout.auth_data()).expect("auth 目录");
    std::fs::create_dir_all(layout.sha1_cache("ab")).expect("缓存分片");
    std::fs::write(layout.launcher_config(), b"{\"x\":1}").expect("写配置");
    std::fs::write(layout.auth_data().join("token.json"), b"{}").expect("写凭据");
    std::fs::write(layout.sha1_cache("ab").join("hash"), b"payload").expect("写缓存");
    std::fs::write(layout.logs().join("launcher.log"), b"log").expect("写日志");
}

#[test]
fn plan_orders_partitions_and_skips_missing_ones() {
    let temp = TempDir::new("dir-migration-plan");
    seed(&temp.child("old/.ULL"));
    let from = resolved(&temp.child("old/.ULL"));
    let to = resolved(&temp.child("new/.ULL"));

    let plan = dir_migration::plan(&from, &to, &DirMigrationOptions::default()).expect("计划");
    let partitions: Vec<Partition> = plan.steps.iter().map(|step| step.partition).collect();
    assert_eq!(
        partitions,
        vec![
            Partition::Config,
            Partition::Data,
            Partition::Logs,
            Partition::Cache
        ],
        "配置最先、缓存最后"
    );

    let cache_off = DirMigrationOptions {
        migrate_cache: false,
    };
    let plan = dir_migration::plan(&from, &to, &cache_off).expect("计划");
    assert_eq!(plan.steps.len(), 3);
    assert!(plan
        .steps
        .iter()
        .all(|step| step.partition != Partition::Cache));
}

#[test]
fn execute_moves_everything_and_leaves_a_pointer() {
    let temp = TempDir::new("dir-migration-run");
    let old_home = temp.child("old/.ULL");
    let new_home = temp.child("new/.ULL");
    seed(&old_home);
    let from = resolved(&old_home);
    let to = resolved(&new_home);

    let plan = dir_migration::plan(&from, &to, &DirMigrationOptions::default()).expect("计划");
    let outcome = dir_migration::execute(&plan, 1_700_000_000_000).expect("执行");

    assert_eq!(
        outcome.moved,
        vec![
            Partition::Config,
            Partition::Data,
            Partition::Logs,
            Partition::Cache
        ]
    );

    let moved = LauncherLayout::new(new_home.clone());
    assert!(moved.launcher_config().is_file(), "配置要搬过去");
    assert!(moved.auth_data().join("token.json").is_file());
    assert!(moved.logs().join("launcher.log").is_file());
    assert!(moved.sha1_cache("ab").join("hash").is_file());
    assert!(!LauncherLayout::new(old_home.clone()).config().exists());

    let stored = receipt::read(&outcome.receipt_path)
        .expect("读收据")
        .expect("有收据");
    assert_eq!(stored.kind, ReceiptKind::DataDirMove);
    assert_eq!(stored.source, old_home);
    assert_eq!(stored.target, new_home);
    assert_eq!(stored.applied_at_ms, 1_700_000_000_000);

    let pointer = redirect::read(&old_home)
        .expect("读指针")
        .expect("旧目录要留指针");
    assert_eq!(pointer.launcher_home, new_home);
    assert_eq!(pointer.mode, DataMode::Custom);
    assert_eq!(outcome.pointer_path, old_home.join("redirect.json"));
}

#[test]
fn refuses_same_source_and_target() {
    let temp = TempDir::new("dir-migration-same");
    seed(&temp.child("only/.ULL"));
    let home = resolved(&temp.child("only/.ULL"));
    let err = dir_migration::plan(&home, &home, &DirMigrationOptions::default())
        .expect_err("源目标相同要拒绝");
    assert!(
        matches!(err, PlatformError::MigrationRefused { .. }),
        "{err}"
    );
}

#[test]
fn refuses_missing_source_and_non_empty_target() {
    let temp = TempDir::new("dir-migration-refuse");
    let from = resolved(&temp.child("gone/.ULL"));
    let to = resolved(&temp.child("new/.ULL"));
    assert!(matches!(
        dir_migration::plan(&from, &to, &DirMigrationOptions::default()).expect_err("源不存在"),
        PlatformError::MigrationRefused { .. }
    ));

    seed(&temp.child("old/.ULL"));
    let from = resolved(&temp.child("old/.ULL"));
    std::fs::create_dir_all(to.launcher_home.clone()).expect("建目标");
    std::fs::write(to.launcher_home.join("something"), b"x").expect("占位");
    let err = dir_migration::plan(&from, &to, &DirMigrationOptions::default())
        .expect_err("目标非空要拒绝");
    assert!(err.to_string().contains("已有数据"), "{err}");
}

#[test]
fn rolls_back_moved_partitions_when_a_later_step_fails() {
    let temp = TempDir::new("dir-migration-rollback");
    let old_home = temp.child("old/.ULL");
    let new_home = temp.child("new/.ULL");
    seed(&old_home);
    let from = resolved(&old_home);
    let to = resolved(&new_home);

    let plan = dir_migration::plan(&from, &to, &DirMigrationOptions::default()).expect("计划");
    // 计划之后在目标侧造障碍：data 位置先占成普通文件 → 搬 data 必失败。
    std::fs::create_dir_all(&new_home).expect("建目标根");
    std::fs::write(new_home.join("data"), b"obstruction").expect("造障碍");

    let err = dir_migration::execute(&plan, 42).expect_err("应当失败");
    assert!(
        matches!(err, PlatformError::MigrationRefused { .. }),
        "{err}"
    );

    let old = LauncherLayout::new(old_home);
    assert!(old.launcher_config().is_file(), "config 必须被回滚搬回来");
    assert!(old.auth_data().join("token.json").is_file());
    assert!(!old.redirect_path().exists(), "失败不该留指针");
    assert!(!new_home.join("config").exists(), "目标侧不该残留已搬分区");
    assert!(!new_home.join("data").is_dir(), "障碍文件保持原样");
}

#[test]
fn receipt_anti_replay_uses_source_target_signature() {
    let temp = TempDir::new("dir-migration-receipt");
    let receipt_path: PathBuf = temp.child("config/migration-receipt.json");
    let receipt = receipt::MigrationReceipt::data_dir_move(
        PathBuf::from("/old/.ULL"),
        PathBuf::from("/new/.ULL"),
        1_700_000_000_000,
    );
    receipt::write(&receipt_path, &receipt).expect("写收据");
    let loaded = receipt::read(&receipt_path).expect("读").expect("有");
    assert_eq!(
        receipt::decide(Some(&loaded), &receipt.source_fingerprint),
        receipt::ReplayDecision::SkipSameSource
    );
}
