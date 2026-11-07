//! `core::game::java::location`：落点三模式（Portable / Standard / Custom）+ 孤儿目录清理。
//!
//! 三模式的口径来自 D23/D6：Portable 保持 v1 布局、Standard 走 OS 标准目录的
//! `<发行方>/<版本>` 分层、Custom 由 `java.installRoot` 覆盖；清理只碰我们自己的形状。

use std::path::PathBuf;

use unlucky_core::game::java::location::{
    cleanup_orphans, sanitize_version, standard_java_root, JavaInstallRoot, LocationMode,
};
use unlucky_core::game::java::JdkVendor;
use unlucky_core::platform::layout::LauncherLayout;
use unlucky_core::platform::paths::{MapEnv, Platform};

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "ull-java-loc-{tag}-{}-{nanos}-{seq}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("建临时目录");
    dir
}

#[test]
fn standard_root_follows_each_platform_convention() {
    let env = MapEnv::new()
        .with("APPDATA", "C:/Users/u/AppData/Roaming")
        .with("HOME", "/home/u")
        .with("USERPROFILE", "C:/Users/u");
    // 期望值一律用 `join` 拼（Windows 上的分隔符与 `Path` 比较细节交给标准库）。
    let unix_home = PathBuf::from("/home/u");
    assert_eq!(
        standard_java_root(Platform::Windows, &env),
        Some(PathBuf::from("C:/Users/u/AppData/Roaming").join("Java")),
        "Windows 用户原话：%APPDATA%\\Java"
    );
    assert_eq!(
        standard_java_root(Platform::MacOs, &env),
        Some(unix_home.join("Library/Application Support").join("Java"))
    );
    assert_eq!(
        standard_java_root(Platform::Linux, &env),
        Some(unix_home.join(".local/share").join("java")),
        "Linux 走 XDG 约定（无 XDG_DATA_HOME 时回落 ~/.local/share）"
    );
    // `Path::is_absolute()` 的口径是编译目标决定的：Windows 上 `C:/data/u` 才是绝对路径，
    // 而 `/data/u` 只是「rooted」。夹具按目标平台给，才能同时验到「绝对 → 采用」。
    let xdg_root = if cfg!(windows) {
        "C:/data/u"
    } else {
        "/data/u"
    };
    let xdg = MapEnv::new()
        .with("HOME", "/home/u")
        .with("XDG_DATA_HOME", xdg_root);
    assert_eq!(
        standard_java_root(Platform::Linux, &xdg),
        Some(PathBuf::from(xdg_root).join("java"))
    );
    // 相对路径的 XDG_DATA_HOME 按规范忽略 → 回落 HOME。
    let relative = MapEnv::new()
        .with("HOME", "/home/u")
        .with("XDG_DATA_HOME", "data/u");
    assert_eq!(
        standard_java_root(Platform::Linux, &relative),
        Some(unix_home.join(".local/share").join("java"))
    );
    assert_eq!(standard_java_root(Platform::Linux, &MapEnv::new()), None);
}

#[test]
fn portable_and_custom_modes_derive_the_documented_directories() {
    let launcher = LauncherLayout::new("/data/.ULL");
    let portable = JavaInstallRoot::portable(&launcher);
    assert_eq!(portable.mode, LocationMode::Portable);
    assert_eq!(
        portable.jdk_dir(21, JdkVendor::Temurin, "21.0.13+9"),
        PathBuf::from("/data/.ULL/data/java-managed/jdk-21-temurin"),
        "便携模式必须保持 v1 布局（M33/M35 兼容）"
    );
    let custom = JavaInstallRoot::custom("/opt/java");
    assert_eq!(custom.mode, LocationMode::Custom);
    assert_eq!(
        custom.jdk_dir(21, JdkVendor::Oracle, "21.0.2"),
        PathBuf::from("/opt/java/oracle/21.0.2"),
        "Custom 用 <vendor>/<version>（D23）"
    );
    assert_eq!(custom.cache_dir(), PathBuf::from("/opt/java/.cache"));
    assert!(custom.describe().contains("custom"));
}

#[test]
fn version_strings_are_sanitized_for_directory_names() {
    assert_eq!(sanitize_version("21.0.13+9"), "21.0.13+9");
    assert_eq!(sanitize_version("21.0.13.0.2"), "21.0.13.0.2");
    assert_eq!(sanitize_version("21.0.13+3"), "21.0.13+3");
    assert_eq!(sanitize_version(" 21.0.2 "), "21.0.2");
    assert_eq!(sanitize_version("../evil"), ".._evil");
    assert_eq!(sanitize_version(""), "unknown");
}

#[test]
fn cleanup_removes_staging_and_empty_shells_but_never_foreign_dirs() {
    let root = temp_dir("cleanup");
    // 半成品：解压中断的暂存目录。
    std::fs::create_dir_all(root.join(".staging-jdk-21-temurin")).expect("建暂存");
    std::fs::write(root.join(".staging-jdk-21-temurin/bin"), b"x").expect("写暂存文件");
    // 空壳 vendor 目录（Standard 布局的残留）。
    std::fs::create_dir_all(root.join("oracle")).expect("建 vendor");
    // 用户自己的目录（非空、无戳记）：绝不能删。
    std::fs::create_dir_all(root.join("my-own-jdk")).expect("建用户目录");
    std::fs::write(root.join("my-own-jdk/notes.txt"), b"mine").expect("写用户文件");
    // 归档缓存目录：保留。
    std::fs::create_dir_all(root.join(".cache")).expect("建缓存");

    let report = cleanup_orphans(&root);
    assert!(
        !root.join(".staging-jdk-21-temurin").exists(),
        "暂存目录应删"
    );
    assert!(!root.join("oracle").exists(), "空 vendor 目录应删");
    assert!(
        root.join("my-own-jdk/notes.txt").is_file(),
        "用户目录必须还在"
    );
    assert!(root.join(".cache").is_dir(), "缓存目录不能删");
    assert_eq!(report.removed.len(), 2, "{:?}", report.removed);
    let _ = std::fs::remove_dir_all(&root);
}
