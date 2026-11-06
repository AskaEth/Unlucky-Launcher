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

    /// 版本 id → DTO 加载器的接口面（解析在 core，这里只验翻译没走样）。
    ///
    /// 抓得住：显示路径把某个种类翻错/翻塌（例如 Quilt 写成 Forge）——`dto_kind` 的委派与
    /// `dto` 的 `from_core` 都落在这一条上；以及 core 解析端退回老口径（`fabric-loader-…` 才认、
    /// 自家 `1.20.4-fabric-0.16.5` 认不出）——后两条正是 M122 的 bug 面。
    /// 抓不住：core 解析规则本身（真值表在 `unlucky_core::game::loader` 的测试里，那边才是
    /// 真相源）；也抓不住「版本号为空时不显示品牌名」那条（在 `LoaderInfo::badge` 与
    /// app-iced 的 `loader_badge`，M120 已修，别在这里重复）。
    #[test]
    fn loader_parsing_covers_all_shapes() {
        let cases = [
            ("1.20.4", LoaderKind::Vanilla, ""),
            // 自家口径（生成端 `LoaderKind::version_id`）：`<mc>-<kind>-<loader>`
            ("1.20.4-fabric-0.16.5", LoaderKind::Fabric, "0.16.5"),
            ("1.20.4-quilt-0.27.0", LoaderKind::Quilt, "0.27.0"),
            ("1.20.1-forge-47.2.0", LoaderKind::Forge, "47.2.0"),
            ("1.21.1-neoforge-21.1.80", LoaderKind::NeoForge, "21.1.80"),
            // 老 HMCL 口径（存量实例还会带着它们）
            ("fabric-loader-0.16.5-1.20.1", LoaderKind::Fabric, "0.16.5"),
            ("quilt-loader-0.24.0-1.20.1", LoaderKind::Quilt, "0.24.0"),
            // 第三方口径
            ("1.13.2-OptiFine_HD_U_E6", LoaderKind::OptiFine, "HD_U_E6"),
            ("1.12.2-LiteLoader", LoaderKind::LiteLoader, ""),
        ];
        for (id, kind, version) in cases {
            let info = loader_from_id(id);
            assert_eq!(info.kind, kind, "{id}");
            assert_eq!(info.version, version, "{id}");
            assert!(!info.badge().is_empty());
        }
    }

    /// 显示路径的**全量**锚：core 的每一种加载器，从自家版本 id 走到磁贴时仍然是**它自己**。
    ///
    /// 清单取自 [`unlucky_core::game::LoaderKind::ALL`]，**不手抄**：M126 r3 复审实测出的缺口正是
    /// 「手抄的清单跟着漏」——core 加第八家、契约与 help 都补齐、只在 `dto_kind` 那一支写
    /// `=> Vanilla`，三个 crate 全绿而磁贴静默显示「原版」。`ALL` 由 core 的宏表长出来，加一家
    /// 自动进圈，测试作者不需要（也没法）记得补一行。
    ///
    /// 抓得住：显示路径任何一环把种类翻塌成原版——`dto_kind` 又变回手写映射、`dto` 的
    /// `from_core` 那一支接错、`label()` 被写成「原版」；**包括新加的那种**（那时 `ALL` 先变长）。
    /// 抓不住：core 的 id 解析规则本身（真值表在 `unlucky_core::game::loader`）；版本号为空的
    /// 徽标判据（M120 的 `loader_badge`，本 crate 不渲染）。
    #[test]
    fn every_core_loader_kind_reaches_the_tile_as_itself() {
        use unlucky_core::game::LoaderKind as Core;
        for kind in Core::ALL {
            let info = loader_from_id(&format!("1.20.4-{}-0.16.5", kind.as_str()));
            assert_eq!(
                info.kind.as_str(),
                kind.as_str(),
                "core 的 {kind:?} 走到磁贴时变成了 `{}`（徽标 `{}`）",
                info.kind.as_str(),
                info.badge()
            );
            assert_eq!(
                info.badge().starts_with(LoaderKind::Vanilla.label()),
                kind == Core::Vanilla,
                "core 的 {kind:?} 在磁贴上的徽标是 `{}`",
                info.badge()
            );
        }
    }

    /// 端到端（数据层）：真装出来的四个加载器实例，走 [`instances`] 的快照各自带对加载器。
    ///
    /// 造的是本启动器**真实产物**的形状：`profiles.json` 里 `versionId` 是自家 id，
    /// `versions/<id>/<id>.json` 里 `inheritsFrom` 指向原版层（生成端见
    /// `unlucky_core::addon::loader`）。原版版本号由此取自继承链根，不是从 id 里切的。
    ///
    /// 抓得住：`describe_version` ↔ `loader_from_id` ↔ `instances` 这条链上任何一环把加载器
    /// 丢掉（磁贴会显示「原版」——M122 的用户可见症状）；以及继承链根取错（磁贴的版本号）。
    /// 抓不住：实例页真出图（走 app-iced 的视图模型与 `loader_badge`，本 crate 不渲染）；
    /// 也抓不住「真下载安装了 Fabric/Quilt/Forge/NeoForge」——这里只伪造落盘结果。
    #[test]
    fn own_installed_instances_snapshot_carries_their_loader() {
        let dir = std::env::temp_dir().join(format!("ull-host-game-own-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let env = HostEnv::at_root(&dir);
        std::fs::create_dir_all(env.layout.config()).expect("config 目录");

        let cases = [
            (
                "1.20.4-fabric-0.16.5",
                "1.20.4",
                LoaderKind::Fabric,
                "Fabric 0.16.5",
            ),
            (
                "1.20.4-quilt-0.27.0",
                "1.20.4",
                LoaderKind::Quilt,
                "Quilt 0.27.0",
            ),
            (
                "1.20.1-forge-47.2.0",
                "1.20.1",
                LoaderKind::Forge,
                "Forge 47.2.0",
            ),
            (
                "1.21.1-neoforge-21.1.80",
                "1.21.1",
                LoaderKind::NeoForge,
                "NeoForge 21.1.80",
            ),
        ];
        for (version_id, mc_version, _, _) in cases {
            for (id, inherits) in [(mc_version, None), (version_id, Some(mc_version))] {
                let json = match inherits {
                    Some(parent) => format!(r#"{{"id":"{id}","inheritsFrom":"{parent}"}}"#),
                    None => format!(r#"{{"id":"{id}"}}"#),
                };
                let path = env.game.version_json(id);
                std::fs::create_dir_all(path.parent().expect("版本目录")).expect("版本目录");
                std::fs::write(&path, json).expect("写版本 JSON");
            }
        }

        let profiles = ProfileList {
            profiles: cases
                .iter()
                .map(
                    |(version_id, _, _, _)| unlucky_core::game::InstanceProfile {
                        id: unlucky_core::game::ProfileId::generate(),
                        name: format!("装好的 {version_id}"),
                        version_id: (*version_id).to_string(),
                        game_dir: None,
                        running_dir: None,
                        java_path: None,
                        created_at: None,
                        last_played_at: None,
                        extra: BTreeMap::new(),
                    },
                )
                .collect(),
            ..ProfileList::default()
        };
        unlucky_core::game::install::instance::write_profiles(&env.layout, &profiles)
            .expect("写清单");

        let snapshot = instances(&env);
        assert_eq!(snapshot.len(), cases.len());
        for ((version_id, mc_version, kind, badge), tile) in cases.iter().zip(&snapshot) {
            assert_eq!(tile.version_id.as_str(), *version_id);
            assert_eq!(
                tile.mc_version.as_str(),
                *mc_version,
                "{version_id} 的版本号取错"
            );
            assert_eq!(tile.loader.kind, *kind, "{version_id} 的加载器");
            assert_eq!(tile.loader.badge().as_str(), *badge, "{version_id} 的徽标");
            assert!(tile.version_present, "{version_id} 的载荷应当存在");
        }
        std::fs::remove_dir_all(&dir).ok();
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
