//! 参数后段：清单 JVM 参数尾部（第 20~23 项）与游戏参数（第 24~32 项）。
//!
//! 顺序本身有语义（`analyse/02` §4.1）：清单 JVM 参数在用户 `javaArguments` 之后
//! **无条件追加**，因此能覆盖用户参数，但覆盖不了用户的 override 参数；
//! 用户自定义游戏参数永远最后追加，是可以覆盖前面一切游戏参数的“逃生门”。

use crate::launch::args_table::AssemblyWarning;
use crate::launch::argument::{ArgumentEntry, RuleContext};
use crate::launch::command_builder::{tokenize, CommandBuilder};
use crate::launch::defaults;
use crate::launch::game_version::GameVersion;
use crate::launch::platform::PlatformCtx;
use crate::launch::request::{LaunchRequest, QuickPlay};

/// `--graphicsBackend` 的最低版本（实测首发：`26.2-snapshot-2`，2026-04-09）。
pub const GRAPHICS_BACKEND_MIN_VERSION: &str = "26.2-snapshot-2";

/// `--graphicsBackend` 能否下发给这个版本。
///
/// 两条判据**不要合并**（`research/05-mc-vulkan.md` §3.2/§4.2 的实测结论）：
///
/// - **可解析版本**走精确门限 `26.2-snapshot-2`。`26.2-snapshot-1` 已经有 `lwjgl-vulkan`
///   库但 `Main.class` 还没声明这个参数，用能力位放行会让它 `UnrecognizedOptionException`
///   直接起不来——库早了两天。
/// - **周快照**（`26w14a` 形态）无法与 `26.2-snapshot-N` 排序，只能信版本库表：
///   带 `lwjgl-vulkan` 就放行（26.1 全线含周快照都没有该库，所以这个推断安全）。
pub fn graphics_backend_allowed(version: &GameVersion, has_vulkan_backend: bool) -> bool {
    if version.is_known() {
        return version.is_at_least(GRAPHICS_BACKEND_MIN_VERSION);
    }
    version.is_weekly_snapshot() && has_vulkan_backend
}

/// 应用第 20~32 项，返回告警。
pub(crate) fn apply_game_arguments(
    req: &LaunchRequest,
    ctx: &PlatformCtx,
    builder: &mut CommandBuilder,
) -> Vec<AssemblyWarning> {
    let mut warnings = Vec::new();
    let rule_ctx = RuleContext::new(
        ctx.os.manifest_name(),
        &ctx.arch,
        &ctx.os_version,
        &req.features,
    );

    // 第 20 项：清单 JVM 参数（现代）或 legacy 默认 JVM 参数。
    if req.manifest.jvm.is_empty() {
        builder.add_all_default(&defaults::legacy_jvm_arguments());
    } else {
        let values = collect_applicable(&req.manifest.jvm, &rule_ctx);
        builder.add_all(&values);
    }

    // 第 21 项：登录源提供的 JVM 参数（authlib-injector 等）。
    for arg in &req.auth_jvm_args {
        builder.add(arg);
    }

    // 第 22 项：`-javaagent:` × N。
    for agent in &req.java_agents {
        builder.add(&format!("-javaagent:{agent}"));
    }

    // 第 23 项：主类（args_table 已保证非空）。
    if let Some(main_class) = &req.manifest.main_class {
        builder.add(main_class);
    }

    // 第 24 项：legacy `minecraftArguments` 整串分词。
    if let Some(legacy) = &req.manifest.legacy_arguments {
        for token in tokenize(legacy) {
            builder.add(&token);
        }
    }

    // 第 25 项：现代 `arguments.game`（带 rules/features）。
    if !req.manifest.game.is_empty() {
        let values = collect_applicable(&req.manifest.game, &rule_ctx);
        builder.add_all(&values);
    }

    // 第 26 项：legacy 默认游戏参数（分辨率）。
    if req.manifest.legacy_arguments.is_some() {
        builder.add_all(&defaults::legacy_game_args(req.resolution.as_ref()));
    }

    // 第 27 项：登录源提供的游戏参数。
    for arg in &req.auth_game_args {
        builder.add(arg);
    }

    // 第 28 项：QuickPlay / `--server --port`。
    apply_quick_play(req, builder, &mut warnings);

    // 第 29 项：全屏。
    if req.fullscreen {
        builder.add("--fullscreen");
    }

    // 第 30 项：SOCKS 代理的游戏侧参数。
    if let Some(proxy) = &req.proxy {
        builder.add("--proxyHost");
        builder.add(&proxy.host);
        builder.add("--proxyPort");
        builder.add(&proxy.port.to_string());
        if let Some(username) = &proxy.username {
            builder.add("--proxyUser");
            builder.add(username);
        }
        if let Some(password) = &proxy.password {
            builder.add("--proxyPass");
            builder.add(password);
        }
    }

    // 第 31 项：图形后端（精确门限 ≥26.2-snapshot-2，周快照走库表兜底）。
    if let Some(backend) = &req.graphics_backend {
        if graphics_backend_allowed(&req.instance.version, req.manifest.has_vulkan_backend) {
            builder.add("--graphicsBackend");
            builder.add(backend);
        } else {
            warnings.push(AssemblyWarning::GraphicsBackendSkipped {
                version: req.instance.version.raw().to_string(),
            });
        }
    }

    // 第 32 项：用户自定义游戏参数（最后追加，可覆盖一切）。
    builder.add_all(&req.user.game);

    warnings
}

fn apply_quick_play(
    req: &LaunchRequest,
    builder: &mut CommandBuilder,
    warnings: &mut Vec<AssemblyWarning>,
) {
    let Some(quick_play) = &req.quick_play else {
        return;
    };
    let supports = req.instance.version.is_at_least("1.20");
    match quick_play {
        QuickPlay::Singleplayer { world } if supports => {
            builder.add("--quickPlaySingleplayer");
            builder.add(world);
        }
        QuickPlay::Multiplayer { address } if supports => {
            builder.add("--quickPlayMultiplayer");
            builder.add(address);
        }
        QuickPlay::Realms { realm_id } if supports => {
            builder.add("--quickPlayRealms");
            builder.add(realm_id);
        }
        QuickPlay::Multiplayer { address } => match parse_server_address(address) {
            Some((host, port)) => {
                builder.add("--server");
                builder.add(&host);
                if let Some(port) = port {
                    builder.add("--port");
                    builder.add(&port.to_string());
                }
                warnings.push(AssemblyWarning::QuickPlayDegraded {
                    address: address.clone(),
                });
            }
            None => warnings.push(AssemblyWarning::QuickPlayAddressInvalid {
                address: address.clone(),
            }),
        },
        QuickPlay::Singleplayer { .. } | QuickPlay::Realms { .. } => {
            warnings.push(AssemblyWarning::QuickPlayUnsupported {
                version: req.instance.version.raw().to_string(),
            });
        }
    }
}

/// 解析 `host[:port]`；端口非法或为空时该段视为不存在。
pub fn parse_server_address(address: &str) -> Option<(String, Option<u16>)> {
    let trimmed = address.trim();
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() => match port.parse::<u16>() {
            Ok(port) if port > 0 => Some((host.to_string(), Some(port))),
            _ => None,
        },
        Some(_) => None,
        None => Some((trimmed.to_string(), None)),
    }
}

fn collect_applicable(entries: &[ArgumentEntry], ctx: &RuleContext<'_>) -> Vec<String> {
    let mut values = Vec::new();
    for entry in entries {
        if entry.applies(ctx) {
            values.extend(entry.values.iter().cloned());
        }
    }
    values
}
