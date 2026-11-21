//! 实例快照：`profiles.json` ∪ 盘上的 `versions/*` + 版本 JSON + 会话索引 + 实例设置
//! → [`InstanceSummary`]。
//!
//! 「磁贴上的每个字」都在这里翻译完：版本号取自继承链根、加载器取自版本 id（解析在
//! [`unlucky_core::game::loader`]，本文件只做 DTO 翻译）、最后游玩时间取自会话索引、
//! 隔离档位取自实例设置（缺省跟全局 `instances.mode`）。
//!
//! **列表是并集**（M273）：档案 + 盘上有清单但没有档案绑定的版本目录，见 [`instances`] 的文档。
//!
//! **根是多个**（M274）：配置的 `game_home` + 官方启动器的标准位置（`%APPDATA%\.minecraft` 等），
//! 规范化真实路径去重（[`roots`]）。每个自动发现的实例因此带**自己的根**——读版本清单、派生运行
//! 目录、列存档 / 模组都走 [`game_for`]，不全是 `env.game`。

use std::collections::BTreeMap;

use unlucky_core::game::install::instance::{
    read_instance_config, read_profiles, write_instance_config, write_profiles,
};
use unlucky_core::game::isolation::IsolationMode;
use unlucky_core::game::{GameLayout, InstanceProfile, ProfileId, ProfileList, VersionJson};

use crate::dto::{
    InstanceRuntime, InstanceSummary, LoaderInfo, LoaderKind, NoticeLevel, NoticeView,
};

use super::env::HostEnv;

mod discovered;
// M302：实例扫盘的代次账 + 后台线程入口（`host/mod.rs` 贴 400 行硬线，逻辑放这里）
mod refresh;
mod roots;

pub(super) use discovered::{base_mode, game_for};
pub(super) use refresh::{spawn_scan, Scan};

use discovered::{display_name, external_summary, external_version_of};

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
///
/// **按传入的那个根读**（M274）：外部实例的版本清单在自己那个根下，拿配置根去读会读空。
pub fn describe_version(game: &GameLayout, version_id: &str) -> (String, LoaderInfo) {
    let loader = loader_from_id(version_id);
    let mut current = version_id.to_string();
    let mut seen = 0;
    while seen < 16 {
        seen += 1;
        let path = game.version_json(&current);
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

/// 实例的隔离档位（实例文件优先，缺省 = [`base_mode`]）。
///
/// `game` 是这个实例**自己那个根**的布局（多根之后同一个版本 id 在两个根里可以有不同基底档）。
pub fn isolation_of(env: &HostEnv, game: &GameLayout, id: &str, version_id: &str) -> String {
    let base = base_mode(env, game, id, version_id);
    match read_instance_config(&env.layout, id) {
        Ok(Some(config)) => config.effective_isolation(base).as_str().to_string(),
        _ => base.as_str().to_string(),
    }
}

/// 实例快照：**档案 ∪ 盘上的版本目录**（M273），版本目录来自**多个游戏根**（M274）。
///
/// ## 为什么是并集（M273）
///
/// 用户实测：HMCL 能看见 `<.minecraft>/versions/樱落怡然客户端`，ULL 的实例页却是空的——
/// 因为这里以前只列 `profiles.json` 里的档案。第三方启动器（HMCL / PCL / 官方启动器）把实例
/// 存在 `versions/<id>/<id>.json`，**没有**我们的档案记录。只读档案 ⇒ 已装好的实例在界面上
/// 不存在，用户以为「读不到这个 .minecraft」。
///
/// ## 为什么是多个根（M274）
///
/// 官方启动器用的是 `%APPDATA%\.minecraft`，便携模式的 ULL 用的是 `<exe>/.minecraft`——**两个
/// 不同的目录**。只扫配置根等于「装了官方启动器也看不见它的实例」。所以这里遍历
/// `roots::all(env)`：配置根 + 官方启动器的标准位置（规范化真实路径去重，见 `roots::key_of`）。
///
/// 那一批根里**只有配置根**有档案语义：`profiles.json` 的 `version_id` 指的是配置根下的版本，
/// 所以「已被档案绑定 ⇒ 不重复列」这条只对配置根成立；别的根里的同名版本是**另一个实例**
/// （id 里带着根，见 `discovered::external_instance_id`）。
///
/// ## 三条不变量
///
/// 1. **只读**：合成视图不建目录、不写 `profiles.json`、不碰任何 `versions/` 里一个字节
///    （`instances()` 会被每次刷新调用，写盘会变成「一打开启动器就改别人的文件夹」）；
/// 2. **档案优先**：配置根里某个版本已经有档案绑定（不论档案 id 是什么）⇒ 只按档案出**一条**，
///    不重复挂合成视图（用户改过的名字/隔离档不会因为盘上有同名版本被顶掉）；
/// 3. **坏东西不当实例**：清单缺 / JSON 坏 / `id` 与目录名不一致 / 缺父版本 / 越界符号链接
///    ⇒ 不进列表（`AdoptCandidate::adoptable()`）；某个根整个读不出来也只是少列它那几条，
///    不让实例页因此报错。
pub fn instances(env: &HostEnv) -> Vec<InstanceSummary> {
    let profiles = profiles(env);
    let played = last_played(env);
    // **每个根只扫一遍**（M274 r1 复审 P2-1）：这一拍的「根 × 版本」清单读一次，之后
    // ①每份档案属于哪个根、②它是否占住配置根的同名版本，都是内存查表。
    // 反查写进循环里是 `#根 × (1 + #ext 档案)` 次全树扫描——官方根 200 版本 + 10 份 ext 档案
    // 时 `instances()` 会从 236ms 涨到 1.35s（复审实测 ×5.7）。
    let found = discovered::Found::scan(env);
    let mut homes: Vec<Option<GameLayout>> = Vec::with_capacity(profiles.profiles.len());
    let mut bound: Vec<&str> = Vec::new();
    for profile in &profiles.profiles {
        // 两件事用**同一份**判据（`binds_configured_root` 与写盘那道门共用一个函数）：
        // ① 这条档案属于哪个根（磁贴按它描述自己）；② 它是否占住配置根的同名版本。
        if discovered::binds_configured_root(&found, profile) {
            bound.push(profile.version_id.as_str());
        }
        homes.push(found.game_of(profile.id.as_str()).cloned());
    }
    let mut snapshot: Vec<InstanceSummary> = profiles
        .profiles
        .iter()
        .zip(&homes)
        .map(|(profile, home)| profile_summary(env, profile, home.as_ref(), &played))
        .collect();
    let known: Vec<&str> = profiles
        .profiles
        .iter()
        .map(|profile| profile.id.as_str())
        .collect();
    for (root, versions) in found.roots() {
        let configured = root.configured();
        for version_id in versions {
            let id = discovered::external_instance_id(root.key(), version_id);
            if known.contains(&id.as_str()) {
                continue;
            }
            if configured && bound.contains(&version_id.as_str()) {
                continue;
            }
            snapshot.push(external_summary(env, root, version_id, &played));
        }
    }
    snapshot
}

/// 一条档案 → 磁贴（档案路径的唯一翻译点）。
///
/// `home` 是这条档案**自己那个根**（[`discovered::Found`] 查出来的；`None` = 配置根）：
/// 懒创建出来的 `ext-` 档案可能属于官方根，那时「版本在不在盘上 / 隔离档 / MC 版本 / 版本门槛」
/// 都必须按那个根算——否则磁贴说「文件缺失 · 共用目录」，而存档与启动实际读的是官方根
/// （M274 r1 复审 P1-1：显示与行为分叉，最坏的一种）。
fn profile_summary(
    env: &HostEnv,
    profile: &InstanceProfile,
    home: Option<&GameLayout>,
    played: &BTreeMap<String, String>,
) -> InstanceSummary {
    let version_id = profile.version_id.clone();
    let game = home.unwrap_or(&env.game);
    let (mc_version, loader) = describe_version(game, &version_id);
    let id = profile.id.as_str();
    InstanceSummary {
        id: id.to_string(),
        name: profile.name.clone(),
        mc_version,
        loader,
        last_played: played.get(id).cloned(),
        runtime: InstanceRuntime::Idle,
        icon: None,
        isolation: isolation_of(env, game, id, &version_id),
        version_present: game.is_version_installed(&version_id),
        launch: super::launch_args::view(env, game, id, &version_id),
        version_id,
    }
}

/// 拿实例的档案；外部实例（盘上有版本、没有档案）**就地补一条并落盘**。
///
/// 只在「用户第一次改这个实例」时才走到这里（[`set_isolation`] / 启动参数写回），于是
/// 「读」永远不写盘：`instances()` 合成视图、启动、改运行目录都不碰 `profiles.json`。
///
/// 幂等：同样的 `id` 第二次调用直接返回已落盘的那条，不会长出第二条。
///
/// # Errors
///
/// 档案既不存在、盘上也找不到对应版本时返回用户可读文案；读写 `profiles.json` 失败同样。
pub(super) fn ensure_profile(env: &HostEnv, id: &str) -> Result<InstanceProfile, String> {
    let mut list =
        read_profiles(&env.layout).map_err(|error| format!("读实例清单失败：{error}"))?;
    if let Some(existing) = list
        .profiles
        .iter()
        .find(|profile| profile.id.as_str() == id)
    {
        return Ok(existing.clone());
    }
    let Some((root, version_id)) = external_version_of(env, &list, id) else {
        return Err(format!("找不到实例 `{id}`"));
    };
    let profile = InstanceProfile {
        id: ProfileId::new(id),
        // 显示名与磁贴同源：别的根里的同名版本带上根路径，否则两条磁贴分不清（M274）。
        name: display_name(&root, &version_id),
        version_id,
        ..InstanceProfile::default()
    };
    list.profiles.push(profile.clone());
    write_profiles(&env.layout, &list).map_err(|error| format!("写实例清单失败：{error}"))?;
    Ok(profile)
}

/// 改实例隔离档位（写实例设置文件；不动任何数据）。
///
/// 外部实例（盘上有版本、还没有档案）**第一次**改档位时才懒创建档案并落盘
/// （[`ensure_profile`]）——在那之前，浏览列表不会在用户的盘上多出任何文件。
///
/// # Errors
///
/// 实例不存在、文件读不了/写不了时返回用户可读文案。
pub fn set_isolation(env: &HostEnv, id: &str, mode: &str) -> Result<(), String> {
    let parsed = IsolationMode::parse(mode)
        .ok_or_else(|| format!("未知隔离档位 `{mode}`（isolated / shared）"))?;
    let profile = ensure_profile(env, id)?;
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

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_union;

#[cfg(test)]
mod tests_multiroot;

#[cfg(test)]
mod tests_multiroot2;

#[cfg(test)]
mod tests_multiroot3;
