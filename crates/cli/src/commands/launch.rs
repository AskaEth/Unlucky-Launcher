//! `ull launch <instance> [--dry-run]`（D18：`--dry-run` 只打印组装后的命令行）。
//!
//! D20 的隔离档位在**这里**落成 `--gameDir`：本命令按覆盖链
//! （临时参数 > 实例 `isolation` > 全局 `instances.mode`）派生出运行目录，
//! 交给 [`snapshot::prepare`] 装进 `LaunchDirs.game_dir`（占位符 `${game_directory}`
//! 与子进程 CWD 都取这个值），并在启动时做一次切换检测（只探测、不迁移）。

use std::path::PathBuf;

use serde_json::json;
use unlucky_core::game::install::instance::{
    read_instance_config, write_instance_config, InstanceConfig, Isolation,
};
use unlucky_core::game::isolation::{derive_game_dir, observe_mode, IsolationMode};
use unlucky_core::game::ProfileList;
use unlucky_core::launch::time::{format_rfc3339_millis, now};
use unlucky_core::launch::{LaunchError, Launcher, LauncherConfig};
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;

use crate::cli::LaunchArgs;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::Report;

use super::identity::{self, Identity};
use super::snapshot::{self, PlanInput};

/// 执行 `launch`。
///
/// # Errors
///
/// 实例/版本不存在、版本 JSON 不可用、命令行组装失败、进程起不来时返回用户错误。
pub fn run(ctx: &Ctx, args: &LaunchArgs) -> Result<Report, CliError> {
    let profiles = super::instances::read_profiles(ctx)?;
    let game = ctx.game();
    let profile = profiles
        .profiles
        .iter()
        .find(|profile| profile.id.as_str() == args.instance || profile.name == args.instance);

    let (input, instance_found) = match profile {
        Some(profile) => (
            PlanInput {
                version_id: profile.version_id.clone(),
                instance_id: profile.id.as_str().to_string(),
                instance_name: profile.name.clone(),
                game_dir: profile.game_dir.clone(),
                java: args.java.clone().or_else(|| profile.java_path.clone()),
                username: args.username.clone(),
                max_memory: args.max_memory,
                ..PlanInput::default()
            },
            true,
        ),
        None => {
            // 也接受「直接给版本 id」：没有实例档案时按版本跑一次（脚本化常见）。
            if !game.is_version_installed(&args.instance) {
                return Err(CliError::user(
                    "not_found",
                    format!(
                        "既没有实例 `{}`，也没有版本 `{}`",
                        args.instance, args.instance
                    ),
                )
                .with_hint(instance_hint(&profiles)));
            }
            (
                PlanInput {
                    version_id: args.instance.clone(),
                    instance_name: args.instance.clone(),
                    java: args.java.clone(),
                    username: args.username.clone(),
                    max_memory: args.max_memory,
                    ..PlanInput::default()
                },
                false,
            )
        }
    };

    // 实例存在性先确认，再解析身份：微软轨解析身份会联网刷新令牌（有副作用），
    // 不该在一个注定 not_found 的启动上白跑一遍、更不该顺手改写凭据。
    let identity = identity::resolve(
        ctx,
        args.account.as_deref(),
        args.username.as_deref(),
        args.dry_run,
    )?;
    let input = PlanInput {
        auth: Some(identity.placeholders.clone()),
        ..input
    };
    let isolation = resolve_isolation(ctx, profile, &input, args.dry_run)?;
    let input = PlanInput {
        // 派生结果就是 `--gameDir` 的取值（覆盖链已求值完）。
        game_dir: Some(isolation.game_dir.clone()),
        ..input
    };

    // D13：实例覆盖 > 托管匹配 > 系统。托管命中时把路径钉死交给 `snapshot::prepare`。
    let java = super::java::resolve_for_launch(ctx, &input.version_id, input.java.as_deref());
    let input = PlanInput {
        java: java.override_path.clone(),
        ..input
    };

    let plan = snapshot::prepare(ctx, &input)?;
    let config = LauncherConfig::new(ctx.sessions_root())
        .with_launcher(super::meta::LAUNCHER_NAME, env!("CARGO_PKG_VERSION"));
    let launcher = Launcher::new(config);
    let mut human = Vec::new();
    human.push(format!(
        "隔离档位 {}（来源 {}）· 运行目录 {}",
        isolation.isolation.as_str(),
        isolation.source.as_str(),
        isolation.game_dir.display()
    ));
    if let Some(warning) = &isolation.warning {
        human.push(format!("提示: {warning}"));
    }
    human.push(identity_line(&identity));
    for note in &identity.notes {
        human.push(format!("提示: {note}"));
    }
    human.push(java.summary());
    for warning in java.warnings() {
        human.push(format!("告警: {warning}"));
    }

    if args.dry_run {
        let dry = launcher
            .dry_run(&plan.request)
            .map_err(|error| map_launch_error(&error, ctx))?;
        let mut lines = vec![dry.line().to_string()];
        lines.push(format!(
            "（--dry-run：未启动进程 · classpath {} 项 · 编码 {}）",
            dry.classpath_entries, dry.encoding
        ));
        if let Some(natives) = &dry.natives {
            lines.push(format!("natives：{}", natives.summary()));
        }
        let mut warnings = dry.warnings.clone();
        warnings.extend(java.warnings());
        // 人读行里 Java 告警已经在上面打过一次，这里只补 `dry_run` 自己新发现的。
        for warning in &dry.warnings {
            lines.push(format!("告警: {warning}"));
        }
        human.extend(lines);
        let natives = dry.natives.as_ref().map(|natives| {
            json!({
                "dir": natives.dir.display().to_string(),
                "files": natives.files,
                "skipped": natives.skipped,
                "ignored": natives.ignored,
                "jars": natives.jars,
                "custom": natives.custom,
                "up_to_date": natives.up_to_date,
                "era": natives.era.as_str(),
            })
        });
        let data = json!({
            "dry_run": true,
            "instance_found": instance_found,
            "version_id": plan.version_id,
            "isolation": isolation.isolation.as_str(),
            "isolation_source": isolation.source.as_str(),
            "game_dir": isolation.game_dir.display().to_string(),
            "mode_change_warning": isolation.warning,
            "account": account_json(&identity),
            "java": java.to_json(),
            "program": dry.program.display().to_string(),
            "argv": dry.argv,
            "display": dry.display,
            "classpath_entries": dry.classpath_entries,
            "encoding": dry.encoding,
            "warnings": warnings,
            "natives": natives,
            "adapter_warnings": plan.warnings,
        });
        return Ok(Report::new(data, human));
    }

    let handle = launcher
        .launch(&plan.request)
        .map_err(|error| map_launch_error(&error, ctx))?;
    let data = json!({
        "dry_run": false,
        "instance_found": instance_found,
        "isolation": isolation.isolation.as_str(),
        "isolation_source": isolation.source.as_str(),
        "game_dir": isolation.game_dir.display().to_string(),
        "mode_change_warning": isolation.warning,
        "account": account_json(&identity),
        "java": java.to_json(),
        "warnings": java.warnings(),
        "session_id": handle.session_id,
        "log_path": handle.log_path.display().to_string(),
        "exit_code": handle.exit.code,
        "exit_type": handle.exit.exit_type.as_str(),
        "uptime_ms": handle.uptime_ms,
        "summary": handle.exit.summary(),
        "artifacts": handle.artifacts.iter().map(|artifact| artifact.path.display().to_string()).collect::<Vec<_>>(),
        "adapter_warnings": plan.warnings,
    });
    human.push(format!(
        "会话 {} 结束：{}",
        handle.session_id,
        handle.exit.summary()
    ));
    human.push(format!("事件流: {}", handle.log_path.display()));
    Ok(Report::new(data, human))
}

/// 隔离派生的结果（运行目录 + 生效档位 + 提示）。
struct IsolationPlan {
    /// 生效运行目录（喂 `--gameDir`）。
    game_dir: PathBuf,
    /// 生效档位。
    isolation: Isolation,
    /// 档位来自哪一层（`temporary` / `instance` / `global`）。
    source: unlucky_core::game::IsolationSource,
    /// 切换检测提示（两侧都有内容时；`None` = 不必提示）。
    warning: Option<String>,
}

/// 按覆盖链派生运行目录，并在启动时做一次切换检测。
///
/// 覆盖链：临时参数（`ull launch` 暂无该 flag，留 `None`） > 实例 `isolation` > 全局 `instances.mode`。
/// 切换检测只探测 `mods/saves/config` 的条目数并写进实例文件的 `lastModeChange`，
/// **不迁移任何数据**（D20 定案）；提示只展示一次（`warned` 落盘）。
///
/// 遗留实例（`profiles.json` 有档案、`config/instances/<id>.json` 还没有——M27 之前的形态）
/// 会在**首次真启动**时补建基线文件：否则这批「最可能撞上存档消失」的用户永远拿不到提示。
/// `--dry-run` 仍然一个字节都不写。
fn resolve_isolation(
    ctx: &Ctx,
    profile: Option<&unlucky_core::game::InstanceProfile>,
    input: &PlanInput,
    dry_run: bool,
) -> Result<IsolationPlan, CliError> {
    let global_mode = global_mode(ctx);
    let mut config = match profile {
        Some(profile) => {
            read_instance_config(&ctx.layout(), profile.id.as_str()).map_err(|error| {
                CliError::user(
                    "instance_config",
                    format!("读实例设置 {} 失败：{error}", profile.id.as_str()),
                )
            })?
        }
        None => None,
    };
    // 补建基线：有档案没有设置文件 → 先按最小配置在内存里建一份（落盘看下面的 dirty）。
    let bootstrap = config.is_none() && profile.is_some();
    if bootstrap {
        config = Some(InstanceConfig::new(
            input.instance_id.clone(),
            input.version_id.clone(),
        ));
    }
    // 读兼容：M27 之前的档案只在 profiles.json 里存 `game_dir`；这种「显式指定目录」的历史语义
    // 等价于 `custom`，不要静默退回全局共享。补建基线时把这条映射**写进文件**，
    // 让实例文件成为唯一真相（profiles.json 的 `game_dir` 是遗留字段）。
    if let Some(config) = config.as_mut() {
        if config.isolation.is_none() {
            if let Some(dir) = profile
                .and_then(|profile| profile.game_dir.clone())
                .filter(|dir| !dir.as_os_str().is_empty())
            {
                config.isolation = Some(Isolation::Custom);
                config.game_dir = Some(dir);
            }
        }
    }

    let derivation = derive_game_dir(
        &ctx.game(),
        &input.version_id,
        global_mode,
        config.as_ref(),
        None,
    )
    .map_err(|error| CliError::user("isolation", error.to_string()))?;

    // 切换检测：生效档位与记录里的 `to` 不同 → 记一次（探测双侧，只记事实）。
    let at = format_rfc3339_millis(now());
    let mut dirty = false;
    let warning = match config.as_mut() {
        Some(config) => {
            let changed = observe_mode(config, &ctx.game(), &at, derivation.isolation)
                .map_err(|error| CliError::user("isolation_probe", error.to_string()))?;
            let warning = config.take_mode_change_warning();
            dirty = bootstrap || changed || warning.is_some();
            warning.map(|warning| warning.message())
        }
        None => None,
    };
    // `--dry-run` 不落盘（否则「只看一眼」也会吃掉一次性提示、改写实例状态）。
    if !dry_run && dirty {
        if let Some(config) = config.as_ref() {
            write_instance_config(&ctx.layout(), config).map_err(|error| {
                CliError::user("instance_config", format!("写实例设置失败：{error}"))
            })?;
        }
    }

    Ok(IsolationPlan {
        game_dir: derivation.game_dir,
        isolation: derivation.isolation,
        source: derivation.source,
        warning,
    })
}

/// 全局默认隔离模式（`launcher.json` 的 `instances.mode`，缺键/坏值一律按 D20 默认 `isolated`）。
fn global_mode(ctx: &Ctx) -> IsolationMode {
    let store = SettingStore::load(ctx.launcher_config(), launcher_schema());
    store
        .doc()
        .effective_str("instances.mode")
        .and_then(IsolationMode::parse)
        .unwrap_or_default()
}

fn map_launch_error(error: &LaunchError, ctx: &Ctx) -> CliError {
    let hint = format!("版本本体目录：{}", ctx.game().versions_dir().display());
    CliError::user("launch", error.to_string()).with_hint(hint)
}

fn instance_hint(profiles: &ProfileList) -> String {
    if profiles.profiles.is_empty() {
        "用 `ull instances list` 查看实例，或 `ull install <version>` 登记一个".to_string()
    } else {
        let names: Vec<&str> = profiles
            .profiles
            .iter()
            .map(|profile| profile.name.as_str())
            .collect();
        format!("可用实例：{}", names.join("、"))
    }
}

/// 人读一行：本次用谁的身份启动。
fn identity_line(identity: &Identity) -> String {
    match &identity.account {
        Some(meta) => format!(
            "账户 {}（{}）· UUID {}",
            meta.display_name(),
            meta.kind.label(),
            identity.placeholders.uuid
        ),
        None => format!(
            "账户 离线占位 {}（未登记账户）",
            identity.placeholders.player_name
        ),
    }
}

/// `--json` 里的身份块（**只有可公开字段**，token 永远不出现）。
fn account_json(identity: &Identity) -> serde_json::Value {
    match &identity.account {
        Some(meta) => json!({
            "name": meta.display_name(),
            "kind": meta.kind.as_str(),
            "profile_id": identity.placeholders.uuid,
            "account_id": meta.id.as_str(),
            "registered": true,
        }),
        None => json!({
            "name": identity.placeholders.player_name,
            "kind": "offline-placeholder",
            "profile_id": identity.placeholders.uuid,
            "registered": false,
        }),
    }
}
