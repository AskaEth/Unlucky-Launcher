//! 三模式数据目录解析：Portable / Standard / Custom + 平台落点 + 高级覆盖 + redirect 跟随。

use std::path::PathBuf;

use unlucky_core::platform::data_dir::{DataDirRequest, DataMode, ResolvedData, Resolver};
use unlucky_core::platform::error::PlatformError;
use unlucky_core::platform::paths::{standard_game_home, standard_launcher_home, MapEnv, Platform};
use unlucky_core::platform::redirect::{self, RedirectPointer, REDIRECT_SCHEMA_VERSION};

use crate::common::TempDir;

#[test]
fn portable_puts_dot_ull_and_minecraft_beside_exe() {
    let env = MapEnv::new().with("APPDATA", "/roaming");
    let resolver = Resolver::with_env(Platform::Windows, Some(PathBuf::from("/games/ULL")), &env);
    assert_eq!(resolver.default_mode(), DataMode::Portable);

    let resolved = resolver.resolve(&DataDirRequest::default()).expect("解析");
    assert_eq!(resolved.mode, DataMode::Portable);
    assert_eq!(resolved.launcher_home, PathBuf::from("/games/ULL/.ULL"));
    assert_eq!(resolved.game_home, PathBuf::from("/games/ULL/.minecraft"));
    assert!(resolved.redirected_from.is_none());
}

#[test]
fn portable_without_exe_dir_errors() {
    let env = MapEnv::new();
    let resolver = Resolver::with_env(Platform::Linux, None, &env);
    let err = resolver
        .resolve(&DataDirRequest::default())
        .expect_err("应当报错");
    assert!(matches!(err, PlatformError::NoExecutableDir));
}

#[test]
fn standard_uses_per_os_locations() {
    let windows = MapEnv::new().with("APPDATA", "/roaming");
    assert_eq!(
        standard_launcher_home(Platform::Windows, &windows),
        Some(PathBuf::from("/roaming/ULL"))
    );
    assert_eq!(
        standard_game_home(Platform::Windows, &windows),
        Some(PathBuf::from("/roaming/.minecraft"))
    );

    let mac = MapEnv::new().with("HOME", "/Users/u");
    assert_eq!(
        standard_launcher_home(Platform::MacOs, &mac),
        Some(PathBuf::from("/Users/u/Library/Application Support/ULL"))
    );
    assert_eq!(
        standard_game_home(Platform::MacOs, &mac),
        Some(PathBuf::from(
            "/Users/u/Library/Application Support/minecraft"
        ))
    );

    // XDG 规范要求绝对路径；测试用 temp_dir（两种平台都算绝对路径）。
    let xdg_root = std::env::temp_dir().join("ull-xdg");
    let xdg_text = xdg_root.to_string_lossy().into_owned();
    let linux = MapEnv::new()
        .with("HOME", "/home/u")
        .with("XDG_DATA_HOME", &xdg_text);
    assert_eq!(
        standard_launcher_home(Platform::Linux, &linux),
        Some(xdg_root.join("ull"))
    );
    assert_eq!(
        standard_game_home(Platform::Linux, &linux),
        Some(PathBuf::from("/home/u/.minecraft"))
    );

    // 相对 XDG_DATA_HOME 不合法 → 回落 ~/.local/share/ull。
    let relative_xdg = MapEnv::new()
        .with("HOME", "/home/u")
        .with("XDG_DATA_HOME", "relative/xdg");
    assert_eq!(
        standard_launcher_home(Platform::Linux, &relative_xdg),
        Some(PathBuf::from("/home/u/.local/share/ull"))
    );
}

#[test]
fn standard_falls_back_when_env_missing() {
    let env = MapEnv::new()
        .with("HOME", "/home/u")
        .with("USERPROFILE", "/home/u");
    assert_eq!(
        standard_launcher_home(Platform::Linux, &env),
        Some(PathBuf::from("/home/u/.local/share/ull"))
    );
    assert_eq!(
        standard_launcher_home(Platform::Windows, &env),
        Some(PathBuf::from("/home/u/AppData/Roaming/ULL"))
    );

    let empty = MapEnv::new();
    assert_eq!(standard_launcher_home(Platform::Windows, &empty), None);
}

#[test]
fn standard_mode_resolution_and_macos_default() {
    let env = MapEnv::new().with("HOME", "/Users/u");
    let resolver = Resolver::with_env(Platform::MacOs, None, &env);
    assert_eq!(resolver.default_mode(), DataMode::Standard);

    let request = DataDirRequest {
        mode: DataMode::Standard,
        ..DataDirRequest::default()
    };
    let resolved = resolver.resolve(&request).expect("解析");
    assert_eq!(
        resolved.launcher_home,
        PathBuf::from("/Users/u/Library/Application Support/ULL")
    );
    assert_eq!(
        resolved.game_home,
        PathBuf::from("/Users/u/Library/Application Support/minecraft")
    );
}

#[test]
fn custom_requires_absolute_existing_root_choice() {
    let env = MapEnv::new();
    let resolver = Resolver::with_env(Platform::Windows, Some(PathBuf::from("/games")), &env);
    let root = std::env::temp_dir().join("ull-custom-root");

    let ok = DataDirRequest {
        mode: DataMode::Custom,
        custom_root: Some(root.clone()),
        ..DataDirRequest::default()
    };
    let resolved = resolver.resolve(&ok).expect("解析");
    assert_eq!(resolved.launcher_home, root.join(".ULL"));
    assert_eq!(resolved.game_home, root.join(".minecraft"));

    let relative = DataDirRequest {
        mode: DataMode::Custom,
        custom_root: Some(PathBuf::from("relative/dir")),
        ..DataDirRequest::default()
    };
    assert!(matches!(
        resolver.resolve(&relative).expect_err("相对路径应报错"),
        PlatformError::InvalidCustomRoot { .. }
    ));

    let missing = DataDirRequest {
        mode: DataMode::Custom,
        custom_root: None,
        ..DataDirRequest::default()
    };
    assert!(resolver.resolve(&missing).is_err());
}

#[test]
fn advanced_overrides_split_launcher_and_game_home() {
    let env = MapEnv::new();
    let resolver = Resolver::with_env(Platform::Windows, Some(PathBuf::from("/games")), &env);
    let request = DataDirRequest {
        launcher_home: Some(PathBuf::from("/config/ULL")),
        game_home: Some(PathBuf::from("/bigdisk/.minecraft")),
        ..DataDirRequest::default()
    };
    let resolved = resolver.resolve(&request).expect("解析");
    assert_eq!(resolved.launcher_home, PathBuf::from("/config/ULL"));
    assert_eq!(resolved.game_home, PathBuf::from("/bigdisk/.minecraft"));
    assert!(!resolved.same_target(&ResolvedData {
        mode: DataMode::Portable,
        launcher_home: PathBuf::from("/other"),
        game_home: PathBuf::from("/bigdisk/.minecraft"),
        redirected_from: None,
    }));
}

#[test]
fn effective_resolution_follows_redirect_pointer() {
    let temp = TempDir::new("data-dir-redirect");
    let exe_dir = temp.child("portable");
    let moved_to = temp.child("after-move");
    std::fs::create_dir_all(&exe_dir).expect("建目录");

    let pointer = RedirectPointer {
        schema_version: REDIRECT_SCHEMA_VERSION,
        mode: DataMode::Custom,
        launcher_home: moved_to.clone(),
        game_home: moved_to.join(".minecraft"),
        migrated_at_ms: 1_700_000_000_000,
    };
    redirect::write(&exe_dir.join(".ULL"), &pointer).expect("写指针");

    let env = MapEnv::new();
    let resolver = Resolver::with_env(Platform::Windows, Some(exe_dir.clone()), &env);
    let resolved = resolver
        .resolve_default(&DataDirRequest::default())
        .expect("解析");

    assert_eq!(resolved.launcher_home, moved_to);
    assert_eq!(resolved.mode, DataMode::Portable);
    assert_eq!(
        resolved.redirected_from,
        Some(exe_dir.join(".ULL")),
        "旧目录要留在记录里，便于 UI 提示「数据已迁走」"
    );
}
