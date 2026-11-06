//! `redirect.json`：写/读/跟随/成环/损坏/版本更高。

use std::path::PathBuf;

use unlucky_core::platform::data_dir::DataMode;
use unlucky_core::platform::error::PlatformError;
use unlucky_core::platform::redirect::{
    follow, read, redirect_path, remove, write, RedirectPointer, REDIRECT_SCHEMA_VERSION,
};

use crate::common::TempDir;

fn pointer(target: &str) -> RedirectPointer {
    RedirectPointer {
        schema_version: REDIRECT_SCHEMA_VERSION,
        mode: DataMode::Standard,
        launcher_home: PathBuf::from(target),
        game_home: PathBuf::from(target).join(".minecraft"),
        migrated_at_ms: 1_700_000_000_000,
    }
}

#[test]
fn round_trips_pointer_file() {
    let temp = TempDir::new("redirect");
    let origin = temp.child(".ULL");
    assert_eq!(read(&origin).expect("读空目录"), None);

    let written = write(&origin, &pointer("/elsewhere/.ULL")).expect("写指针");
    assert_eq!(written, redirect_path(&origin));
    assert!(origin.join("redirect.json").is_file());

    let loaded = read(&origin).expect("读指针").expect("有指针");
    assert_eq!(loaded, pointer("/elsewhere/.ULL"));
    assert!(remove(&origin).expect("删指针"));
    assert!(!remove(&origin).expect("再删一次"));
}

#[test]
fn follows_chain_to_final_target() {
    let temp = TempDir::new("redirect-chain");
    let first = temp.child("a");
    let second = temp.child("b");
    let third = temp.child("c");

    write(&first, &pointer(&second.to_string_lossy())).expect("写 a");
    write(&second, &pointer(&third.to_string_lossy())).expect("写 b");

    let followed = follow(&first, 8).expect("跟随").expect("有指针");
    assert_eq!(followed.hops, 2);
    assert_eq!(followed.pointer.launcher_home, third);
}

#[test]
fn detects_redirect_cycle() {
    let temp = TempDir::new("redirect-cycle");
    let first = temp.child("a");
    let second = temp.child("b");
    write(&first, &pointer(&second.to_string_lossy())).expect("写 a");
    write(&second, &pointer(&first.to_string_lossy())).expect("写 b");

    let err = follow(&first, 8).expect_err("应当成环报错");
    assert!(matches!(err, PlatformError::RedirectCycle { .. }), "{err}");
}

#[test]
fn rejects_corrupt_and_newer_pointer() {
    let temp = TempDir::new("redirect-corrupt");
    let dir = temp.child(".ULL");
    std::fs::create_dir_all(&dir).expect("建目录");
    std::fs::write(dir.join("redirect.json"), b"{not json").expect("写坏文件");
    assert!(matches!(
        read(&dir).expect_err("坏文件应报错"),
        PlatformError::RedirectCorrupt { .. }
    ));

    let mut newer = pointer("/newer/.ULL");
    newer.schema_version = REDIRECT_SCHEMA_VERSION + 1;
    std::fs::write(
        dir.join("redirect.json"),
        serde_json::to_vec(&newer).expect("序列化"),
    )
    .expect("写文件");
    assert!(matches!(
        read(&dir).expect_err("新版本指针应报错"),
        PlatformError::RedirectCorrupt { .. }
    ));
}
