//! JVM 参数段（`analyse/02` §4.1 的第 8~17 项）。
//!
//! 这一段全部走“默认值抑制”：用户/清单显式写过的键我们一律不覆盖。
//! 返回最终生效的日志解码编码——它要传给日志泵（`analyse/02` §4.4）。

use crate::launch::args_table::AssemblyWarning;
use crate::launch::command_builder::CommandBuilder;
use crate::launch::defaults;
use crate::launch::platform::{OsFamily, PlatformCtx};
use crate::launch::request::LaunchRequest;

/// 应用第 8~17 项，返回（日志解码编码，告警）。
pub(crate) fn apply_jvm_arguments(
    req: &LaunchRequest,
    ctx: &PlatformCtx,
    builder: &mut CommandBuilder,
) -> (String, Vec<AssemblyWarning>) {
    let mut warnings = Vec::new();
    let java = &req.java;

    // 第 8 项：编码（`-Dfile.encoding=` + stdout/stderr 编码）。
    let requested = match req.encoding.as_deref() {
        Some(value) if !value.eq_ignore_ascii_case("COMPAT") => value.to_string(),
        _ => ctx.native_charset.clone(),
    };
    let existing = builder.add_default("-Dfile.encoding=", &requested);
    let mut encoding = existing
        .as_deref()
        .and_then(|arg| arg.strip_prefix("-Dfile.encoding="))
        .unwrap_or(&requested)
        .to_string();
    if encoding.eq_ignore_ascii_case("COMPAT") {
        encoding = ctx.native_charset.clone();
    }
    builder.add_all_default(&defaults::stream_encoding_args(java.major, &encoding));

    // 第 9 项：Log4j2 RCE 缓解三件套。
    builder.add_all_default(&defaults::log4j_mitigation_args());

    // 第 10 项：formatMsgNoLookups + 日志配置注入。
    let lookups = builder.add_default("-Dlog4j2.formatMsgNoLookups=", "true");
    let user_disabled_lookups = lookups.as_deref() == Some("-Dlog4j2.formatMsgNoLookups=false");
    if req.log4j.enabled && (!user_disabled_lookups || req.log4j.debug) {
        match &req.log4j.config_file {
            Some(path) => {
                let value = path.display().to_string();
                let _ = builder.add_default("-Dlog4j.configurationFile=", &value);
            }
            None => warnings.push(AssemblyWarning::Log4jConfigMissing),
        }
    }

    // 第 11 项：品牌/特性钩子注入的额外 JVM 参数。
    for arg in &req.extra_jvm {
        builder.add(arg);
    }

    // 第 12 项：主 jar、macOS dock、`-Duser.home`（仅非 Windows）。
    let primary_jar = req.dirs.primary_jar.display().to_string();
    let _ = builder.add_default("-Dminecraft.client.jar=", &primary_jar);
    if ctx.os == OsFamily::MacOs && !req.launcher.name.is_empty() {
        let name = req.launcher.name.clone();
        let _ = builder.add_default("-Xdock:name=", &name);
    }
    if ctx.os != OsFamily::Windows {
        if let Some(home) = &ctx.home_dir {
            let value = home.display().to_string();
            let _ = builder.add_default("-Duser.home=", &value);
        }
    }

    // 第 13 项：代理（用户已写任意代理参数则整体跳过，`analyse/02` §4.1 #13）。
    if let Some(proxy) = &req.proxy {
        if user_has_proxy_args(req) {
            warnings.push(AssemblyWarning::ProxyJvmArgsSkipped);
        } else {
            apply_proxy(builder, proxy);
        }
    }

    // 第 14 项：自动优化。
    if req.optimization.enabled {
        let args = defaults::optimization_args(
            java.major,
            java.bits,
            req.optimization.physical_memory_mb,
            user_specified_gc(req),
        );
        builder.add_all_default(&args);
    }

    // 第 15 项：版本特定开关。
    builder.add_all_default(&defaults::version_specific_args(java.major));

    // 第 16 项：FML 证书豁免。
    if req.mod_loader.is_fml_like() {
        builder.add_all_default(&defaults::fml_args());
    }

    // 第 17 项：渲染器。
    builder.add_all_default(&defaults::renderer_args(&req.renderer));

    (encoding, warnings)
}

fn apply_proxy(builder: &mut CommandBuilder, proxy: &crate::launch::request::SocksProxy) {
    let port = proxy.port.to_string();
    for key in ["http.proxyHost", "https.proxyHost", "socksProxyHost"] {
        let _ = builder.add_default(&format!("-D{key}="), &proxy.host);
    }
    for key in ["http.proxyPort", "https.proxyPort", "socksProxyPort"] {
        let _ = builder.add_default(&format!("-D{key}="), &port);
    }
    if let Some(username) = &proxy.username {
        for key in ["http.proxyUser", "https.proxyUser"] {
            let _ = builder.add_default(&format!("-D{key}="), username);
        }
    }
    if let Some(password) = &proxy.password {
        for key in ["http.proxyPassword", "https.proxyPassword"] {
            let _ = builder.add_default(&format!("-D{key}="), password);
        }
    }
}

fn user_has_proxy_args(req: &LaunchRequest) -> bool {
    req.user
        .override_jvm
        .iter()
        .chain(req.user.jvm.iter())
        .any(|arg| arg.starts_with("-D") && arg.to_ascii_lowercase().contains("proxy"))
}

fn user_specified_gc(req: &LaunchRequest) -> bool {
    req.user
        .override_jvm
        .iter()
        .chain(req.user.jvm.iter())
        .chain(
            req.manifest
                .jvm
                .iter()
                .flat_map(|entry| entry.values.iter()),
        )
        .any(|arg| {
            let lower = arg.to_ascii_lowercase();
            lower.starts_with("-xx:+use") || lower.starts_with("-xx:-use")
        })
}
