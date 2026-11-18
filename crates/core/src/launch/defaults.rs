//! 默认参数表：JVM 优化、编码、log4j、禁用参数、环境变量。
//!
//! 平移自 `analyse/02` §4.3（自动优化）、§4.4（编码/Log4j）、§4.1 #33（禁用参数）、
//! §4.10（环境变量）。所有函数都是纯函数——同样的输入永远给同样的参数，便于快照测试。

use crate::launch::platform::OsFamily;
use crate::launch::request::{LaunchRequest, ModLoaderKind, RendererOptions, Resolution};

/// 旧版清单的默认 JVM 参数（`analyse/02` §4.7 的 `DEFAULT_JVM_ARGUMENTS`）。
///
/// 现代清单把这两项写在自己的 `arguments.jvm` 里，只有 legacy 需要补。
pub fn legacy_jvm_arguments() -> Vec<String> {
    vec![
        "-Djava.library.path=${natives_directory}".to_string(),
        "-cp".to_string(),
        "${classpath}".to_string(),
    ]
}

/// stdout/stderr 编码参数。
///
/// Java ≥19 用 `-Dstdout.encoding`，更早用 `-Dsun.stdout.encoding`（`analyse/02` §4.4：
/// 用户指定的 `-Dfile.encoding` 会决定日志泵的解码方式，这里必须跟随）。
pub fn stream_encoding_args(java_major: u32, encoding: &str) -> Vec<String> {
    if java_major >= 19 {
        vec![
            format!("-Dstdout.encoding={encoding}"),
            format!("-Dstderr.encoding={encoding}"),
        ]
    } else {
        vec![
            format!("-Dsun.stdout.encoding={encoding}"),
            format!("-Dsun.stderr.encoding={encoding}"),
        ]
    }
}

/// Log4j2 RCE 缓解三件套（`analyse/02` §4.1 #9）。
pub fn log4j_mitigation_args() -> Vec<String> {
    vec![
        "-Djava.rmi.server.useCodebaseOnly=true".to_string(),
        "-Dcom.sun.jndi.rmi.object.trustURLCodebase=false".to_string(),
        "-Dcom.sun.jndi.cosnaming.object.trustURLCodebase=false".to_string(),
    ]
}

/// 自动优化参数（`analyse/02` §4.3）。
///
/// `user_specified_gc` 为真时不注入 G1 整组（用户显式挑了 GC 就别打架）；
/// 其它每一条都还会被 `CommandBuilder` 的默认值抑制再筛一遍。
pub fn optimization_args(
    java_major: u32,
    bits: u32,
    physical_memory_mb: Option<u64>,
    user_specified_gc: bool,
) -> Vec<String> {
    let mut args = vec![
        "-XX:+UnlockExperimentalVMOptions".to_string(),
        "-XX:+UnlockDiagnosticVMOptions".to_string(),
    ];
    if java_major >= 8 && !user_specified_gc {
        for arg in [
            "-XX:+UseG1GC",
            "-XX:G1MixedGCCountTarget=5",
            "-XX:G1NewSizePercent=20",
            "-XX:G1ReservePercent=20",
            "-XX:MaxGCPauseMillis=50",
            "-XX:G1HeapRegionSize=32m",
        ] {
            args.push(arg.to_string());
        }
    }
    // 崩溃诊断关键项：关掉“快速抛出时省略栈”，保证日志里有完整堆栈。
    args.push("-XX:-OmitStackTraceInFastThrow".to_string());
    if java_major <= 8 {
        args.push("-XX:MaxInlineLevel=15".to_string());
    }
    if bits >= 64 && physical_memory_mb.is_some_and(|mb| mb > 4096) {
        for arg in [
            "-XX:-DontCompileHugeMethods",
            "-XX:MaxNodeLimit=240000",
            "-XX:NodeLimitFudgeFactor=8000",
            "-XX:TieredCompileTaskTimeout=10000",
            "-XX:ReservedCodeCacheSize=400M",
            "-XX:NonNMethodCodeHeapSize=12M",
            "-XX:ProfiledCodeHeapSize=194M",
            "-XX:NmethodSweepActivity=1",
        ] {
            args.push(arg.to_string());
        }
    }
    if bits >= 64 && (25..=26).contains(&java_major) {
        args.push("-XX:+UseCompactObjectHeaders".to_string());
    }
    args
}

/// 版本特定的 JVM 开关（`analyse/02` §4.1 #15）。
pub fn version_specific_args(java_major: u32) -> Vec<String> {
    match java_major {
        16 => vec!["--illegal-access=permit".to_string()],
        24 | 25 => vec!["--sun-misc-unsafe-memory-access=allow".to_string()],
        _ => Vec::new(),
    }
}

/// Forge/NeoForge 的证书校验豁免（`analyse/02` §4.1 #16）。
pub fn fml_args() -> Vec<String> {
    vec![
        "-Dfml.ignoreInvalidMinecraftCertificates=true".to_string(),
        "-Dfml.ignorePatchDiscrepancies=true".to_string(),
    ]
}

/// 渲染器相关 `-D` 参数（`analyse/02` §4.1 #17，v1 只做 native 路径覆盖）。
///
/// 软件渲染的细节开关走环境变量（[`environment`]），因为 Mesa/llvmpipe 读的是 env。
pub fn renderer_args(renderer: &RendererOptions) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(dir) = &renderer.mesa_native_dir {
        args.push(format!("-Dorg.lwjgl.librarypath={}", dir.display()));
    }
    args
}

/// 被禁止的参数（最后统一清理，`analyse/02` §4.1 #33）。
pub fn forbidden_args(java_major: u32) -> Vec<&'static str> {
    if java_major >= 9 {
        vec!["-Xincgc"]
    } else {
        Vec::new()
    }
}

/// 旧版清单的默认游戏参数（分辨率，`analyse/02` §4.1 #26）。
pub fn legacy_game_args(resolution: Option<&Resolution>) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(res) = resolution {
        args.push("--width".to_string());
        args.push(res.width.to_string());
        args.push("--height".to_string());
        args.push(res.height.to_string());
    }
    args
}

/// 进程环境变量（`analyse/02` §4.10）。
///
/// 顺序：实例标识 → 渲染器开关 → 加载器标记 → 用户自定义（最后覆盖）。
pub fn environment(req: &LaunchRequest, os: OsFamily) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = Vec::new();
    set_env(&mut env, "INST_NAME", &req.instance.name);
    set_env(&mut env, "INST_ID", &req.instance.id);
    set_env(
        &mut env,
        "INST_DIR",
        &req.dirs.game_dir.display().to_string(),
    );
    set_env(
        &mut env,
        "INST_MC_DIR",
        &req.dirs.game_dir.display().to_string(),
    );
    set_env(
        &mut env,
        "INST_JAVA",
        &req.java.binary.display().to_string(),
    );
    if req.mod_loader == ModLoaderKind::OptiFine {
        set_env(&mut env, "INST_OPTIFINE", "1");
    }
    if req.renderer.software {
        set_env(&mut env, "LIBGL_ALWAYS_SOFTWARE", "1");
        set_env(&mut env, "MESA_LOADER_DRIVER_OVERRIDE", "llvmpipe");
        set_env(&mut env, "GALLIUM_DRIVER", "llvmpipe");
    }
    // Termux + glibc 构建：termux-x11 下 glibc Mesa 的 zink 卡 DRI3，softpipe 是当前
    // 可用路径。放在软件渲染块之后、用户自定义之前（用户显式设置仍可覆盖）。
    if cfg!(target_os = "linux")
        && crate::platform::termux::is_termux(&crate::platform::paths::ProcessEnv)
    {
        apply_termux_gl_defaults(&mut env);
    }
    if os == OsFamily::MacOs {
        if let Some(icd) = &req.renderer.vulkan_icd {
            set_env(&mut env, "VK_ICD_FILENAMES", &icd.display().to_string());
            set_env(&mut env, "VK_DRIVER_FILES", &icd.display().to_string());
        }
    }
    match req.mod_loader {
        ModLoaderKind::Forge => set_env(&mut env, "FORGE_INSTALL", "1"),
        ModLoaderKind::NeoForge => set_env(&mut env, "NEOFORGE_INSTALL", "1"),
        ModLoaderKind::Fabric => set_env(&mut env, "FABRIC_INSTALL", "1"),
        ModLoaderKind::Quilt => set_env(&mut env, "QUILT_INSTALL", "1"),
        ModLoaderKind::LiteLoader => set_env(&mut env, "LITELOADER_INSTALL", "1"),
        ModLoaderKind::OptiFine | ModLoaderKind::Vanilla => {}
    }
    for (key, value) in &req.user.environment {
        set_env(&mut env, key, value);
    }
    env
}

fn set_env(env: &mut Vec<(String, String)>, key: &str, value: &str) {
    if let Some(slot) = env.iter_mut().find(|(existing, _)| existing == key) {
        slot.1 = value.to_string();
    } else {
        env.push((key.to_string(), value.to_string()));
    }
}

/// Termux（glibc 构建）上的默认 GL 环境：softpipe。
///
/// glibc Mesa 26 的 zink 在 termux-x11 下报 `DRI3 not available`，softpipe 是当前
/// 可用的软件路径；硬件路径等 DRI3 问题解决后再切。
fn apply_termux_gl_defaults(env: &mut Vec<(String, String)>) {
    set_env(env, "MESA_LOADER_DRIVER_OVERRIDE", "swrast");
    set_env(env, "LIBGL_ALWAYS_SOFTWARE", "1");
}

#[cfg(test)]
mod tests {
    use super::apply_termux_gl_defaults;

    #[test]
    fn termux_gl_defaults_force_softpipe() {
        let mut env = Vec::new();
        apply_termux_gl_defaults(&mut env);
        assert!(env.contains(&(
            "MESA_LOADER_DRIVER_OVERRIDE".to_string(),
            "swrast".to_string()
        )));
        assert!(env.contains(&("LIBGL_ALWAYS_SOFTWARE".to_string(), "1".to_string())));
    }

    #[test]
    fn termux_gl_defaults_overwrite_earlier_llvmpipe_choice() {
        let mut env = vec![(
            "MESA_LOADER_DRIVER_OVERRIDE".to_string(),
            "llvmpipe".to_string(),
        )];
        apply_termux_gl_defaults(&mut env);
        assert_eq!(
            env.iter()
                .find(|(key, _)| key == "MESA_LOADER_DRIVER_OVERRIDE")
                .map(|(_, value)| value.as_str()),
            Some("swrast")
        );
    }
}
