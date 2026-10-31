//! 实例快照：`profiles.json` + 版本 JSON + 会话索引 + 实例设置 → [`InstanceSummary`]。
//!
//! 「磁贴上的每个字」都在这里翻译完：版本号取自继承链根、加载器取自版本 id 前缀、
//! 最后游玩时间取自会话索引、隔离档位取自实例设置（缺省跟全局 `instances.mode`）。

use std::collections::BTreeMap;
use std::path::Path;

use unlucky_core::game::install::instance::{
    read_instance_config, read_profiles, write_instance_config,
};
use unlucky_core::game::isolation::IsolationMode;
use unlucky_core::game::{ProfileList, VersionJson};
use unlucky_core::launch::session_index;

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

/// 全会话索引里「每个实例最近一次启动」的时间串。
pub fn last_played(env: &HostEnv) -> BTreeMap<String, String> {
    let index = session_index::index_path(&env.sessions_root());
    let entries = session_index::read_all(&index).unwrap_or_default();
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

/// 版本 id → 加载器（HMCL 同款命名口径：`1.20.1-forge-47.2.0` / `fabric-loader-0.16.5-1.20.1`）。
pub fn loader_from_id(version_id: &str) -> LoaderInfo {
    let lower = version_id.to_ascii_lowercase();
    let grab = |marker: &str| -> String {
        lower
            .split_once(marker)
            .map(|(_, rest)| rest.trim_start_matches('-').to_string())
            .unwrap_or_default()
    };
    let (kind, version) = if lower.contains("neoforge") {
        (LoaderKind::NeoForge, grab("neoforge-"))
    } else if lower.contains("fabric-loader") {
        (LoaderKind::Fabric, grab("fabric-loader-"))
    } else if lower.contains("quilt-loader") {
        (LoaderKind::Quilt, grab("quilt-loader-"))
    } else if lower.contains("forge") {
        (LoaderKind::Forge, grab("forge-"))
    } else if lower.contains("optifine") {
        (LoaderKind::OptiFine, grab("optifine"))
    } else if lower.contains("liteloader") {
        (LoaderKind::LiteLoader, grab("liteloader-"))
    } else {
        (LoaderKind::Vanilla, String::new())
    };
    LoaderInfo { kind, version }
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
            InstanceSummary {
                id: profile.id.as_str().to_string(),
                name: profile.name.clone(),
                mc_version,
                loader,
                last_played: played.get(profile.id.as_str()).cloned(),
                runtime: InstanceRuntime::Idle,
                icon: None,
                isolation: isolation_of(env, profile.id.as_str()),
                version_present: env.game.is_version_installed(&version_id),
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
mod tests {
    use super::*;

    #[test]
    fn loader_parsing_covers_all_shapes() {
        let cases = [
            ("1.20.4", LoaderKind::Vanilla, ""),
            (
                "fabric-loader-0.16.5-1.20.1",
                LoaderKind::Fabric,
                "0.16.5-1.20.1",
            ),
            (
                "quilt-loader-0.24.0-1.20.1",
                LoaderKind::Quilt,
                "0.24.0-1.20.1",
            ),
            ("1.20.1-forge-47.2.0", LoaderKind::Forge, "47.2.0"),
            ("1.21.1-neoforge-21.1.80", LoaderKind::NeoForge, "21.1.80"),
        ];
        for (id, kind, version) in cases {
            let info = loader_from_id(id);
            assert_eq!(info.kind, kind, "{id}");
            assert_eq!(info.version, version, "{id}");
            assert!(!info.badge().is_empty());
        }
    }

    #[test]
    fn isolation_write_then_read_round_trips() {
        let dir = std::env::temp_dir().join(format!("ull-host-game-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let env = HostEnv::at_root(&dir);
        std::fs::create_dir_all(env.layout.config()).expect("config 目录");

        // 直接建一份 profiles.json（register 需要版本本体，这里只测隔离写读）
        let profiles = ProfileList {
            profiles: vec![unlucky_core::game::InstanceProfile {
                id: unlucky_core::game::ProfileId::generate(),
                name: "demo".to_string(),
                version_id: "1.20.1".to_string(),
                game_dir: None,
                running_dir: None,
                java_path: None,
                created_at: None,
                last_played_at: None,
                extra: BTreeMap::new(),
            }],
            ..ProfileList::default()
        };
        unlucky_core::game::install::instance::write_profiles(&env.layout, &profiles)
            .expect("写清单");
        let id = profiles.profiles[0].id.as_str().to_string();

        assert_eq!(isolation_of(&env, &id), "isolated");
        set_isolation(&env, &id, "shared").expect("切档位");
        assert_eq!(isolation_of(&env, &id), "shared");
        assert!(set_isolation(&env, &id, "nope").is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
