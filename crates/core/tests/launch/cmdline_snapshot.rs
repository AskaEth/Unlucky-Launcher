//! 命令行组装快照测试（`analyse/02` §4.1 的 33 项顺序表 + §4.2 默认值抑制）。
//!
//! 期望值是手工核对过的**完整 argv**：一旦组装顺序变了，这里必须一起改——
//! 顺序本身有语义（后加的默认值会被先加的显式值抑制）。

use crate::prelude::*;

#[test]
fn legacy_forge_full_argv() {
    let (mut request, ctx) = legacy_forge_request();
    request.mod_loader = ModLoaderKind::Forge;
    request.manifest.legacy_arguments = Some(
        "--username ${auth_player_name} --version ${version_name} --gameDir ${game_directory} \
         --assetsDir ${assets_root} --assetIndex ${assets_index_name} --uuid ${auth_uuid} \
         --accessToken ${auth_session} --userType ${user_type} \
         --tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker"
            .to_string(),
    );
    // 用户指定 GBK → 日志泵也必须用 GBK 解码（`analyse/02` §4.4）。
    request.user.jvm = vec!["-Dfile.encoding=GBK".to_string()];

    let command = assemble(&request, &ctx).expect("组装应成功");
    assert_eq!(command.encoding, "GBK");
    assert_eq!(command.main_class, "net.minecraft.launchwrapper.Launch");
    assert_eq!(command.classpath.len(), 3, "主 jar 必须追加在末位");

    assert_argv(
        &command,
        &[
            "C:/java/bin/java.exe",
            "-Xmx4096m",
            "-Xms1024m",
            "-XX:MetaspaceSize=64m",
            "-Dfile.encoding=GBK",
            "-Dsun.stdout.encoding=GBK",
            "-Dsun.stderr.encoding=GBK",
            "-Djava.rmi.server.useCodebaseOnly=true",
            "-Dcom.sun.jndi.rmi.object.trustURLCodebase=false",
            "-Dcom.sun.jndi.cosnaming.object.trustURLCodebase=false",
            "-Dlog4j2.formatMsgNoLookups=true",
            "-Dlog4j.configurationFile=C:/mc/log4j2.xml",
            "-Dminecraft.client.jar=C:/mc/versions/1.12.2/1.12.2.jar",
            "-Dfml.ignoreInvalidMinecraftCertificates=true",
            "-Dfml.ignorePatchDiscrepancies=true",
            "-Djava.library.path=C:/mc/versions/1.12.2/natives-windows-x86_64",
            "-cp",
            "C:/mc/libraries/a.jar;C:/mc/libraries/b.jar;C:/mc/versions/1.12.2/1.12.2.jar",
            "net.minecraft.launchwrapper.Launch",
            "--username",
            "Steve",
            "--version",
            "1.12.2-Forge",
            "--gameDir",
            "C:/mc",
            "--assetsDir",
            "C:/mc/assets",
            "--assetIndex",
            "1.12",
            "--uuid",
            "0123456789abcdef0123456789abcdef",
            "--accessToken",
            "SECRET-TOKEN-123456",
            "--userType",
            "msa",
            "--tweakClass",
            "net.minecraftforge.fml.common.launcher.FMLTweaker",
            "--width",
            "854",
            "--height",
            "480",
        ],
    );
}

#[test]
fn modern_fabric_full_argv_with_rules() {
    let (mut request, ctx) = legacy_forge_request();
    request.instance.name = "1.20.1-Fabric".to_string();
    request.instance.version = GameVersion::parse("1.20.1");
    request.manifest.main_class =
        Some("net.fabricmc.loader.impl.launch.knot.KnotClient".to_string());
    request.manifest.legacy_arguments = None;
    request.manifest.jvm = vec![
        ArgumentEntry::plain(["-Djava.library.path=${natives_directory}"]),
        ArgumentEntry::plain(["-cp", "${classpath}"]),
    ];
    request.manifest.game = vec![
        ArgumentEntry::plain(["--username", "${auth_player_name}"]),
        ArgumentEntry::plain(["--version", "${version_name}"]),
        ArgumentEntry::ruled(
            vec![Rule::allow().with_os_name("windows")],
            ["-Dos.windows=true"],
        ),
        ArgumentEntry::ruled(vec![Rule::allow().with_os_name("osx")], ["-Dos.macos=true"]),
        ArgumentEntry::ruled(
            vec![Rule::allow().with_feature("has_custom_resolution", true)],
            [
                "--width",
                "${resolution_width}",
                "--height",
                "${resolution_height}",
            ],
        ),
        // 先 allow 后 disallow：按“最后一个命中的 action”生效（`analyse/02` §4.7）。
        ArgumentEntry::ruled(
            vec![
                Rule::allow().with_os_name("windows"),
                Rule::disallow().with_feature("is_demo_user", true),
            ],
            ["--windows-only"],
        ),
    ];
    request.resolution = Some(Resolution {
        width: 1280,
        height: 720,
    });
    request.user.game = vec!["--fullscreen".to_string()];

    let command = assemble(&request, &ctx).expect("组装应成功");
    // 主 jar 由组装器追加到最后，manifest 里的 `-cp` 只覆盖库。
    assert_eq!(command.classpath.len(), 3);

    assert_argv(
        &command,
        &[
            "C:/java/bin/java.exe",
            "-Xmx4096m",
            "-Xms1024m",
            "-XX:MetaspaceSize=64m",
            "-Dfile.encoding=UTF-8",
            "-Dsun.stdout.encoding=UTF-8",
            "-Dsun.stderr.encoding=UTF-8",
            "-Djava.rmi.server.useCodebaseOnly=true",
            "-Dcom.sun.jndi.rmi.object.trustURLCodebase=false",
            "-Dcom.sun.jndi.cosnaming.object.trustURLCodebase=false",
            "-Dlog4j2.formatMsgNoLookups=true",
            "-Dlog4j.configurationFile=C:/mc/log4j2.xml",
            "-Dminecraft.client.jar=C:/mc/versions/1.12.2/1.12.2.jar",
            "-Djava.library.path=C:/mc/versions/1.12.2/natives-windows-x86_64",
            "-cp",
            "C:/mc/libraries/a.jar;C:/mc/libraries/b.jar;C:/mc/versions/1.12.2/1.12.2.jar",
            "net.fabricmc.loader.impl.launch.knot.KnotClient",
            "--username",
            "Steve",
            "--version",
            "1.20.1-Fabric",
            "-Dos.windows=true",
            "--width",
            "1280",
            "--height",
            "720",
            "--windows-only",
            "--fullscreen",
        ],
    );
    assert!(!command.args.iter().any(|arg| arg == "-Dos.macos=true"));
}

#[test]
fn user_override_beats_defaults() {
    let (mut request, ctx) = legacy_forge_request();
    request.java = JavaSelection::new("C:/java8/bin/java.exe", 8, 32);
    request.user.override_jvm = vec!["-Xmx8G".to_string(), "-XX:+UseSerialGC".to_string()];
    request.user.jvm = vec!["-Xmx2G".to_string()];
    request.optimization = OptimizationOptions {
        enabled: true,
        physical_memory_mb: Some(8192),
    };
    request.manifest.legacy_arguments = None;

    let command = assemble(&request, &ctx).expect("组装应成功");
    let args = command.args;
    // override 的 -Xmx8G 生效，默认的 -Xmx4096m 被抑制。
    assert!(args.iter().any(|arg| arg == "-Xmx8G"));
    assert!(!args.iter().any(|arg| arg == "-Xmx4096m"));
    // 用户 JVM 参数里的 -Xmx2G 属于“默认值”，被 override 抑制。
    assert!(!args.iter().any(|arg| arg == "-Xmx2G"));
    // 用户显式挑了 GC → 不注入 G1 整组，但诊断关键项照旧。
    assert!(args.iter().any(|arg| arg == "-XX:+UseSerialGC"));
    assert!(!args.iter().any(|arg| arg == "-XX:+UseG1GC"));
    assert!(args
        .iter()
        .any(|arg| arg == "-XX:-OmitStackTraceInFastThrow"));
    // 32 位老 Java → PermSize，且不因内存充裕注入 CodeCache 组。
    assert!(args.iter().any(|arg| arg == "-XX:MetaspaceSize=64m"));
    assert!(!args
        .iter()
        .any(|arg| arg.starts_with("-XX:ReservedCodeCacheSize")));
}

#[test]
fn java8_uses_legacy_encoding_keys_and_keeps_xincgc() {
    let (mut request, ctx) = legacy_forge_request();
    request.java = JavaSelection::new("C:/java8/bin/java.exe", 8, 32);
    request.user.jvm = vec!["-Xincgc".to_string()];
    request.manifest.legacy_arguments = None;
    let command = assemble(&request, &ctx).expect("组装应成功");
    assert!(command
        .args
        .iter()
        .any(|arg| arg == "-Dsun.stdout.encoding=UTF-8"));
    assert!(!command
        .args
        .iter()
        .any(|arg| arg == "-Dstdout.encoding=UTF-8"));
    // Java 8 上 -Xincgc 仍合法（只在 ≥9 上被禁，`analyse/02` §4.1 #33）。
    assert!(command.args.iter().any(|arg| arg == "-Xincgc"));

    let mut modern = request.clone();
    modern.java = JavaSelection::new("C:/java21/bin/java.exe", 21, 64);
    let command = assemble(&modern, &ctx).expect("组装应成功");
    assert!(command
        .args
        .iter()
        .any(|arg| arg == "-Dstdout.encoding=UTF-8"));
    assert!(
        !command.args.iter().any(|arg| arg == "-Xincgc"),
        "Java ≥9 必须删掉 -Xincgc"
    );
}

#[test]
fn quickplay_and_graphics_backend_are_version_gated() {
    let (mut request, ctx) = legacy_forge_request();
    request.manifest.legacy_arguments = None;
    request.quick_play = Some(QuickPlay::Multiplayer {
        address: "mc.example.com:25565".to_string(),
    });
    request.graphics_backend = Some("vulkan".to_string());

    // 1.12.2：QuickPlay 退化成 --server/--port，--graphicsBackend 丢弃。
    let legacy = assemble(&request, &ctx).expect("组装应成功");
    let server = legacy
        .args
        .windows(2)
        .any(|pair| pair[0] == "--server" && pair[1] == "mc.example.com");
    assert!(server, "老版本应退化为 --server");
    assert!(legacy
        .args
        .windows(2)
        .any(|pair| pair[0] == "--port" && pair[1] == "25565"));
    assert!(!legacy.args.iter().any(|arg| arg == "--graphicsBackend"));
    assert!(legacy
        .warnings
        .contains(&AssemblyWarning::QuickPlayDegraded {
            address: "mc.example.com:25565".to_string()
        }));
    assert!(legacy
        .warnings
        .iter()
        .any(|warning| matches!(warning, AssemblyWarning::GraphicsBackendSkipped { .. })));

    // 26.3：QuickPlay 直用，--graphicsBackend 允许。
    let mut modern = request.clone();
    modern.instance.version = GameVersion::parse("26.3");
    let modern = assemble(&modern, &ctx).expect("组装应成功");
    assert!(modern
        .args
        .windows(2)
        .any(|pair| pair[0] == "--quickPlayMultiplayer" && pair[1] == "mc.example.com:25565"));
    assert!(modern
        .args
        .windows(2)
        .any(|pair| pair[0] == "--graphicsBackend" && pair[1] == "vulkan"));
}

#[test]
fn assembly_fails_fast_on_missing_components() {
    let (mut request, ctx) = legacy_forge_request();
    request.missing_components = vec![
        MissingComponent::named("org.lwjgl:lwjgl:3.3.1"),
        MissingComponent::at_path("net.minecraft:client", "C:/mc/versions/1.12.2/1.12.2.jar"),
    ];
    let error = assemble(&request, &ctx).expect_err("缺组件必须启动前失败");
    match error {
        LaunchError::Assembly(AssemblyError::MissingComponents(items)) => {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].name, "org.lwjgl:lwjgl:3.3.1");
        }
        other => panic!("错误类型不对：{other}"),
    }

    let (mut request, ctx) = legacy_forge_request();
    request.manifest.main_class = None;
    assert!(matches!(
        assemble(&request, &ctx),
        Err(LaunchError::Assembly(AssemblyError::MissingMainClass))
    ));

    let (mut request, ctx) = legacy_forge_request();
    request.classpath.clear();
    request.dirs.primary_jar = std::path::PathBuf::new();
    assert!(matches!(
        assemble(&request, &ctx),
        Err(LaunchError::Assembly(AssemblyError::EmptyClasspath))
    ));
}

#[test]
fn posix_gets_nice_and_colon_classpath() {
    let mut ctx = PlatformCtx::new(OsFamily::Linux, "x86_64");
    ctx.home_dir = Some(std::path::PathBuf::from("/home/aska"));
    let (mut request, _) = legacy_forge_request();
    request.nice = Some(5);
    request.wrapper = vec!["prime-run".to_string()];
    request.manifest.legacy_arguments = None;

    let command = assemble(&request, &ctx).expect("组装应成功");
    assert_eq!(command.argv()[0], "nice");
    assert_eq!(command.argv()[1], "-n");
    assert_eq!(command.argv()[2], "5");
    assert_eq!(command.argv()[3], "prime-run");
    assert_eq!(command.argv()[4], "C:/java/bin/java.exe");
    assert!(command
        .args
        .iter()
        .any(|arg| arg
            == "C:/mc/libraries/a.jar:C:/mc/libraries/b.jar:C:/mc/versions/1.12.2/1.12.2.jar"));
    // 非 Windows 才注入 -Duser.home。
    assert!(command
        .args
        .iter()
        .any(|arg| arg.starts_with("-Duser.home=")));
}

#[test]
fn placeholder_table_is_complete() {
    let (request, ctx) = legacy_forge_request();
    let map = unlucky_core::launch::placeholders::PlaceholderMap::from_request(&request, &ctx);
    for key in unlucky_core::launch::placeholders::PLACEHOLDER_KEYS {
        assert!(map.get(key).is_some(), "占位符 ${key} 未登记");
    }
    assert_eq!(map.get("primary_jar_name"), Some("1.12.2.jar"));
    assert_eq!(map.get("version_name"), Some("1.12.2-Forge"));
    // 未定义的占位符原样保留（便于发现清单漏项）。
    assert_eq!(map.render("${nope}"), "${nope}");
    assert_eq!(map.render("a${version_name}b"), "a1.12.2-Forgeb");
}
