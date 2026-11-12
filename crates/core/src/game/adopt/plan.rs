//! 落点计算与真正的登记：`plan`（纯算 + 零写盘）与 `adopt`（plan + 只写自己的两份配置）。
//!
//! # 写盘面（全部在 ULL 数据根下，游戏根一个字节都不动）
//!
//! 1. `config/instances/<id>.json`（实例设置：隔离档 + 运行目录）；
//! 2. `config/profiles.json`（档案列表）；
//! 3. 选了缺省运行目录时，`<data>/instances/<实例目录名>` 这个空目录（已存在且非空 ⇒ 拒绝；
//!    路径上的已有祖先若解析到**数据根之外**（符号链接/目录联接）⇒ 拒绝，不沿它写盘；
//!    已被**别的实例**占着 ⇒ 顺延 `-2`/`-3` 后缀，绝不让两个实例共用同一个运行目录）。
//!
//! 写配置**之前**还要核一遍真实写面（`config/`、`config/instances/`、`profiles.json` 必须落在
//! `<.ULL>` 内、且默认写盘面不得与游戏树重叠），判据见 [`fspaths`]。
//!
//! `versions/` `libraries/` `assets/` **一个字节都不写**：登记只填 `version_id`，内容靠 D20 的
//! 「恒共享层」被启动/下载链路复用（[`crate::game::isolation::SHARED_LAYERS`]）。

use std::path::{Path, PathBuf};

use super::super::install::instance::{read_profiles, register_with, InstanceSettings, Isolation};
use super::super::layout::GameLayout;
use super::super::profiles::ProfileList;
use super::candidate::{AdoptCandidate, AdoptTarget};
use super::error::AdoptError;
use super::fspaths;
use super::inspect::inspect;
use super::run_dir;
use crate::platform::layout::LauncherLayout;

/// 一次登记请求（CLI / presenter 的输入形状）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdoptRequest {
    /// 要接进来的版本 id（= `versions/<id>/`）。
    pub version_id: String,
    /// 展示名；缺省 = 版本 id。
    pub name: Option<String>,
    /// 运行目录选择（缺省 [`AdoptTarget::InstanceDir`]，**不是** `isolated`）。
    pub target: AdoptTarget,
    /// 档案的创建时间（RFC-3339；core 不碰时钟格式，由调用方给）。
    pub created_at: Option<String>,
}

impl AdoptRequest {
    /// 最小请求（缺省名 + 缺省运行目录）。
    #[must_use]
    pub fn new(version_id: impl Into<String>) -> Self {
        Self {
            version_id: version_id.into(),
            name: None,
            target: AdoptTarget::default(),
            created_at: None,
        }
    }
}

/// 一次登记的**计划**（不写任何东西；`--dry-run` / GUI 预检就吃这个）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdoptPlan {
    /// 检查过的候选（含问题清单，正常时为空）。
    pub candidate: AdoptCandidate,
    /// 将写进档案的展示名。
    pub name: String,
    /// 运行目录选择。
    pub target: AdoptTarget,
    /// 生效隔离档。
    pub isolation: Isolation,
    /// 生效运行目录（= `--gameDir`）。
    pub run_dir: PathBuf,
    /// 人读提示（共享/版本目录的后果、已有同版本实例…）。
    pub notes: Vec<String>,
}

/// 一次登记的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdoptOutcome {
    /// 实例 id（稳定 UUID）。
    pub instance_id: String,
    /// 展示名。
    pub name: String,
    /// 绑定的版本 id。
    pub version_id: String,
    /// 实例设置文件路径。
    pub config_path: PathBuf,
    /// 生效隔离档。
    pub isolation: Isolation,
    /// 运行目录选择。
    pub target: AdoptTarget,
    /// 生效运行目录。
    pub run_dir: PathBuf,
    /// 是否新建了档案（`false` 只可能是复用同名同版本档案，本命令会先拒绝那种情况）。
    pub created: bool,
    /// 是否新建了缺省运行目录（`false` = 目录已存在但为空、直接复用）。
    pub dir_created: bool,
    /// 人读提示（与 [`AdoptPlan::notes`] 同一份）。
    pub notes: Vec<String>,
}

/// 计算一次登记：检查候选 + 解析名字 + 查重名 + 算运行目录。**零写盘**。
///
/// # Errors
///
/// - [`AdoptError::UnknownVersion`]：版本不在盘上；
/// - [`AdoptError::NotAdoptable`]：候选有问题（清单坏/缺父/环/越界…，原因逐条给出；由
///   [`inspect`] 判出来）；
/// - [`AdoptError::DuplicateName`] / [`AdoptError::AlreadyRegistered`]：名字冲突 / 重复登记；
/// - [`AdoptError::TargetNotEmpty`] / [`AdoptError::TargetInvalid`]：运行目录不可用；
/// - [`AdoptError::Game`]：`profiles.json` 读不了。
pub fn plan(
    game: &GameLayout,
    launcher: &LauncherLayout,
    request: &AdoptRequest,
) -> Result<AdoptPlan, AdoptError> {
    // 写盘面两道判据都放在 `inspect` 之前（dry-run 也要报，别让预检说「可以」）：
    // ① 默认写盘面不得与**受保护游戏树**重叠（r2 R2.2；整体迁移到游戏树外照旧允许）；
    // ② 配置落点必须在 `<.ULL>` 内（r1 R2）。
    // 放前面是有意的——检查候选的读盘也会穿过 `config/`（`profiles.json`），写面不可信时先停手。
    fspaths::ensure_surfaces_clear_of_game_tree(game, launcher, None)?;
    fspaths::ensure_config_write_surface(launcher)?;
    // `inspect` 只在候选**可登记**时返回 `Ok`，不合格一律是 `NotAdoptable`（问题清单已带出）。
    let candidate = inspect(game, launcher, &request.version_id)?;
    let name = resolve_name(request.name.as_deref(), &candidate.version_id);
    let profiles = read_profiles(launcher)?;
    let mut notes = Vec::new();
    // 重名与重复登记：**显式报错**，不静默复用（复用会把用户「导入这个」的意图改成「改一个别的实例」）。
    if let Some(existing) = profiles
        .profiles
        .iter()
        .find(|profile| profile.name == name)
    {
        if existing.version_id == candidate.version_id {
            return Err(AdoptError::AlreadyRegistered {
                version_id: candidate.version_id.clone(),
                name,
                instance_id: existing.id.as_str().to_owned(),
            });
        }
        return Err(AdoptError::DuplicateName {
            name,
            instance_id: existing.id.as_str().to_owned(),
        });
    }
    for profile in &candidate.registered {
        notes.push(format!(
            "这个版本已经有实例了：`{}`（id {}）——再登记会得到第二个实例，两个实例共用同一份版本文件",
            profile.name, profile.id
        ));
    }

    let (run_dir, bump_notes) = run_dir_of(
        game,
        launcher,
        &candidate.version_id,
        &name,
        &request.target,
        &profiles,
    )?;
    notes.extend(bump_notes);
    // 缺省运行目录也要落在游戏树之外（`shared`/`version_dir`/`custom` 是显式选择，不复检）。
    if request.target == AdoptTarget::InstanceDir {
        fspaths::ensure_surfaces_clear_of_game_tree(game, launcher, Some(&run_dir))?;
    }
    notes.extend(target_notes(&request.target, &run_dir));

    Ok(AdoptPlan {
        candidate,
        name,
        target: request.target.clone(),
        isolation: request.target.isolation(),
        run_dir,
        notes,
    })
}

/// 按计划登记：先准备缺省运行目录（若选了它），再写实例设置与档案。
///
/// # Errors
///
/// 同 [`plan`]，外加建目录/写配置失败（[`AdoptError::Game`]）。
pub fn adopt(
    game: &GameLayout,
    launcher: &LauncherLayout,
    request: &AdoptRequest,
) -> Result<AdoptOutcome, AdoptError> {
    let plan = plan(game, launcher, request)?;
    // `plan` 已核过；写盘前再核一次（中间可能有别人动过树），失败就一个字节都不写。
    fspaths::ensure_surfaces_clear_of_game_tree(game, launcher, None)?;
    if plan.target == AdoptTarget::InstanceDir {
        fspaths::ensure_surfaces_clear_of_game_tree(game, launcher, Some(&plan.run_dir))?;
    }
    fspaths::ensure_config_write_surface(launcher)?;
    let mut dir_created = false;
    // 目录先建：建不出来就一个配置都不写（失败的残留是一个空目录，可复用）。
    if plan.target == AdoptTarget::InstanceDir && !plan.run_dir.is_dir() {
        run_dir::ensure_inside_data(launcher, &plan.run_dir)?;
        std::fs::create_dir_all(&plan.run_dir)
            .map_err(|error| AdoptError::Game(crate::game::GameError::io(&plan.run_dir, error)))?;
        dir_created = true;
        // 建完再核一次：`create_dir_all` 会沿已存在的目录联接走，越过边界就不算成功。
        run_dir::ensure_inside_data(launcher, &plan.run_dir)?;
    }
    let settings = InstanceSettings {
        isolation: Some(plan.isolation),
        game_dir: match &plan.target {
            // `custom` 档必须有运行目录：缺省目录与用户目录都要落进 `gameDir`。
            AdoptTarget::InstanceDir | AdoptTarget::Custom(_) => Some(plan.run_dir.clone()),
            AdoptTarget::Shared | AdoptTarget::VersionDir => None,
        },
    };
    let registration = register_with(
        launcher,
        &plan.name,
        &plan.candidate.version_id,
        request.created_at.as_deref(),
        &settings,
    )?;
    Ok(AdoptOutcome {
        instance_id: registration.profile.id.as_str().to_owned(),
        name: registration.profile.name.clone(),
        version_id: registration.profile.version_id.clone(),
        config_path: registration.config_path,
        isolation: plan.isolation,
        target: plan.target,
        run_dir: plan.run_dir,
        created: registration.created,
        dir_created,
        notes: plan.notes,
    })
}

/// 运行目录的派生（缺省落点复用整合包导入的 `<data>/instances/<实例目录名>`）。
///
/// 缺省档**必须**查「已有实例占着哪个真实目录」（R1）：slug 与大小写不敏感都会让两个不同展示名
/// 落到同一个物理目录上；撞上就顺延后缀（见 [`run_dir::derive_instance_dir`]），而不是让两个
/// 实例悄悄共用一个运行目录。返回（目录，顺延提示）。
fn run_dir_of(
    game: &GameLayout,
    launcher: &LauncherLayout,
    version_id: &str,
    name: &str,
    target: &AdoptTarget,
    profiles: &ProfileList,
) -> Result<(PathBuf, Vec<String>), AdoptError> {
    match target {
        AdoptTarget::InstanceDir => {
            let claims = run_dir::claims(launcher, profiles)?;
            run_dir::derive_instance_dir(launcher, name, &claims)
        }
        AdoptTarget::Shared => Ok((game.root().to_path_buf(), Vec::new())),
        AdoptTarget::VersionDir => Ok((game.version_dir(version_id), Vec::new())),
        AdoptTarget::Custom(dir) => Ok((run_dir::resolve_custom_dir(dir)?, Vec::new())),
    }
}

/// 运行目录选择的后果说明（人读；CLI 与 GUI 共用同一份话术）。
///
/// `shared` / `version_dir` 必须**明确告知**：它们会把玩家数据写进别人的 `.minecraft`
/// （D20 的 `isolated` 档运行目录就是 `versions/<id>/`）。
#[must_use]
pub fn target_notes(target: &AdoptTarget, run_dir: &Path) -> Vec<String> {
    match target {
        AdoptTarget::InstanceDir => vec![format!(
            "运行目录 = {}（ULL 数据区新建，与别人的 .minecraft 隔离；versions/libraries/assets 仍按 D20 恒共享，不复制）",
            run_dir.display()
        )],
        AdoptTarget::Shared => vec![format!(
            "运行目录 = 游戏根 {}（共享档）：mods/saves/config 会落在别人的 .minecraft 里，与其它启动器共用；\
             要隔离请用缺省（ULL 数据区新目录）或 --isolation custom --game-dir <dir>",
            run_dir.display()
        )],
        AdoptTarget::VersionDir => vec![format!(
            "运行目录 = 版本目录 {}（版本隔离档）：第一次启动会在这里新增 mods/saves/config/logs，\
             而这个目录是别的启动器的版本目录（不覆盖已有文件）；要隔离请用缺省或 --isolation custom",
            run_dir.display()
        )],
        AdoptTarget::Custom(_) => vec![format!(
            "运行目录 = {}（用户指定；不新建、不改动其中已有内容）",
            run_dir.display()
        )],
    }
}

/// 展示名：去空格，空串退回版本 id（与 `pack import` 的「名字不给就用包名」同一口径）。
fn resolve_name(name: Option<&str>, version_id: &str) -> String {
    name.map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| version_id.to_owned())
}
