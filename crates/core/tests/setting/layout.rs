//! 四分区布局、路径拼接、实例 id 白名单、可搬运路径字符串。

use std::path::PathBuf;

use unlucky_core::platform::layout::{
    is_valid_instance_id, LauncherLayout, Partition, CONFIG_DIR, GAME_HOME_DIR, INSTANCES_DIR,
    LAUNCHER_CONFIG_FILE, LAUNCHER_HOME_DIR, MIGRATION_RECEIPT_FILE, PROFILES_CONFIG_FILE,
    REDIRECT_FILE,
};

use crate::common::TempDir;

#[test]
fn home_dir_names_match_design() {
    assert_eq!(LAUNCHER_HOME_DIR, ".ULL");
    assert_eq!(GAME_HOME_DIR, ".minecraft");
    assert_eq!(CONFIG_DIR, "config");
    assert_eq!(INSTANCES_DIR, "instances");
    assert_eq!(LAUNCHER_CONFIG_FILE, "launcher.json");
    assert_eq!(PROFILES_CONFIG_FILE, "profiles.json");
    assert_eq!(REDIRECT_FILE, "redirect.json");
    assert_eq!(MIGRATION_RECEIPT_FILE, "migration-receipt.json");
}

#[test]
fn exposes_four_partitions() {
    let layout = LauncherLayout::new("/home/u/.ULL");
    assert_eq!(layout.home(), std::path::Path::new("/home/u/.ULL"));
    assert_eq!(layout.config(), PathBuf::from("/home/u/.ULL/config"));
    assert_eq!(layout.data(), PathBuf::from("/home/u/.ULL/data"));
    assert_eq!(layout.cache(), PathBuf::from("/home/u/.ULL/cache"));
    assert_eq!(layout.logs(), PathBuf::from("/home/u/.ULL/logs"));

    assert_eq!(Partition::ALL.len(), 4);
    assert_eq!(Partition::Config.dir_name(), "config");
    assert!(Partition::Cache.is_disposable());
    assert!(!Partition::Config.is_disposable());
}

#[test]
fn builds_config_and_cache_paths() {
    let layout = LauncherLayout::new("/home/u/.ULL");
    assert_eq!(
        layout.launcher_config(),
        PathBuf::from("/home/u/.ULL/config/launcher.json")
    );
    assert_eq!(
        layout.profiles_config(),
        PathBuf::from("/home/u/.ULL/config/profiles.json")
    );
    assert_eq!(
        layout.sha1_cache("ab"),
        PathBuf::from("/home/u/.ULL/cache/sha1/ab")
    );
    assert_eq!(
        layout.etag_cache(),
        PathBuf::from("/home/u/.ULL/cache/etag.json")
    );
    assert_eq!(
        layout.meta_cache(),
        PathBuf::from("/home/u/.ULL/cache/meta")
    );
    assert_eq!(
        layout.java_managed(),
        PathBuf::from("/home/u/.ULL/data/java-managed")
    );
    assert_eq!(layout.auth_data(), PathBuf::from("/home/u/.ULL/data/auth"));
    assert_eq!(
        layout.agent_sessions(),
        PathBuf::from("/home/u/.ULL/data/agent/sessions")
    );
    assert_eq!(
        layout.redirect_path(),
        PathBuf::from("/home/u/.ULL/redirect.json")
    );
    assert_eq!(
        layout.migration_receipt(),
        PathBuf::from("/home/u/.ULL/config/migration-receipt.json")
    );
}

#[test]
fn instance_config_rejects_path_traversal() {
    let layout = LauncherLayout::new("/home/u/.ULL");
    let id = "6f9619ff-8b86-d011-b42d-00c04fc964ff";
    assert!(is_valid_instance_id(id));
    assert_eq!(
        layout.instance_config(id).expect("合法 id"),
        PathBuf::from(format!("/home/u/.ULL/config/instances/{id}.json"))
    );
    assert!(layout.instance_config("../escape").is_err());
    assert!(layout.instance_config("").is_err());
    assert!(layout.instance_config("a/b").is_err());
}

#[test]
fn ensure_creates_all_partitions() {
    let temp = TempDir::new("layout");
    let layout = LauncherLayout::new(temp.child(".ULL"));
    layout.ensure().expect("建目录");
    for partition in Partition::ALL {
        assert!(
            layout.partition(partition).is_dir(),
            "{}",
            partition.as_str()
        );
    }
}

#[test]
fn portable_path_strings_keep_absolute_and_normalise_relative() {
    use unlucky_core::platform::paths::{from_portable_string, to_portable_string};

    assert_eq!(
        to_portable_string(std::path::Path::new(".minecraft")),
        ".minecraft"
    );
    assert_eq!(
        to_portable_string(std::path::Path::new("versions\\1.20.1")),
        "versions/1.20.1"
    );
    let absolute = std::env::temp_dir();
    assert_eq!(to_portable_string(&absolute), absolute.to_string_lossy());
    assert_eq!(
        from_portable_string("versions/1.20.1"),
        PathBuf::from("versions/1.20.1")
    );
}
