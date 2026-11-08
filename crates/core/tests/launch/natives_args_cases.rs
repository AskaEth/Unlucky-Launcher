//! natives 时代分界与参数接线：清单口径收集、`-Djava.library.path` 归属、1.19+ 四个目录属性。
//!
//! 抽取行为本身在 `natives_cases.rs`；这里只验「参数从哪来、收集口径对不对」。

use unlucky_core::launch::natives;

use crate::prelude::*;

/// 参数接线：legacy 清单没有 `arguments.jvm`，`-Djava.library.path` 由启动器补且只补一次。
#[test]
fn legacy_manifest_gets_single_java_library_path() {
    let (request, ctx) = legacy_forge_request();
    let command = assemble(&request, &ctx).expect("组装应成功");
    let natives = request.dirs.natives_dir.display().to_string();
    let library_path: Vec<&String> = command
        .args
        .iter()
        .filter(|arg| arg.starts_with("-Djava.library.path="))
        .collect();
    assert_eq!(library_path.len(), 1, "legacy 只该有一条：{library_path:?}");
    assert_eq!(library_path[0], &format!("-Djava.library.path={natives}"));
}

/// 参数接线：1.13+ 清单自带 `-Djava.library.path=${natives_directory}`，启动器不得重复注入。
#[test]
fn modern_manifest_keeps_its_own_library_path_once() {
    let (mut request, ctx) = legacy_forge_request();
    let natives = request.dirs.natives_dir.display().to_string();
    request.manifest.jvm = vec![
        ArgumentEntry::plain(["-Djava.library.path=${natives_directory}".to_string()]),
        ArgumentEntry::plain(["-cp".to_string(), "${classpath}".to_string()]),
    ];
    request.manifest.legacy_arguments = None;
    let command = assemble(&request, &ctx).expect("组装应成功");
    let library_path: Vec<&String> = command
        .args
        .iter()
        .filter(|arg| arg.starts_with("-Djava.library.path="))
        .collect();
    assert_eq!(library_path.len(), 1, "清单 + 默认值不该叠加");
    assert_eq!(library_path[0], &format!("-Djava.library.path={natives}"));
}

/// 参数接线：1.19+ 的四个目录属性都要渲染到同一目录（LWJGL 3.3 运行期往那里解压）。
#[test]
fn classpath_native_era_renders_all_four_directory_properties() {
    let (mut request, ctx) = legacy_forge_request();
    let natives = request.dirs.natives_dir.display().to_string();
    request.manifest.jvm = vec![ArgumentEntry::plain([
        "-Djava.library.path=${natives_directory}".to_string(),
        "-Djna.tmpdir=${natives_directory}".to_string(),
        "-Dorg.lwjgl.system.SharedLibraryExtractPath=${natives_directory}".to_string(),
        "-Dio.netty.native.workdir=${natives_directory}".to_string(),
    ])];
    request.manifest.legacy_arguments = None;
    let command = assemble(&request, &ctx).expect("组装应成功");
    for key in [
        "-Djava.library.path=",
        "-Djna.tmpdir=",
        "-Dorg.lwjgl.system.SharedLibraryExtractPath=",
        "-Dio.netty.native.workdir=",
    ] {
        assert!(
            command
                .args
                .iter()
                .any(|arg| arg == &format!("{key}{natives}")),
            "缺少 {key}{natives}：{:?}",
            command.args
        );
    }
}

/// 收集口径：`natives` 表按 OS 命中 → 分类器 jar；1.19+ 无表 → 空；
/// `hint=local` 走实例私有目录（HMCL `analyse/03` §6.8 口径）。
///
/// 注意：安装层当前的 natives 分类器分支**不认** `hint=local`（走共享 `libraries/`），
/// 所以这一条断言测的是 `collect` 的正确行为，不是「与安装层一致」——见
/// `collect` 文档里的已知缺口说明。
#[test]
fn collect_picks_classifier_jars_per_environment() {
    use unlucky_core::game::{Environment, GameLayout, Library};

    let libraries: Vec<Library> = serde_json::from_str(
        r#"[
            {"name":"org.lwjgl:lwjgl:3.2.2",
             "natives":{"linux":"natives-linux","windows":"natives-windows"},
             "extract":{"exclude":["META-INF/"]},
             "downloads":{"artifact":{"path":"org/lwjgl/lwjgl/3.2.2/lwjgl-3.2.2.jar","url":"https://libraries.minecraft.net/a.jar"},
                          "classifiers":{"natives-windows":{"path":"org/lwjgl/lwjgl/3.2.2/lwjgl-3.2.2-natives-windows.jar","url":"https://libraries.minecraft.net/b.jar"}}}},
            {"name":"org.lwjgl:lwjgl-tinyfd:3.2.2","natives":{"windows":"natives-windows"},
             "hint":"local",
             "downloads":{"classifiers":{"natives-windows":{"path":"org/lwjgl/lwjgl-tinyfd/3.2.2/lwjgl-tinyfd-3.2.2-natives-windows.jar","url":"https://libraries.minecraft.net/c.jar"}}}},
            {"name":"org.lwjgl:lwjgl:3.3.3:natives-linux",
             "rules":[{"action":"allow","os":{"name":"linux"}}],
             "downloads":{"artifact":{"path":"org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-linux.jar","url":"https://libraries.minecraft.net/d.jar"}}}
        ]"#,
    )
    .expect("库 JSON 应可解析");
    let layout = GameLayout::new("C:/mc");
    let environment = Environment {
        os: "windows".to_string(),
        os_version: "10.0.19045".to_string(),
        arch: "x86_64".to_string(),
        ..Environment::default()
    };

    let natives = natives::collect(&layout, "1.16.5", &libraries, &environment);
    assert_eq!(natives.len(), 2, "只该收 windows 分类器：{natives:?}");
    assert!(
        natives[0]
            .path
            .ends_with("libraries/org/lwjgl/lwjgl/3.2.2/lwjgl-3.2.2-natives-windows.jar"),
        "{:?}",
        natives[0].path
    );
    assert_eq!(natives[0].name, "org.lwjgl:lwjgl:3.2.2:natives-windows");
    assert_eq!(natives[0].exclude, vec!["META-INF/".to_string()]);
    // `hint=local`：按 HMCL 口径落实例私有目录（安装层尚未跟进，见函数文档）。
    assert!(
        natives[1]
            .path
            .ends_with("versions/1.16.5/libraries/org/lwjgl/lwjgl-tinyfd/3.2.2/lwjgl-tinyfd-3.2.2-natives-windows.jar"),
        "{:?}",
        natives[1].path
    );

    // 1.19+ 形态（无 natives 表）→ 空表（natives jar 在 classpath 上）。
    let modern: Vec<Library> = serde_json::from_str(
        r#"[{"name":"org.lwjgl:lwjgl:3.3.3:natives-windows",
             "rules":[{"action":"allow","os":{"name":"windows"}}],
             "downloads":{"artifact":{"path":"org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar","url":"https://libraries.minecraft.net/e.jar"}}}]"#,
    )
    .expect("库 JSON 应可解析");
    assert!(natives::collect(&layout, "1.20.1", &modern, &environment).is_empty());
}
