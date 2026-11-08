use std::path::PathBuf;

use super::*;
use crate::game::launch_overrides::{
    resolve, GlobalLaunchDefaults, GraphicsBackend, LaunchOverrides,
};
use crate::launch::adapter::{build_request, AdapterOptions, LibraryInput, SnapshotInput};
use crate::launch::args_table::{assemble, AssemblyWarning};
use crate::launch::game_version::GameVersion;
use crate::launch::platform::{OsFamily, PlatformCtx};
use crate::launch::request::{
    AuthPlaceholders, InstanceInfo, JavaSelection, LaunchDirs, LauncherInfo, Log4jOptions,
    ManifestArgs, OptimizationOptions,
};

fn request(version: &str) -> LaunchRequest {
    LaunchRequest {
        instance: InstanceInfo {
            id: "inst-1".to_string(),
            name: version.to_string(),
            version: GameVersion::parse(version),
            version_type: "release".to_string(),
        },
        launcher: LauncherInfo {
            name: "UnluckyLauncher".to_string(),
            version: "0.2.2".to_string(),
        },
        java: JavaSelection::new("C:/java/bin/java.exe", 21, 64),
        memory: MemoryOptions {
            min_mb: None,
            max_mb: None,
        },
        manifest: ManifestArgs {
            main_class: Some("net.minecraft.client.main.Main".to_string()),
            ..ManifestArgs::default()
        },
        dirs: LaunchDirs {
            game_dir: PathBuf::from("C:/mc"),
            primary_jar: PathBuf::from("C:/mc/versions/x/x.jar"),
            ..LaunchDirs::default()
        },
        classpath: vec![PathBuf::from("C:/mc/libraries/a.jar")],
        auth: AuthPlaceholders::default(),
        log4j: Log4jOptions {
            enabled: false,
            ..Log4jOptions::default()
        },
        optimization: OptimizationOptions {
            enabled: false,
            physical_memory_mb: None,
        },
        language: "zh-CN".to_string(),
        ..LaunchRequest::default()
    }
}

fn ctx() -> PlatformCtx {
    PlatformCtx::new(OsFamily::Windows, "x86_64")
}

fn argvs(req: &LaunchRequest) -> Vec<String> {
    assemble(req, &ctx()).expect("组装应成功").args
}

fn position(args: &[String], needle: &str) -> Option<usize> {
    args.iter().position(|arg| arg == needle)
}

/// **叠加规则**：实例段在 `-Xmx` 之前、全局段在之后；两段都下发（不替换）。
#[test]
fn instance_and_global_jvm_args_stack_in_order() {
    let instance = LaunchOverrides {
        jvm_args: Some("-XX:+UseG1GC".to_string()),
        ..LaunchOverrides::default()
    };
    let global = GlobalLaunchDefaults {
        jvm_args: Some("-Xss4m".to_string()),
        ..GlobalLaunchDefaults::default()
    };
    let resolved = resolve(Some(&instance), &global);
    let mut req = request("1.20.4");
    apply(&mut req, &resolved);
    let args = argvs(&req);

    let instance_at = position(&args, "-XX:+UseG1GC").expect("实例参数要下发");
    let memory_at = position(&args, "-Xmx2048m").expect("内存字段要产 -Xmx");
    let global_at = position(&args, "-Xss4m").expect("全局参数要下发");
    assert!(
        instance_at < memory_at,
        "实例段（override_jvm）必须在 -Xmx 之前：{args:?}"
    );
    assert!(
        global_at > memory_at,
        "全局段（user.jvm）在 -Xmx 之后（基线不抢内存字段）：{args:?}"
    );
}

/// **-Xmx 冲突**：实例文本写 `-Xmx` ⇒ 内存字段不再产出（文本赢）；全局文本写 `-Xmx` ⇒ 被内存字段抑制。
#[test]
fn xmx_conflict_follows_the_assembly_order() {
    let instance = LaunchOverrides {
        jvm_args: Some("-Xmx3g".to_string()),
        ..LaunchOverrides::default()
    };
    let resolved = resolve(Some(&instance), &GlobalLaunchDefaults::default());
    let mut req = request("1.20.4");
    apply(&mut req, &resolved);
    let args = argvs(&req);
    assert!(
        args.iter().any(|arg| arg == "-Xmx3g"),
        "实例文本的 -Xmx 必须生效：{args:?}"
    );
    assert!(
        !args.iter().any(|arg| arg == "-Xmx2048m"),
        "内存字段的 -Xmx 必须被抑制：{args:?}"
    );

    let resolved = resolve(None, &GlobalLaunchDefaults::with_jvm_args("-Xmx3g"));
    let mut req = request("1.20.4");
    apply(&mut req, &resolved);
    let args = argvs(&req);
    assert!(
        args.iter().any(|arg| arg == "-Xmx2048m"),
        "全局文本的 -Xmx 不生效（第 7 项晚于第 5 项）：{args:?}"
    );
    assert!(!args.iter().any(|arg| arg == "-Xmx3g"), "{args:?}");
}

/// 内存三级回落落进 `-Xmx`：实例 → 全局 → 默认。
#[test]
fn memory_falls_back_into_xmx() {
    let global = GlobalLaunchDefaults {
        memory_mb: Some(6144),
        ..GlobalLaunchDefaults::default()
    };
    let instance = LaunchOverrides {
        memory_mb: Some(8192),
        ..LaunchOverrides::default()
    };
    for (overrides, expected) in [(Some(instance), "-Xmx8192m"), (None, "-Xmx6144m")] {
        let resolved = resolve(overrides.as_ref(), &global);
        let mut req = request("1.20.4");
        apply(&mut req, &resolved);
        assert!(
            argvs(&req).iter().any(|arg| arg == expected),
            "期望 {expected}"
        );
    }
    let resolved = resolve(None, &GlobalLaunchDefaults::default());
    let mut req = request("1.20.4");
    apply(&mut req, &resolved);
    assert!(argvs(&req).iter().any(|arg| arg == "-Xmx2048m"));
}

/// 游戏参数最后追加（可覆盖前面一切游戏参数）。
#[test]
fn game_args_land_at_the_tail() {
    let instance = LaunchOverrides {
        game_args: Some("--fullscreen --width 1920".to_string()),
        ..LaunchOverrides::default()
    };
    let resolved = resolve(Some(&instance), &GlobalLaunchDefaults::default());
    let mut req = request("1.20.4");
    apply(&mut req, &resolved);
    let args = argvs(&req);
    let tail = &args[args.len() - 3..];
    assert_eq!(tail, ["--fullscreen", "--width", "1920"], "{args:?}");
}

/// **版本门槛**：26.2-snapshot-2+ 才下发；老版本不下发但必须给诊断告警（不静默）。
#[test]
fn graphics_backend_is_version_gated_with_a_warning() {
    let instance = LaunchOverrides {
        graphics_backend: Some(GraphicsBackend::Vulkan),
        ..LaunchOverrides::default()
    };
    let resolved = resolve(Some(&instance), &GlobalLaunchDefaults::default());

    let mut supported = request("26.2-snapshot-2");
    apply(&mut supported, &resolved);
    let command = assemble(&supported, &ctx()).expect("组装");
    let at = position(&command.args, "--graphicsBackend").expect("支持版本要下发");
    assert_eq!(command.args[at + 1], "vulkan");
    assert!(
        !command
            .warnings
            .iter()
            .any(|warning| matches!(warning, AssemblyWarning::GraphicsBackendSkipped { .. })),
        "支持版本不该有跳过告警：{:?}",
        command.warnings
    );

    let mut old = request("1.20.4");
    apply(&mut old, &resolved);
    let command = assemble(&old, &ctx()).expect("组装");
    assert!(
        !command.args.iter().any(|arg| arg == "--graphicsBackend"),
        "老版本不许下发：{:?}",
        command.args
    );
    assert!(
        command
            .warnings
            .iter()
            .any(|warning| matches!(warning, AssemblyWarning::GraphicsBackendSkipped { .. })),
        "跳过必须留诊断：{:?}",
        command.warnings
    );
}

/// `default` = 不下发、不告警（三态里的「跟随游戏默认」）。
#[test]
fn default_backend_emits_nothing() {
    for instance in [None, Some(LaunchOverrides::default())] {
        let resolved = resolve(instance.as_ref(), &GlobalLaunchDefaults::default());
        let mut req = request("26.2");
        apply(&mut req, &resolved);
        let command = assemble(&req, &ctx()).expect("组装");
        assert!(!command.args.iter().any(|arg| arg == "--graphicsBackend"));
        assert!(command.warnings.is_empty(), "{:?}", command.warnings);
    }
}

/// 启动器自带的 `--fullscreen`（第 29 项）与实例游戏参数共存：实例在最后，可覆盖。
#[test]
fn instance_game_args_win_over_launcher_defaults() {
    let instance = LaunchOverrides {
        game_args: Some("--fullscreen".to_string()),
        ..LaunchOverrides::default()
    };
    let resolved = resolve(Some(&instance), &GlobalLaunchDefaults::default());
    let mut req = request("1.20.4");
    req.fullscreen = true;
    apply(&mut req, &resolved);
    let args = argvs(&req);
    assert_eq!(
        args.iter()
            .filter(|arg| arg.as_str() == "--fullscreen")
            .count(),
        2,
        "两处都给 --fullscreen，由 MC 取后者（实例段在最后）：{args:?}"
    );
    assert_eq!(args.last().map(String::as_str), Some("--fullscreen"));
    assert!(
        position(&args, "net.minecraft.client.main.Main").unwrap() < args.len() - 1,
        "实例游戏参数必须在主类之后"
    );
}

/// Java 路径不进请求、只作为调用方的选择依据。
#[test]
fn java_path_is_a_scalar_for_the_caller() {
    let instance = LaunchOverrides {
        java_path: Some(PathBuf::from("D:/jdk21/bin/java.exe")),
        ..LaunchOverrides::default()
    };
    let resolved = resolve(Some(&instance), &GlobalLaunchDefaults::default());
    assert_eq!(
        java_binary(&resolved),
        Some(Path::new("D:/jdk21/bin/java.exe"))
    );
    let mut req = request("1.20.4");
    let before = req.java.binary.clone();
    apply(&mut req, &resolved);
    assert_eq!(req.java.binary, before, "apply 不改 Java 选择");
}

/// `backend_allowed`：继承解析后的库表 + 版本门限，两条判据的**唯一组合入口**。
///
/// 它是 UI 灰化与拼装链共用的那一格：调用方只负责把「继承解析后的版本 JSON」和
/// 「游戏版本号」递进来，不许自己拼 `has_vulkan_backend && is_at_least(..)`。
#[test]
fn backend_allowed_combines_capability_and_threshold() {
    use crate::game::VersionJson;

    let with_vulkan = VersionJson::from_json_str(
        r#"{"id":"x","libraries":[{"name":"org.lwjgl:lwjgl-vulkan:3.4.3"}]}"#,
    )
    .expect("解析");
    let without = VersionJson::from_json_str(r#"{"id":"x"}"#).expect("解析");

    // 可解析版本：精确门限说了算（库表只作周快照兜底）。
    assert!(backend_allowed(
        &GameVersion::parse("26.2-snapshot-2"),
        &without
    ));
    assert!(!backend_allowed(
        &GameVersion::parse("26.2-snapshot-1"),
        &with_vulkan
    ));
    assert!(!backend_allowed(
        &GameVersion::parse("1.20.4"),
        &with_vulkan
    ));
    // 周快照：看库表。
    assert!(backend_allowed(&GameVersion::parse("26w14a"), &with_vulkan));
    assert!(!backend_allowed(&GameVersion::parse("26w14a"), &without));
}

/// **周快照 + 加载器**（M157 r3 复审 P2-1）：装配门限必须按**继承链的 MC 根版本**判。
///
/// 造一个 Fabric 实例（id `26w14a-fabric-0.16.5`，继承周快照 `26w14a`），库表里有
/// `org.lwjgl:lwjgl-vulkan`：`game_version = Some("26w14a")` ⇒ 命令行带 `--graphicsBackend vulkan`；
/// 退回到「拿带加载器后缀的 version_id 当游戏版本」⇒ `GameVersion::is_weekly_snapshot`
/// 的 6 字符形态认不出来，参数被跳过（**负向锚点**：这正是复审复现出来的静默丢失）。
///
/// 抓得住：adapter 的 `game_version` 传错（调用方又回退成 `None`）——周快照 loader 实例
/// 「UI 说可用、命令行里没有」。
/// 抓不住：两个调用点自己有没有传（那由 presenter `host::launch::tests` 与
/// CLI `snapshot::tests` 各自的根版本断言守）。
#[test]
fn modded_weekly_snapshot_needs_the_root_version_for_the_gate() {
    let version_id = "26w14a-fabric-0.16.5";
    let libraries = [LibraryInput::new(
        "C:/mc/libraries/org/lwjgl/lwjgl-vulkan/3.4.3/lwjgl-vulkan-3.4.3.jar",
        "org.lwjgl:lwjgl-vulkan:3.4.3",
    )];

    for (game_version, expected) in [(Some("26w14a"), true), (None, false)] {
        let snapshot = SnapshotInput {
            game_version,
            libraries: &libraries,
            primary_jar: PathBuf::from("C:/mc/versions/x/x.jar"),
            ..SnapshotInput::minimal(version_id, "net.minecraft.client.main.Main")
        };
        let options = AdapterOptions {
            // `assemble` 要求 Java 可执行文件非空（`EmptyJavaPath`）；adapter 默认是空路径，
            // 所以这里显式给一个（原来这条用例从没被执行过，r4 把它提到模块层才暴露出来）。
            java: JavaSelection::new("C:/java/bin/java.exe", 21, 64),
            check_classpath: false,
            language: "zh-CN".to_string(),
            ..AdapterOptions::default()
        };
        let mut out = build_request(&snapshot, options).request;
        let resolved = resolve(
            Some(&LaunchOverrides {
                graphics_backend: Some(GraphicsBackend::Vulkan),
                ..LaunchOverrides::default()
            }),
            &GlobalLaunchDefaults::default(),
        );
        apply(&mut out, &resolved);
        let command = assemble(&out, &ctx()).expect("组装应成功");
        let sent = command.args.iter().any(|arg| arg == "--graphicsBackend");
        assert_eq!(
            sent, expected,
            "game_version={game_version:?} 时是否下发不对：{:?}",
            command.args
        );
        let skipped = command
            .warnings
            .iter()
            .any(|warning| matches!(warning, AssemblyWarning::GraphicsBackendSkipped { .. }));
        assert_eq!(skipped, !expected, "跳过与否必须与下发相反（不静默）");
    }
}
