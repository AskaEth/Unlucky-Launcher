//! 自动发现的实例（M273 · 多根 M274）：盘上扫出来的版本 → 磁贴，以及它们的**运行目录判据**。
//!
//! 从 [`super::instances`] 里拆出来只为守 400 行硬线（那边装了档案侧的翻译）。本文件三件事：
//!
//! 1. **哪些盘上的版本算实例**：`versions/<id>/<id>.json` 能解析（判据来自 `core::game::adopt::scan`；
//!    本文件只负责「每个根扫一遍」并把结果装进 [`Found`]）；
//! 2. **它们从哪个目录跑**：[`base_mode`]——`versions/<id>/` 里有痕迹（`mods` / `saves` /
//!    `config` / `options.txt`）就按独立目录，否则按共用目录（**它自己那个根**）；
//! 3. **身份与判据**：[`external_instance_id`]（id 里带根）、[`game_for`]（读盘走哪个根）、
//!    [`binds_configured_root`]（这条档案占住的是**配置根**里那个版本吗——列表侧与写盘那道门共用）。
//!
//! 「哪个版本已经被档案绑定」这一步**不在本文件**：它属于 [`super::instances`] 的并集规则，
//! 而写盘那道门（[`external_version_of`]）用的是同一个 [`binds_configured_root`]。
//!
//! # 多根（M274）：身份 = (根, 版本)
//!
//! 两个根里都可能有 `1.20.4`，所以版本 id **不再唯一**。身份的落点只有一个：
//! [`external_instance_id`]（把根的规范化路径喂进哈希）。于是「同一个根 + 同一个版本目录」跨重启
//! 拿到同一个 id，而两个根里的同名版本拿到两个不同 id——列表自然出两条。
//!
//! 反查（id → 根 + 版本）只能靠**重算**：哈希不可逆，所以 [`Found::scan`] 枚举全部根、每个根扫一遍
//! `versions/`，逐个比 id。**一拍只扫一遍**：`instances()` 建一份 [`Found`] 给整轮用（扫描次数 =
//! #根，r1 复审实测过旧形状是 `#根 × (1 + #ext 档案)`），单实例动作（存档 / 模组 / 启动 / 改设置）
//! 各建一份（同样是 #根 次）。
//!
//! 两条都**只读**：本文件不建目录、不写 `profiles.json`、不碰 `versions/` 一个字节，
//! 更不往官方启动器的根里写任何东西。

use std::collections::BTreeMap;

use unlucky_core::game::isolation::IsolationMode;
use unlucky_core::game::{GameLayout, InstanceProfile, ProfileList};

use crate::dto::{InstanceRuntime, InstanceSummary};

use super::super::env::HostEnv;
use super::roots::{self, GameRoot};
use super::{describe_version, global_mode, isolation_of};

/// `versions/<version_id>/` 里这些名字出现任何一个，就说明这个版本本来就被当成
/// **独立目录**在用（它自己的 mods / 存档 / 配置都在里面）。判据只 `exists()`，不读内容。
///
/// 为什么是这四个：HMCL / PCL / 官方启动器的「版本隔离」都把运行目录放在 `versions/<id>/`，
/// 于是跑过一次就会留下 `mods/` `saves/` `config/` `options.txt` 中的至少一个。
const RUN_DIR_TRACES: [&str; 4] = ["mods", "saves", "config", "options.txt"];

/// 版本目录里有没有「它自己用过」的痕迹（只读）。
///
/// **必须传这个实例自己的根**：同一个版本 id 在两个根里的痕迹可以不同（官方根里跑过、ULL 根里没有）。
fn has_own_run_dir(game: &GameLayout, version_id: &str) -> bool {
    let dir = game.version_dir(version_id);
    RUN_DIR_TRACES.iter().any(|name| dir.join(name).exists())
}

/// 实例的**基底隔离档**（实例文件之下那一层）。
///
/// | 实例 | 基底 | 依据 |
/// |---|---|---|
/// | 有档案（向导 / 用户 / AI 建的） | 全局 `instances.mode` | D20 原口径，一字不变 |
/// | **自动发现的**（id 带 [`crate::dto::DISCOVERED_ID_PREFIX`]） | `versions/<id>` 里有痕迹 ⇒ `isolated`；否则 ⇒ `shared` | 「别人怎么存我们就怎么读」 |
///
/// 为什么自动发现的实例不能一律 `isolated`：「版本隔离」下 `isolated` 的**运行目录就是**
/// `versions/<id>/`。一个从没跑过、也没有 `mods`/`saves` 的外部版本若按 `isolated` 处理，
/// 第一次启动就会在**别人的版本目录**里新长出 `mods/` `saves/` `config/`（D20 的
/// `AdoptTarget` 缺省值正是为了躲开这件事而选 `InstanceDir`）。反过来，盘上已经有这些痕迹的
/// 版本本来就是独立目录实例，按 `shared` 处理会让它突然读到根下别人的存档 ——
/// 两种错都真实存在，所以判据只能是**盘上的事实**。
///
/// **M274**：痕迹按**这个实例自己的根**判（`game` 参数）。换根之后判定不变：官方根里那个
/// 带 `mods/` 的版本仍是 `isolated`，ULL 根里那个干净的版本仍是 `shared`。
///
/// 只读：这里不建目录、不写任何文件（写盘只发生在用户第一次改设置时，见 [`super::ensure_profile`]）。
pub(in crate::host) fn base_mode(
    env: &HostEnv,
    game: &GameLayout,
    id: &str,
    version_id: &str,
) -> IsolationMode {
    if !crate::dto::is_discovered_instance(id) {
        return global_mode(env);
    }
    if has_own_run_dir(game, version_id) {
        IsolationMode::Isolated
    } else {
        IsolationMode::Shared
    }
}

/// 一个根下「盘上有版本、可登记」的那些版本 id（字典序，来自 core 的扫描）。
///
/// 扫描失败（`versions/` 读不出来 / 断链联接）时返回空：列表退回「只有档案」的旧行为。
/// 这里**不**把错误往上抛——实例页打不开是比「少列几个外部实例」严重得多的故障。
///
/// 档案绑定（`profile.version_id`）**不在这一层过滤**：那只对配置根有意义，由 [`super::instances`] 做。
pub(in crate::host) fn external_versions(env: &HostEnv, game: &GameLayout) -> Vec<String> {
    let Ok(scan) = unlucky_core::game::adopt::scan(game, &env.layout) else {
        return Vec::new();
    };
    scan.candidates
        .into_iter()
        .filter(|candidate| candidate.adoptable())
        .map(|candidate| candidate.version_id)
        .collect()
}

/// **M273 时代的 `ext-` id**：只哈希版本目录名（不含根）。M274 之后这种 id 永远反查不到根。
///
/// 存在的唯一理由：`host/game/refresh/live.rs` 判「反查落空的 `ext-` id」时要用它把两种情形分开——
/// 老档案（这条 ⇒ 放行，按配置根解析）与**根不见了**（不是这条 ⇒ 拒绝，否则会起错副本）。
pub(in crate::host) fn legacy_external_instance_id(version_id: &str) -> String {
    instance_id_from(&[version_id])
}

/// FNV-1a 64 的多段哈希：段之间插一个 `0` 分隔符（路径里不可能有 NUL，所以 `("ab","c")` 与
/// `("a","bc")` 不会撞）。
fn instance_id_from(parts: &[&str]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            // 分隔符那一个 `0` 字节的 `hash ^= 0` 是恒等，留下的痕迹只有这一次乘法
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("{}{hash:016x}", crate::dto::DISCOVERED_ID_PREFIX)
}

/// 外部实例的视图 id（= 懒创建时写进 `profiles.json` 的档案 id）。
///
/// **不能直接拿版本 id 当实例 id**：`core::platform::layout::is_valid_instance_id` 只收
/// `[A-Za-z0-9_-]{1,64}`（`config/instances/<id>.json` 与启动链的 `read_instance_config` 都按
/// 这个白名单校验），而版本目录名可以是中文——用户实测的那个就是 `樱落怡然客户端`。
/// 直接沿用版本 id 会让**启动**在写盘前就死在 `LaunchFailureStage::InstanceConfig`。
///
/// 前缀 + 稳定哈希 ⇒ 同一个 (根, 版本) 每次拿到同一个 id（懒创建因此幂等），且是合法实例 id。
///
/// **M274**：`root_key` 也进哈希（`fnv(root || 0 || version)`，见 [`instance_id_from`]）——两个根里
/// 的 `1.20.4` 因此是两个实例。
pub(in crate::host) fn external_instance_id(root_key: &str, version_id: &str) -> String {
    instance_id_from(&[root_key, version_id])
}

/// 磁贴的显示名：配置根里的版本就是版本目录名；**别的根**里的版本带上根路径。
///
/// 为什么非带不可：两个根里都可能有一个叫 `A` 的版本，列表上两条同名磁贴，用户无从分辨点哪一条
/// （而两条的存档在不同的目录里）。路径是**事实**，直接摆出来；只对配置根保持原样是为了不让
/// 绝大多数用户看到一堆长路径。
pub(in crate::host) fn display_name(root: &GameRoot, version_id: &str) -> String {
    if root.configured() {
        return version_id.to_string();
    }
    format!("{version_id} · {}", root.path().display())
}

/// 一条外部版本 → 磁贴（**只读**，不落任何文件）。
pub(in crate::host) fn external_summary(
    env: &HostEnv,
    root: &GameRoot,
    version_id: &str,
    played: &BTreeMap<String, String>,
) -> InstanceSummary {
    let id = external_instance_id(root.key(), version_id);
    let game = root.game();
    let (mc_version, loader) = describe_version(game, version_id);
    InstanceSummary {
        last_played: played.get(&id).cloned(),
        isolation: isolation_of(env, game, &id, version_id),
        launch: super::super::launch_args::view(env, game, &id, version_id),
        version_present: game.is_version_installed(version_id),
        name: display_name(root, version_id),
        version_id: version_id.to_string(),
        mc_version,
        loader,
        runtime: InstanceRuntime::Idle,
        icon: None,
        id,
    }
}

/// **一拍之内**的发现快照：每个根只扫一遍 `versions/`，之后所有「这个 id 属于哪个根」都是内存查表。
///
/// 为什么要有它（M274 r1 复审 P2-1 的实测）：把反查直接写进 `instances()` 的循环里，扫描次数是
/// `#根 × (1 + #ext 档案)`；官方根 200 个版本 + 10 份 `ext-` 档案时 `instances()` 从 236ms 涨到
/// 1.35s（×5.7）。这里把「根 × 版本」的清单一次性读完，反查退化成线性的字符串比较
/// （`ext-` id 是 16 位十六进制，逐条比一遍远比再扫一次盘便宜）。
///
/// 快照是**快照**：一拍里盘变了（用户同时在装版本）不会看到，下一次刷新重建。
pub(in crate::host) struct Found {
    roots: Vec<(GameRoot, Vec<String>)>,
}

impl Found {
    /// 扫一遍全部根（配置根在前，顺序与 `roots::all` 一致）。
    pub(in crate::host) fn scan(env: &HostEnv) -> Self {
        let roots = roots::all(env)
            .into_iter()
            .map(|root| {
                let versions = external_versions(env, root.game());
                (root, versions)
            })
            .collect();
        Self { roots }
    }

    /// 逐个根 + 它这一拍扫到的版本 id（字典序，已滤掉不可登记的）。
    pub(in crate::host) fn roots(&self) -> impl Iterator<Item = (&GameRoot, &[String])> {
        self.roots
            .iter()
            .map(|(root, versions)| (root, versions.as_slice()))
    }

    /// id → (根, 版本 id)。不是 `ext-` 前缀 / 扫不到 ⇒ `None`。
    pub(in crate::host) fn find(&self, id: &str) -> Option<(&GameRoot, &str)> {
        if !crate::dto::is_discovered_instance(id) {
            return None;
        }
        self.roots().find_map(|(root, versions)| {
            versions
                .iter()
                .find(|version_id| external_instance_id(root.key(), version_id) == id)
                .map(|version_id| (root, version_id.as_str()))
        })
    }

    /// id → 那个根的布局（查不到 ⇒ `None`，由调用方决定退回哪个根）。
    pub(in crate::host) fn game_of(&self, id: &str) -> Option<&GameLayout> {
        self.find(id).map(|(root, _)| root.game())
    }
}

/// 实例所属的那个根的布局（读盘路径的唯一入口）。
///
/// 档案实例、以及「反查不到」的 id 一律退回配置根 `env.game`——**没有第二个答案**：
/// 宁可读配置根（旧行为），也不把实例指向一个猜出来的目录。
///
/// # 退回的后果（M274 r2 复审 P2-1，任务⑥点名要写）
///
/// `is_discovered_instance(id)` 却反查不到，只有两种情形：
/// 1. **老档案**（M273 时代的 `ext-` id，哈希里没有根）——退回配置根是对的；
/// 2. **那个版本此刻不可登记**：官方启动器正在更新（清单半写 / 被锁 / 目录被移走）⇒
///    `adopt::scan` 判 `adoptable() == false` ⇒ 这里查不到。
///
/// 第 2 种情形下退回配置根是有代价的：**若配置根里恰好有同名版本**，装配会去读它、
/// `gameDir` 也落在配置根 ⇒ 可能**启动/读档到另一个根**。所以这里留一行启动器日志
/// （`logs/launcher.log`，分类 `multiroot`），让这条回落可观测，而不是让调用方无感。
pub(in crate::host) fn game_for(env: &HostEnv, id: &str) -> GameLayout {
    let found = Found::scan(env);
    if let Some(game) = found.game_of(id) {
        return game.clone();
    }
    if crate::dto::is_discovered_instance(id) {
        // 两因并列，**不要**只写后一种：`ext-` 前缀在 M273 时代就有，那时 id 只哈希版本目录名
        // （没有根），所以「M274 之前合成的老档案」**永远**反查不到——那是所有老用户的常态，
        // 不是故障。只写「清单半写/被锁」会让每次点开详情页都往日志里加一条像是在报错的假警报
        // （M274 r3 复审 P2-1）。
        unlucky_core::launch::launcher_log::record(
            "multiroot",
            &format!(
                "实例 {id} 反查不到所属根（老档案：M273 之前合成，id 不含根；或该版本此刻不可登记：清单半写/被锁/目录已不在）⇒ 按配置根 {} 解析",
                env.game.root().display()
            ),
        );
    }
    env.game.clone()
}

/// 这条档案**占住的是配置根里的那个版本吗**（`instances()` 的 `bound` 与写盘那道门共用同一份判据）。
///
/// - 反查到某个根 ⇒ 看那个根是不是配置根；
/// - 反查不到 ⇒ 按配置根口径算（老 M273 的 `ext-` id、用户/向导建的自建档案、那批根已不在盘上）。
///
/// 为什么必须是**一个**函数（M274 r2 复审 P1-1）：这两处曾经各写一份，列表侧对、写盘侧只比
/// `version_id`——于是官方根那个 `A` 被懒创建成档案后，**配置根**里同名那个 `A` 被判成
/// 「已被绑定」，`ensure_profile` 返回 `找不到实例 ext-…`，而磁贴还好端端列着。
/// 判据分叉的代价是「有时能改有时不能改」，最难排的那一种。
pub(in crate::host) fn binds_configured_root(found: &Found, profile: &InstanceProfile) -> bool {
    found
        .find(profile.id.as_str())
        .is_none_or(|(root, _)| root.configured())
}

/// 视图 id → (所属根, 版本 id) 的反查（只认「可登记、且**还没有档案绑定**」的版本）。
///
/// 与 `instances()` 同一份判据（含 [`binds_configured_root`]）：合成的视图 id 只在那个集合里有意义，
/// 于是「界面上出现过的外部实例」与「能懒创建的外部实例」永远是同一批（有了档案就走档案，
/// 不再重复列一条）。
#[must_use]
pub(in crate::host) fn external_version_of(
    env: &HostEnv,
    profiles: &ProfileList,
    id: &str,
) -> Option<(GameRoot, String)> {
    // 一拍只扫一次：下面「查自己」与「查绑定者」都用这一份。
    let found = Found::scan(env);
    let (root, version_id) = found
        .find(id)
        .map(|(root, version_id)| (root.clone(), version_id.to_string()))?;
    // 档案绑定只对**配置根**有意义（别处的同名版本是另一个实例，id 不会撞）。
    // 而「绑定者」是不是配置根里的那一条，也按它**自己的根**判——不许只比 `version_id`：
    // 官方根的 `A` 与配置根的 `A` 是两个实例（r2 复审 P1-1 的现场）。
    if root.configured() {
        let taken = profiles.profiles.iter().any(|profile| {
            profile.version_id == version_id
                && (profile.id.as_str() == id || binds_configured_root(&found, profile))
        });
        if taken {
            return None;
        }
    }
    Some((root, version_id))
}
