//! 本地 Mod 列表与启停的宿主实现（M224 · MODTOGGLE）。
//!
//! ## 三条纪律
//!
//! | 纪律 | 落点 |
//! |---|---|
//! | 磁盘只在后台线程碰 | [`spawn_list`] / [`spawn_set_enabled`] 走 `job::spawn`，UI 只收快照 |
//! | 单根语义（D20，M274 起按**实例自己的根**） | 运行目录来自 `game::isolation::derive_game_dir`——**与启动同一个答案**，不另派生；`super::game::game_for` 先定根，再把那个根喂给派生 |
//! | 游戏在跑就不改名 | `running` 标记在 host 这一侧挡：Windows 上进程正持有 jar，rename 会失败，与其让用户读一句 OS 报错不如直说 |
//!
//! ## 为什么不复用启动路径的 `run_dir()`
//!
//! `host::launch` 的 `run_dir()` 顺带把「这次用哪个档位」写进实例设置（`observe_mode`）。列 mod /
//! 启停**不该因为用户点开详情页就写盘**，所以这里只取派生结果、不落任何设置。
//!
//! ## 失败口径
//!
//! 一律走 [`crate::dto::ModFailureView`]（结构 + OS 原文），**不拼中文**：文案由 app 层按 i18n key
//! 现求（M161 的纪律，`dto::events` 的 `LaunchFinished` 同款）。

use std::path::PathBuf;

use unlucky_core::addon::mods::{
    inspect, ModDuplicate, ModError, ModInspection, ModPrerequisite, ModScan, SkipReason,
};
use unlucky_core::game::install::instance::read_instance_config;
use unlucky_core::game::isolation::derive_game_dir;

use crate::dto::{
    locate, InstanceId, InstanceSummary, ModCopyView, ModDuplicateView, ModEntryView,
    ModFailureKind, ModFailureView, ModListView, ModPrerequisiteView, ModProblemView, ModSkipKind,
    ModSkipView, ModsCommand,
};

use super::env::HostEnv;
use super::job::{spawn, JobSender};

/// 后台线程回报的一次结果（`HostTask::Mods` 的载荷）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// 实例 id（迟到结果靠它判「这条还是不是当前实例的」）。
    pub instance: InstanceId,
    /// 列表结果：成功 = 整份快照；失败 = 具名原因。
    pub list: Result<ModListView, ModFailureView>,
    /// 这一拍是启停操作的结果时带上「改了什么」（host 只记日志；开关跟快照翻）。
    pub toggled: Option<Toggled>,
}

/// 一次成功的启停（`renamed == false` ⇒ 本来就是那个状态，什么也没动）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toggled {
    /// 原磁盘名。
    pub from: String,
    /// 新磁盘名。
    pub to: String,
    /// 现在的启用状态。
    pub enabled: bool,
    /// 是否真的改了名。
    pub renamed: bool,
}

/// 后台列一个实例的 `mods/`。
pub fn spawn_list(env: HostEnv, tx: JobSender, id: InstanceId, running: bool) {
    spawn("mods-list", move || {
        let report = Report {
            instance: id.clone(),
            list: list(&env, &id, running),
            toggled: None,
        };
        let _ = tx.send(super::HostTask::Mods(Box::new(report)));
    });
}

/// 后台启停一个文件，然后把**新快照**一起带回来（app 不必再请求一次）。
pub fn spawn_set_enabled(
    env: HostEnv,
    tx: JobSender,
    id: InstanceId,
    file: String,
    enabled: bool,
    running: bool,
) {
    spawn("mods-toggle", move || {
        let report = toggle(&env, &id, &file, enabled, running);
        let _ = tx.send(super::HostTask::Mods(Box::new(report)));
    });
}

/// 一条指令 → 后台线程（`CoreHost::dispatch` 的唯一入口）。
pub fn dispatch(env: HostEnv, tx: JobSender, command: ModsCommand, running: bool) {
    match command {
        ModsCommand::List(id) => spawn_list(env, tx, id, running),
        ModsCommand::SetEnabled { id, file, enabled } => {
            spawn_set_enabled(env, tx, id, file, enabled, running)
        }
    }
}

/// 实例 + 它的运行目录（找不到实例 / 派生不出目录都给具名失败）。
fn target(
    env: &HostEnv,
    id: &str,
    file: &str,
) -> Result<(InstanceSummary, PathBuf), ModFailureView> {
    let instances = super::game::instances(env);
    let Some(instance) = locate(&instances, id) else {
        return Err(failure(
            id,
            file,
            ModFailureKind::InstanceUnavailable,
            format!("实例 {id} 不在快照里（可能刚被删除）"),
        ));
    };
    let dir = run_dir(env, instance).map_err(|detail| {
        failure(
            &instance.id,
            file,
            ModFailureKind::InstanceUnavailable,
            detail,
        )
    })?;
    Ok((instance.clone(), dir))
}

/// 列目录（后台线程里调）。
fn list(env: &HostEnv, id: &str, running: bool) -> Result<ModListView, ModFailureView> {
    let (instance, dir) = target(env, id, "")?;
    let scan = unlucky_core::addon::mods::scan(&dir).map_err(|error| {
        let (kind, detail) = classify(&error);
        failure(&instance.id, "", kind, detail)
    })?;
    Ok(view(&instance, &scan, running))
}

/// 启停一个文件 + 回来时带新快照。
fn toggle(env: &HostEnv, id: &str, file: &str, enabled: bool, running: bool) -> Report {
    let (instance, dir) = match target(env, id, file) {
        Ok(found) => found,
        Err(view) => {
            return Report {
                instance: id.to_string(),
                list: Err(view),
                toggled: None,
            };
        }
    };
    let instance_id = instance.id.clone();
    if running {
        return Report {
            instance: instance_id,
            list: Err(failure(
                id,
                file,
                ModFailureKind::Running,
                "游戏进程正在跑（Windows 会拒绝改名）",
            )),
            toggled: None,
        };
    }
    let toggled = match unlucky_core::addon::mods::set_enabled(&dir, file, enabled) {
        Ok(result) => Toggled {
            from: result.from,
            to: result.to,
            enabled: result.enabled,
            renamed: result.renamed,
        },
        Err(error) => {
            let (kind, detail) = classify(&error);
            return Report {
                instance: instance_id,
                list: Err(failure(id, file, kind, detail)),
                toggled: None,
            };
        }
    };
    // 改名之后**重新扫一遍**再发：权威状态来自磁盘，app 不做乐观回显（与启动/停止同一条纪律）。
    let list = unlucky_core::addon::mods::scan(&dir)
        .map(|scan| view(&instance, &scan, running))
        .map_err(|error| {
            let (kind, detail) = classify(&error);
            failure(&instance_id, file, kind, detail)
        });
    Report {
        instance: instance_id,
        list,
        toggled: Some(toggled),
    }
}

/// 实例的运行目录（D20 覆盖链 + 自动发现的基底档，与启动同一份派生；**不写盘**）。
fn run_dir(env: &HostEnv, instance: &InstanceSummary) -> Result<PathBuf, String> {
    // **这个实例自己那个根**（M274）：共用目录的运行目录是这个根的根目录；
    // 拿 `env.game` 去派生，官方根里的实例会读到 ULL 根下的存档 / 模组（用户的原话：串了）。
    let game = super::game::game_for(env, &instance.id);
    let base = super::game::base_mode(env, &game, &instance.id, &instance.version_id);
    let config = read_instance_config(&env.layout, &instance.id)
        .map_err(|error| format!("读实例设置失败：{error}"))?;
    let derivation = derive_game_dir(&game, &instance.version_id, base, config.as_ref(), None)
        .map_err(|error| format!("派生运行目录失败：{error}"))?;
    Ok(derivation.game_dir)
}

/// 快照映射：core 的扫描结果 → DTO（路径串在 host 这一侧才成形）。
///
/// M312 起这一跳还负责**读 jar 元数据**（[`inspect`]）并把重复 / 前置的报告一起塞进快照：
/// 页面不需要第二个命令、也不该自己读盘。它跑在后台线程（`spawn`）里，只读元数据文件
/// （200 个 mod 的实测量级见 M312 报告）。
fn view(instance: &InstanceSummary, scan: &ModScan, running: bool) -> ModListView {
    let inspection = inspect(&scan.dir, &scan.entries);
    ModListView {
        instance: instance.id.clone(),
        dir: scan.dir.display().to_string(),
        exists: scan.exists,
        entries: scan
            .entries
            .iter()
            .map(|entry| ModEntryView {
                file: entry.file.clone(),
                name: entry.name.clone(),
                enabled: entry.enabled,
                bytes: entry.bytes,
                old: entry.variant.is_old(),
            })
            .collect(),
        skipped: scan
            .skipped
            .iter()
            .map(|skip| ModSkipView {
                file: skip.file.clone(),
                reason: match skip.reason {
                    SkipReason::SymbolicLink => ModSkipKind::SymbolicLink,
                    SkipReason::NotAFile => ModSkipKind::NotAFile,
                    SkipReason::UnsafeName => ModSkipKind::UnsafeName,
                    SkipReason::Unreadable => ModSkipKind::Unreadable,
                },
            })
            .collect(),
        running,
        known: true,
        problems: problems(&inspection),
    }
}

/// core 的报告 → DTO（**只搬结构，不拼句子**：文案由 app 层按 i18n key 现求）。
fn problems(inspection: &ModInspection) -> ModProblemView {
    let report = &inspection.report;
    ModProblemView {
        duplicates: report.duplicates.iter().map(duplicate).collect(),
        missing: report.missing.iter().map(prerequisite).collect(),
        unverifiable: report.unverifiable.iter().map(prerequisite).collect(),
        optional: report.optional.iter().map(prerequisite).collect(),
        opaque: inspection.opaque,
    }
}

fn duplicate(group: &ModDuplicate) -> ModDuplicateView {
    ModDuplicateView {
        id: group.id.clone(),
        display: group.display.clone(),
        keep: group.keep.clone(),
        keep_version: group.keep_version.clone(),
        copies: group
            .copies
            .iter()
            .map(|copy| ModCopyView {
                file: copy.file.clone(),
                version: copy.version.clone(),
                enabled: copy.enabled,
            })
            .collect(),
    }
}

fn prerequisite(item: &ModPrerequisite) -> ModPrerequisiteView {
    ModPrerequisiteView {
        id: item.id.clone(),
        range: item.range.clone(),
        installed_version: item.installed_version.clone(),
        required_by: item.required_by.clone(),
    }
}

fn failure(
    instance: &str,
    file: &str,
    kind: ModFailureKind,
    detail: impl Into<String>,
) -> ModFailureView {
    ModFailureView {
        instance: instance.to_string(),
        file: file.to_string(),
        kind,
        detail: detail.into(),
    }
}

/// core 的错误 → DTO 大类 + 原文（**不翻译**，翻译在 app 层）。
fn classify(error: &ModError) -> (ModFailureKind, String) {
    match error {
        ModError::UnsafeName { detail, .. } => (ModFailureKind::UnsafeName, detail.clone()),
        ModError::ModsDirUnusable { detail, .. } => {
            (ModFailureKind::ModsDirUnusable, detail.clone())
        }
        ModError::NotFound { .. } => (ModFailureKind::NotFound, error.to_string()),
        ModError::NotARegularFile { detail, .. } => {
            (ModFailureKind::NotARegularFile, detail.clone())
        }
        ModError::Collision { .. } => (ModFailureKind::Collision, error.to_string()),
        ModError::Rename { detail, .. } => (ModFailureKind::Rename, detail.clone()),
    }
}

#[cfg(test)]
mod tests;
