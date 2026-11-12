//! 测试共用设施。
//!
//! **crate 名只在本文件出现**（tower 定案 `unlucky_core`；若 M11 改包名，只改这里）。

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

pub use unlucky_core::launch::args_table::{assemble, AssembledCommand, AssemblyWarning};
pub use unlucky_core::launch::argument::{ArgumentEntry, Rule};
pub use unlucky_core::launch::command_builder::{self, CommandBuilder};
pub use unlucky_core::launch::error::{AssemblyError, LaunchError, MissingComponent};
pub use unlucky_core::launch::exit_analysis::{self, ExitType};
pub use unlucky_core::launch::game_version::GameVersion;
pub use unlucky_core::launch::log_level::{self, Level};
pub use unlucky_core::launch::platform::{OsFamily, PlatformCtx};
pub use unlucky_core::launch::pump::{LineSink, LogLine, RingBuffer, RingBufferConfig, Stream};
pub use unlucky_core::launch::redact::{Redactor, TOKEN_PLACEHOLDER};
pub use unlucky_core::launch::request::*;
pub use unlucky_core::launch::session::{EndReason, SessionMeta, SessionSink, SessionWriter};
pub use unlucky_core::launch::session_index;
pub use unlucky_core::launch::window_detect::WindowDetector;

pub use unlucky_core::launch::adapter::{
    build_request, AdapterOptions, AdapterWarning, LibraryInput, SnapshotInput,
};
pub use unlucky_core::launch::crash_scan::CrashSource;
pub use unlucky_core::launch::diagnosis_hook::{DiagnosisHook, DiagnosisRequest};
pub use unlucky_core::launch::launch_session::SessionHandle;
pub use unlucky_core::launch::launcher::{DryRun, Launcher, LauncherConfig};
pub use unlucky_core::launch::process::ProcessConfig;
pub use unlucky_core::launch::sink::{CompositeSink, CountingSink, FnSink};

/// 断言 argv 逐项相等，失败时打印全部实际值。
pub fn assert_argv(command: &AssembledCommand, expected: &[&str]) {
    let actual = command.argv();
    if actual.len() != expected.len() {
        panic!(
            "argv 长度 {} != 期望 {}\n实际:\n{:#?}",
            actual.len(),
            expected.len(),
            actual
        );
    }
    for (index, (actual, expected)) in actual.iter().zip(expected.iter()).enumerate() {
        assert_eq!(actual, expected, "第 {index} 个参数不同");
    }
}

/// 一个“1.12.2 + Forge”的基准请求（Windows/x86_64）。
pub fn legacy_forge_request() -> (LaunchRequest, PlatformCtx) {
    let ctx = PlatformCtx::new(OsFamily::Windows, "x86_64");
    let request = LaunchRequest {
        instance: InstanceInfo {
            id: "inst-1".to_string(),
            name: "1.12.2-Forge".to_string(),
            version: GameVersion::parse("1.12.2"),
            version_type: "release".to_string(),
        },
        launcher: LauncherInfo {
            name: "UnluckyLauncher".to_string(),
            version: "0.1.0".to_string(),
        },
        java: JavaSelection::new("C:/java/bin/java.exe", 17, 64),
        memory: MemoryOptions {
            min_mb: Some(1024),
            max_mb: Some(4096),
        },
        manifest: ManifestArgs {
            main_class: Some("net.minecraft.launchwrapper.Launch".to_string()),
            ..ManifestArgs::default()
        },
        dirs: LaunchDirs {
            game_dir: PathBuf::from("C:/mc"),
            assets_root: PathBuf::from("C:/mc/assets"),
            game_assets: PathBuf::from("C:/mc/assets/virtual/legacy"),
            assets_index: "1.12".to_string(),
            natives_dir: PathBuf::from("C:/mc/versions/1.12.2/natives-windows-x86_64"),
            library_dir: PathBuf::from("C:/mc/libraries"),
            primary_jar: PathBuf::from("C:/mc/versions/1.12.2/1.12.2.jar"),
        },
        classpath: vec![
            PathBuf::from("C:/mc/libraries/a.jar"),
            PathBuf::from("C:/mc/libraries/b.jar"),
        ],
        auth: AuthPlaceholders {
            player_name: "Steve".to_string(),
            uuid: "0123456789abcdef0123456789abcdef".to_string(),
            access_token: "SECRET-TOKEN-123456".to_string(),
            user_type: "msa".to_string(),
            user_properties: "{}".to_string(),
        },
        log4j: Log4jOptions {
            enabled: true,
            config_file: Some(PathBuf::from("C:/mc/log4j2.xml")),
            debug: false,
        },
        // 大部分断言只关心“命令骨架”，优化参数会淹没差异；需要时单独打开。
        optimization: OptimizationOptions {
            enabled: false,
            physical_memory_mb: None,
        },
        language: "zh-CN".to_string(),
        features: FeatureSet {
            has_custom_resolution: true,
            ..FeatureSet::default()
        },
        resolution: Some(Resolution {
            width: 854,
            height: 480,
        }),
        ..LaunchRequest::default()
    };
    (request, ctx)
}

/// 一个唯一的临时目录（不自动清理，便于失败时排查；OS 会回收）。
pub fn temp_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let dir =
        std::env::temp_dir().join(format!("ull-launch-{name}-{}-{nanos}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("创建临时目录");
    dir
}

/// 一个唯一的临时会话根目录（测试结束不自动清理，便于失败时排查；OS 会回收）。
pub fn temp_sessions_root(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let root = std::env::temp_dir().join(format!(
        "ull-launch-test-{name}-{}-{nanos}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    root
}

/// 读回 jsonl 的每一行并解析成 JSON。
pub fn read_events(path: &std::path::Path) -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(path).expect("读取会话文件");
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("解析事件行"))
        .collect()
}

/// 造一个假 jar（`zip` Stored 模式，不依赖真下载）：`entries` 是（条目名, 内容）。
///
/// natives 用例共用（真解压与 `--dry-run` 预览两边都要现造 jar）。
pub fn make_jar(path: &Path, entries: &[(&str, &[u8])]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("建 jar 目录");
    }
    let file = File::create(path).expect("建 jar");
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (name, bytes) in entries {
        writer.start_file(*name, options).expect("写条目");
        writer.write_all(bytes).expect("写条目内容");
    }
    writer.finish().expect("收尾 jar");
}

/// 一份 1.16.5 风格的 natives jar 的条目集（2 个原生库 + 4 条该被排除的噪音）。
pub fn lwjgl_jar_entries() -> Vec<(&'static str, &'static [u8])> {
    vec![
        ("lwjgl.dll", b"DLL-BYTES".as_slice()),
        ("OpenAL.dll", b"OPENAL-BYTES".as_slice()),
        ("META-INF/MANIFEST.MF", b"manifest".as_slice()),
        ("META-INF/LWJGL.SF", b"signature".as_slice()),
        ("lwjgl.dll.sha1", b"deadbeef".as_slice()),
        ("README.txt", b"noise".as_slice()),
    ]
}
