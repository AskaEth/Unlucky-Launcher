//! `ull launch` 的隔离档位派生（D20）：运行目录 + 切换检测。
//!
//! 从 `commands/launch.rs` 原样搬出（那边贴着 400 行红线，M220 的 Java 钉路径改动要往里加行）。
//! **搬动是纯挪**：函数体、注释、判定顺序一字未改，只补了本模块的 import 与 `pub(super)`
//! （`launch.rs` 里的 `#[path = "launch_isolation.rs"] mod launch_isolation;` 挂载）。

use std::path::PathBuf;

use unlucky_core::game::install::instance::{
    read_instance_config, write_instance_config, InstanceConfig, Isolation,
};
use unlucky_core::game::isolation::{derive_game_dir, observe_mode, IsolationMode};
use unlucky_core::launch::time::{format_rfc3339_millis, now};
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;

use crate::context::Ctx;
use crate::error::CliError;

use super::snapshot::PlanInput;

/// 隔离派生的结果（运行目录 + 生效档位 + 提示）。
pub(super) struct IsolationPlan {
    /// 生效运行目录（喂 `--gameDir`）。
    pub(super) game_dir: PathBuf,
    /// 生效档位。
    pub(super) isolation: Isolation,
    /// 档位来自哪一层（`temporary` / `instance` / `global`）。
    pub(super) source: unlucky_core::game::IsolationSource,
    /// 切换检测提示（两侧都有内容时；`None` = 不必提示）。
    pub(super) warning: Option<String>,
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
pub(super) fn resolve_isolation(
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
