//! 实例快照：`profiles.json` + 版本 JSON + 会话索引 + 实例设置 → [`InstanceSummary`]。
//!
//! 「磁贴上的每个字」都在这里翻译完：版本号取自继承链根、加载器取自版本 id（解析在
//! [`unlucky_core::game::loader`]，本文件只做 DTO 翻译）、最后游玩时间取自会话索引、
//! 隔离档位取自实例设置（缺省跟全局 `instances.mode`）。

use std::collections::BTreeMap;
use std::path::Path;

use unlucky_core::game::install::instance::{
    read_instance_config, read_profiles, write_instance_config,
};
use unlucky_core::game::isolation::IsolationMode;
use unlucky_core::game::{ProfileList, VersionJson};

use crate::dto::{
    InstanceRuntime, InstanceSummary, LoaderInfo, LoaderKind, NoticeLevel, NoticeView,
};

use super::env::HostEnv;

/// 全局默认隔离档位（`instances.mode`；缺键/坏值按 D20 默认 `isolated`）。
pub fn global_mode(env: &HostEnv) -> IsolationMode {
    env.store()
        .doc()
        .effective_str("instances.mode")
        .and_then(IsolationMode::parse)
        .unwrap_or_default()
}

/// 读实例清单（文件缺失 = 空列表，不是错误）。
pub fn profiles(env: &HostEnv) -> ProfileList {
    read_profiles(&env.layout).unwrap_or_default()
}

/// 全会话索引里「每个实例最近一次启动」的时间串（`logs/` + 旧家合并）。
pub fn last_played(env: &HostEnv) -> BTreeMap<String, String> {
    let entries = super::logs::entries(env);
    let mut map: BTreeMap<String, String> = BTreeMap::new();
    for entry in entries {
        if entry.instance_id.is_empty() || entry.started_at.is_empty() {
            continue;
        }
        let current = map.get(&entry.instance_id);
        if current.is_none_or(|existing| existing.as_str() < entry.started_at.as_str()) {
            map.insert(entry.instance_id.clone(), entry.started_at);
        }
    }
    map
}

/// 版本 JSON 的继承链根 id + 加载器（读不到返回 `(version_id, 原版)`）。
pub fn describe_version(env: &HostEnv, version_id: &str) -> (String, LoaderInfo) {
    let loader = loader_from_id(version_id);
    let mut current = version_id.to_string();
    let mut seen = 0;
    while seen < 16 {
        seen += 1;
        let path = env.game.version_json(&current);
        let Ok(text) = std::fs::read_to_string(&path) else {
            break;
        };
        let Ok(json) = VersionJson::from_json_str(&text) else {
            break;
        };
        match json.inherits_from {
            Some(parent) if !parent.is_empty() => current = parent,
            _ => break,
        }
    }
    (current, loader)
}

/// 版本 id → 加载器。
///
/// 解析本身**不在这里**：真相源是 [`unlucky_core::game::loader`]（生成端
/// `addon::loader::LoaderKind::version_id` 拼的是 `<mc>-<kind>-<loader>`，如
/// `1.20.4-fabric-0.16.5`）。本函数只做两件事：调 core 的解析、把 core 的种类翻成 DTO
/// 种类（`dto` 里不许出现 core 类型，翻译只能发生在 host 这条边界上）。
pub fn loader_from_id(version_id: &str) -> LoaderInfo {
    let parsed = unlucky_core::game::InstanceLoader::parse(version_id);
    LoaderInfo {
        kind: dto_kind(parsed.kind),
        version: parsed.version,
    }
}

/// core 的加载器种类 → DTO 种类。
///
/// **这里没有映射表**：种类由 core 的稳定 id 反查 DTO 词表得来（[`LoaderKind::parse`] →
/// `dto::LoaderKind::from_core`，全仓唯一那份 core↔DTO 映射）。M134 之前这里是七支手写 match，
/// M126 r3 复审实测过：core 加第八家、契约与 help 都补齐、**只在这一支写 `=> Vanilla`**，三个
/// crate 全绿（402 / 220 / 42）而磁贴静默显示「原版」——M122 修掉的用户可见 bug 复活。委派之后
/// **没有落笔处**：新种类必须先过 `dto::LoaderKind::from_core`（那边编不过就说明 DTO 还没决定怎么
/// 显示它），糊错的后果由 `dto::instances` 的 `loader_kind_ids_are_the_released_contract` 与本
/// 文件的 `every_core_loader_kind_reaches_the_tile_as_itself` 钉住。反查不到只能是 core 的
/// `as_str` 与 `parse` 自相矛盾（同一张宏表长出来，物理上做不到），真发生了就 panic：**不许**
/// 退回「原版」。
fn dto_kind(kind: unlucky_core::game::LoaderKind) -> LoaderKind {
    LoaderKind::parse(kind.as_str()).unwrap_or_else(|| {
        unreachable!(
            "core 的稳定 id `{}` 反查不到 DTO 种类：词表两边脱节了",
            kind.as_str()
        )
    })
}

/// 实例的隔离档位（实例文件优先，缺省显示全局档位）。
pub fn isolation_of(env: &HostEnv, id: &str) -> String {
    let global = global_mode(env);
    match read_instance_config(&env.layout, id) {
        Ok(Some(config)) => config.effective_isolation(global).as_str().to_string(),
        _ => global.as_str().to_string(),
    }
}

/// 实例快照。
pub fn instances(env: &HostEnv) -> Vec<InstanceSummary> {
    let profiles = profiles(env);
    let played = last_played(env);
    profiles
        .profiles
        .iter()
        .map(|profile| {
            let version_id = profile.version_id.clone();
            let (mc_version, loader) = describe_version(env, &version_id);
            let id = profile.id.as_str();
            InstanceSummary {
                id: id.to_string(),
                name: profile.name.clone(),
                mc_version,
                loader,
                last_played: played.get(id).cloned(),
                runtime: InstanceRuntime::Idle,
                icon: None,
                isolation: isolation_of(env, id),
                version_present: env.game.is_version_installed(&version_id),
                launch: super::launch_args::view(env, id, &version_id),
                version_id,
            }
        })
        .collect()
}

/// 改实例隔离档位（写实例设置文件；不动任何数据）。
///
/// # Errors
///
/// 实例不存在、文件读不了/写不了时返回用户可读文案。
pub fn set_isolation(env: &HostEnv, id: &str, mode: &str) -> Result<(), String> {
    let parsed = IsolationMode::parse(mode)
        .ok_or_else(|| format!("未知隔离档位 `{mode}`（isolated / shared）"))?;
    let profiles = profiles(env);
    let profile = profiles
        .profiles
        .iter()
        .find(|profile| profile.id.as_str() == id)
        .ok_or_else(|| format!("找不到实例 `{id}`"))?;
    let mut config = read_instance_config(&env.layout, id)
        .map_err(|error| format!("读实例设置失败：{error}"))?
        .unwrap_or_else(|| {
            unlucky_core::game::install::instance::InstanceConfig::new(
                profile.id.as_str(),
                &profile.version_id,
            )
        });
    config.isolation = Some(match parsed {
        IsolationMode::Isolated => unlucky_core::game::install::instance::Isolation::Isolated,
        IsolationMode::Shared => unlucky_core::game::install::instance::Isolation::Shared,
    });
    write_instance_config(&env.layout, &config)
        .map(|_| ())
        .map_err(|error| format!("写实例设置失败：{error}"))
}

/// 启动前的切换检测：把 M30 的 `ModeChangeWarning` 翻成横幅通知。
///
/// 只在真启动时调用（会写 `lastModeChange.warned`，保证同一次切换只提示一次）。
pub fn mode_change_notice(env: &HostEnv, id: &str) -> Option<NoticeView> {
    let mut config = read_instance_config(&env.layout, id).ok()??;
    let warning = config.take_mode_change_warning()?;
    let _ = write_instance_config(&env.layout, &config);
    Some(NoticeView::new(
        &format!("isolation:{id}"),
        NoticeLevel::Warn,
        "隔离档位已切换",
        &warning.message(),
    ))
}

/// 建目录（数据根首次运行时）。
pub fn ensure_root(env: &HostEnv) {
    let _ = env.layout.ensure();
    let _ = std::fs::create_dir_all(env.game.root());
}

/// 版本本体目录是否存在（启动前快速判断）。
pub fn version_present(env: &HostEnv, version_id: &str) -> bool {
    env.game.is_version_installed(version_id)
}

/// 路径显示成短串（通知/错误文案用）。
pub fn short_path(path: &Path) -> String {
    path.display().to_string()
}

#[cfg(test)]
mod tests;
