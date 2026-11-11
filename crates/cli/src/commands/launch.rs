//! `ull launch <instance> [--dry-run]`（D18：`--dry-run` 只打印组装后的命令行）。
//!
//! D20 的隔离档位在**这里**落成 `--gameDir`：本命令按覆盖链
//! （临时参数 > 实例 `isolation` > 全局 `instances.mode`）派生出运行目录，
//! 交给 [`snapshot::prepare`] 装进 `LaunchDirs.game_dir`（占位符 `${game_directory}`
//! 与子进程 CWD 都取这个值），并在启动时做一次切换检测（只探测、不迁移）。

use serde_json::json;
use unlucky_core::game::java::select;
use unlucky_core::game::ProfileList;
use unlucky_core::launch::{LaunchError, Launcher, LauncherConfig};

use crate::cli::LaunchArgs;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::Report;

use super::identity::{self, Identity};
use super::snapshot::{self, PlanInput};

// 隔离档位（D20）的派生与切换检测搬到独立模块：本文件贴着 400 行红线（M220）。
// 用 `#[path]` 而不是在 `commands/mod.rs` 里登记，是为了这次改动只碰本文件 + 新文件。
#[path = "launch_isolation.rs"]
mod launch_isolation;

use launch_isolation::resolve_isolation;

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

    // D13 选 Java：命中就把路径钉死交给 `snapshot::prepare`（五个来源一个口径，M220）。
    // 顺序是有意的（M220 / nit3）：D4 的拦截排在身份解析**之前**——解析身份会联网刷新微软轨
    // 令牌（有副作用，还会顺手改写凭据），而 D4 判下来必被拦的启动根本用不到身份。
    // 这一步只看版本 id 与「谁指定了 Java」，不依赖身份解析的结果，所以拆得开。
    //
    // 「谁指定了 Java」必须连实例设置/全局设置里的 `javaPath` 一起算：装配期
    // （`snapshot::prepare` 的 `effective.java_path`）用的就是那一份，D4 校验的却只认
    // `--java` / 档案 `javaPath` 的话，用户配过的 JDK 会被自动挑的结果顶掉。
    let configured_java = super::instance_launch::resolved(ctx, &input.instance_id).java_path;
    let chosen_java = input.java.clone().or(configured_java);
    unlucky_core::launch::launcher_log::set_dir(ctx.logs_dir());
    let explicit_java = chosen_java.is_some();
    let java = super::java::resolve_for_launch(ctx, &input.version_id, chosen_java.as_deref());
    if !args.dry_run {
        if let Some(mismatch) = select::decide(java.mismatch, explicit_java).block() {
            unlucky_core::launch::launcher_log::record("launch", &mismatch.message());
            // 提示里补上「挑中的是哪个 java」（M220）：不符时用户第一个问题就是「它到底用了我哪个
            // JDK」，以前只能自己猜；这条文案只加不减。
            return Err(
                CliError::user("java_mismatch", mismatch.message()).with_hint(format!(
                    "{}；挑中的是 {}",
                    mismatch.advice(),
                    java.summary()
                )),
            );
        }
    }

    // 实例存在性先确认、Java 先判，再解析身份：微软轨解析身份会联网刷新令牌（有副作用），
    // 不该在一个注定 not_found 或注定被 D4 拦下的启动上白跑一遍、更不该顺手改写凭据。
    let identity = identity::resolve(
        ctx,
        args.account.as_deref(),
        args.username.as_deref(),
        args.dry_run,
    )?;
    let input = PlanInput {
        auth: Some(identity.placeholders.clone()),
        // `override_path` 已经是 D4 校验过的那个 java，装配期只照用、不再挑。
        java: java.override_path.clone(),
        ..input
    };
    let isolation = resolve_isolation(ctx, profile, &input, args.dry_run)?;
    let input = PlanInput {
        // 派生结果就是 `--gameDir` 的取值（覆盖链已求值完）。
        game_dir: Some(isolation.game_dir.clone()),
        ..input
    };

    // M188/M211：命令行这一路也把启动器日志写进 `<data>/logs/launcher.log`
    // （落点与格式的真相都在 core 的 `launcher_log`，GUI 写的是同一份）。
    unlucky_core::launch::launcher_log::record_start(
        &input.instance_id,
        &isolation.game_dir,
        java.runtime
            .as_ref()
            .map(|runtime| (runtime.binary.as_path(), runtime.major.unwrap_or_default())),
        input.max_memory,
    );
    let plan = snapshot::prepare(ctx, &input)?;
    let config = LauncherConfig::new(ctx.sessions_root())
        .with_launcher(super::meta::LAUNCHER_NAME, env!("CARGO_PKG_VERSION"))
        .with_watchdog_env();
    // D6：两个看门狗时长的**生效值**落 launcher.log —— 卡死之后第一个要看的就是「等了多久才放弃」。
    let watchdog = config.watchdog_summary();
    unlucky_core::launch::launcher_log::record("launch", &watchdog);
    let launcher = Launcher::new(config);
    let mut human = Vec::new();
    human.push(watchdog);
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
    for warning in java.warnings(&plan.request.java.binary) {
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
        warnings.extend(java.warnings(&plan.request.java.binary));
        // 人读行里 Java 告警已经在上面打过一次，这里只补 `dry_run` 自己新发现的。
        for warning in &dry.warnings {
            lines.push(format!("告警: {warning}"));
        }
        human.extend(lines);
        let natives = dry.natives.as_ref().map(|natives| natives.to_json());
        let data = json!({
            "dry_run": true,
            "instance_found": instance_found,
            "version_id": plan.version_id,
            "isolation": isolation.isolation.as_str(),
            "isolation_source": isolation.source.as_str(),
            "game_dir": isolation.game_dir.display().to_string(),
            "mode_change_warning": isolation.warning,
            "account": account_json(&identity),
            "java": java.to_json(&plan.request.java.binary),
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
    unlucky_core::launch::launcher_log::record_end(&input.instance_id, &handle);
    let data = json!({
        "dry_run": false,
        "instance_found": instance_found,
        "isolation": isolation.isolation.as_str(),
        "isolation_source": isolation.source.as_str(),
        "game_dir": isolation.game_dir.display().to_string(),
        "mode_change_warning": isolation.warning,
        "account": account_json(&identity),
        "java": java.to_json(&plan.request.java.binary),
        "warnings": java.warnings(&plan.request.java.binary),
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

fn map_launch_error(error: &LaunchError, ctx: &Ctx) -> CliError {
    // D6：看门狗超时是一个**可区分的**结局（脚本要能单独处理），且自带现场与可调项，
    // 不该被塞上「版本本体目录」这种无关提示。
    if let LaunchError::Watchdog { .. } = error {
        return CliError::user("launch_timeout", error.to_string());
    }
    let hint = format!("版本本体目录：{}", ctx.game().versions_dir().display());
    CliError::user("launch", error.to_string()).with_hint(hint)
}

fn instance_hint(profiles: &ProfileList) -> String {
    if profiles.profiles.is_empty() {
        "用 `%prog% instances list` 查看实例，或 `%prog% install <version>` 登记一个".to_string()
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
