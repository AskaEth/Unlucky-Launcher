//! game 快照 → `LaunchRequest` 适配层测试（缺失库显式记录、features 派生）。

use crate::prelude::*;

fn write_file(path: &std::path::Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("建目录");
    }
    std::fs::write(path, content).expect("写文件");
}

fn jvm_tokens() -> Vec<String> {
    vec![
        "-Djava.library.path=${natives_directory}".to_string(),
        "-cp".to_string(),
        "${classpath}".to_string(),
    ]
}

fn game_tokens() -> Vec<String> {
    vec![
        "--username".to_string(),
        "${auth_player_name}".to_string(),
        "--version".to_string(),
        "${version_name}".to_string(),
    ]
}

#[test]
fn adapter_records_missing_libraries_and_fails_before_start() {
    let dir = temp_dir("adapter-strict");
    let lib_ok = dir.join("libraries/a.jar");
    let lib_missing = dir.join("libraries/b.jar");
    let primary = dir.join("versions/1.20.1/1.20.1.jar");
    write_file(&lib_ok, "x");
    write_file(&primary, "x");
    let libraries = vec![
        LibraryInput::new(lib_ok.clone(), "org.example:a:1"),
        LibraryInput::new(lib_missing.clone(), "org.example:b:2"),
    ];
    let jvm = jvm_tokens();
    let game = game_tokens();
    let snapshot = SnapshotInput {
        version_name: Some("1.20.1 Fabric"),
        version_type: Some("release"),
        game_version: Some("1.20.1"),
        main_class: Some("net.fabricmc.loader.impl.launch.knot.KnotClient"),
        jvm_arguments: &jvm,
        game_arguments: &game,
        libraries: &libraries,
        primary_jar: primary.clone(),
        game_dir: dir.clone(),
        assets_index: Some("17"),
        natives_dir: dir.join("versions/1.20.1/natives"),
        library_dir: dir.join("libraries"),
        assets_root: dir.join("assets"),
        game_assets: dir.join("assets/virtual/legacy"),
        ..SnapshotInput::minimal("1.20.1", "")
    };
    let mut options = AdapterOptions::strict();
    options.java = JavaSelection::new("C:/java/bin/java.exe", 21, 64);
    options.memory = MemoryOptions {
        min_mb: Some(512),
        max_mb: Some(2048),
    };
    options.resolution = Some(Resolution {
        width: 1280,
        height: 720,
    });

    let output = build_request(&snapshot, options);
    assert_eq!(output.classpath_entries, 2, "a.jar + 主 jar");
    assert_eq!(output.missing.len(), 1);
    assert_eq!(output.missing[0].name, "org.example:b:2");
    assert!(output.warnings.iter().any(|warning| matches!(
        warning,
        AdapterWarning::MissingLibrary { name, .. } if name == "org.example:b:2"
    )));
    // features 派生：给了分辨率就打开 has_custom_resolution；1.20.1 支持 QuickPlay。
    assert!(output.request.features.has_custom_resolution);
    assert!(output.request.features.has_quick_plays_support);
    assert_eq!(output.request.instance.name, "1.20.1 Fabric");
    assert_eq!(
        output.request.manifest.main_class.as_deref(),
        Some("net.fabricmc.loader.impl.launch.knot.KnotClient")
    );

    // 严格模式下缺失库 → 组装前显式失败（不再交给运行时 NoClassDefFoundError）。
    let ctx = PlatformCtx::new(OsFamily::Windows, "x86_64");
    match assemble(&output.request, &ctx) {
        Err(LaunchError::Assembly(AssemblyError::MissingComponents(items))) => {
            assert_eq!(items.len(), 1);
        }
        other => panic!("应因缺组件失败，实际：{other:?}"),
    }
}

#[test]
fn adapter_tolerant_mode_skips_missing_libraries() {
    let dir = temp_dir("adapter-tolerant");
    let lib_missing = dir.join("libraries/b.jar");
    let libraries = vec![LibraryInput::new(lib_missing.clone(), "org.example:b:2")];
    let jvm = jvm_tokens();
    let snapshot = SnapshotInput {
        main_class: Some("net.minecraft.client.main.Main"),
        jvm_arguments: &jvm,
        libraries: &libraries,
        primary_jar: dir.join("client.jar"),
        game_dir: dir.clone(),
        assets_index: Some("5"),
        ..SnapshotInput::minimal("1.12.2", "")
    };
    let mut options = AdapterOptions::strict();
    options.check_classpath = false;
    options.java = JavaSelection::new("java", 8, 64);

    let output = build_request(&snapshot, options);
    assert!(
        !output.request.classpath.is_empty(),
        "宽松模式仍保留库（交给启动期报错）"
    );
    assert!(output.request.missing_components.is_empty());
    // 宽松模式不报缺失库（其它告警——如缺游戏参数——与本用例无关）。
    assert!(!output
        .warnings
        .iter()
        .any(|warning| matches!(warning, AdapterWarning::MissingLibrary { .. })));
}

#[test]
fn adapter_warns_on_incomplete_manifest() {
    let dir = temp_dir("adapter-incomplete");
    let snapshot = SnapshotInput {
        main_class: None,
        ..SnapshotInput::minimal("1.7.10", "")
    };
    let output = build_request(&snapshot, AdapterOptions::strict());
    let warnings = &output.warnings;
    assert!(warnings.contains(&AdapterWarning::MissingMainClass));
    assert!(warnings.contains(&AdapterWarning::MissingAssetsIndex));
    assert!(warnings.contains(&AdapterWarning::MissingGameArguments));
    // 1.7.10 < 1.20：不支持 QuickPlay。
    assert!(!output.request.features.has_quick_plays_support);
    assert_eq!(
        output.request.dirs.assets_index, "legacy",
        "缺索引时兜底 legacy"
    );
    let _ = dir;
}

#[test]
fn adapter_legacy_arguments_are_passed_through() {
    let dir = temp_dir("adapter-legacy");
    let snapshot = SnapshotInput {
        main_class: Some("net.minecraft.launchwrapper.Launch"),
        legacy_arguments: Some("--username ${auth_player_name} --gameDir ${game_directory}"),
        game_dir: dir.clone(),
        ..SnapshotInput::minimal("1.12.2", "")
    };
    let output = build_request(&snapshot, AdapterOptions::strict());
    assert!(output.request.manifest.is_legacy());
    assert!(output.request.manifest.game.is_empty());
    assert!(!output
        .warnings
        .contains(&AdapterWarning::MissingGameArguments));
}
