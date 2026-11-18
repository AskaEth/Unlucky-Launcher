//! 实例登记：把「用户点了一下导入/安装」变成 `profiles.json` + `config/instances/<id>.json`。
//!
//! 从 `install/instance.rs` 搬出来只为守 400 行硬线（M221 的 `register_with` 让那边到 430 行）；
//! 搬动是纯挪，唯一新增的是 [`InstanceSettings`] 与 [`register_with`]（登记时一并定隔离档/运行目录）。
//!
//! # 幂等
//!
//! 同一 `version_id` + 同一展示名重复登记：复用已有档案（`created = false`），不产生第二个实例、
//! 不改已有实例的 id（id 一旦写进档案就是稳定的）。
//!
//! # 写盘顺序（M221）
//!
//! **先写实例设置、后写档案**。中断时留下的是「没有档案的孤立设置文件」——不可见、下次登记同一 id
//! 会覆盖；反过来（先档案后设置）会留下一个按全局档启动的实例，而全局缺省是 `isolated`，
//! 那会把 `mods/saves/config` 造进 `versions/<versionId>/`（登记别人的版本时那正是别人的目录）。

use std::path::PathBuf;

use crate::game::error::GameError;
use crate::game::profiles::InstanceProfile;
use crate::platform::layout::LauncherLayout;

use super::{
    instance_config_path, read_profiles, write_instance_config, write_profiles, InstanceConfig,
    Isolation,
};

/// 一次登记的结果。
#[derive(Debug, Clone)]
pub struct Registration {
    /// 档案（新建或复用）。
    pub profile: InstanceProfile,
    /// 每实例设置文件路径。
    pub config_path: PathBuf,
    /// 是否新建（`false` = 已有同版本同名的实例，直接复用）。
    pub created: bool,
}

/// 登记时一并落进 `config/instances/<id>.json` 的初始设置。
///
/// 为什么要有它：登记与「运行目录怎么定」原本是两件事，于是调用方只能先 `register` 再补写设置
/// ——中间那段时间实例是 `inherit`（跟随全局 `isolated`）。把选择前移到登记这一步，就没有那个窗口。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstanceSettings {
    /// 隔离档；`None` = 跟随全局（不落盘）。
    pub isolation: Option<Isolation>,
    /// 自定义运行目录（仅 `custom` 档需要）。
    pub game_dir: Option<PathBuf>,
}

impl InstanceSettings {
    /// 跟随全局（= 未引入 [`InstanceSettings`] 前的历史行为，不落任何设置键）。
    #[must_use]
    pub fn inherit() -> Self {
        Self::default()
    }

    /// 是否什么都没选。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.isolation.is_none() && self.game_dir.is_none()
    }
}

/// 登记一个实例（幂等，跟随全局隔离档）。
///
/// # Errors
///
/// 读/写 `profiles.json`、实例 id 不合法、原子写失败。
pub fn register(
    layout: &LauncherLayout,
    name: &str,
    version_id: &str,
    created_at: Option<&str>,
) -> Result<Registration, GameError> {
    register_with(
        layout,
        name,
        version_id,
        created_at,
        &InstanceSettings::inherit(),
    )
}

/// 同 [`register`]，但把隔离档与运行目录**一并**写进新建实例的设置文件。
///
/// 复用已有档案（同版本 + 同名）时 `settings` 只在「设置文件缺失、需要补建」时生效——
/// 已有设置文件绝不改写（那是用户的显式选择，登记不该覆盖它）。
///
/// # Errors
///
/// 同 [`register`]。
pub fn register_with(
    layout: &LauncherLayout,
    name: &str,
    version_id: &str,
    created_at: Option<&str>,
    settings: &InstanceSettings,
) -> Result<Registration, GameError> {
    let mut profiles = read_profiles(layout)?;
    let existing = profiles
        .profiles
        .iter()
        .find(|profile| profile.version_id == version_id && profile.name == name)
        .cloned();

    if let Some(profile) = existing {
        let config_path = instance_config_path(layout, profile.id.as_str())?;
        // 老档案可能还没有设置文件（早期 CLI 只写 profiles.json），这里补齐。
        if !config_path.is_file() {
            let mut config = InstanceConfig::new(profile.id.as_str(), version_id);
            apply_settings(&mut config, settings);
            write_instance_config(layout, &config)?;
        }
        return Ok(Registration {
            profile,
            config_path,
            created: false,
        });
    }

    let mut profile = InstanceProfile::new(name, version_id);
    profile.created_at = created_at.map(str::to_string);
    let id = profile.id.as_str().to_string();
    let mut config = InstanceConfig::new(id, version_id);
    apply_settings(&mut config, settings);
    let config_path = write_instance_config(layout, &config)?;

    profiles.upsert(profile.clone());
    if let Err(error) = write_profiles(layout, &profiles) {
        // 档案没落盘 ⇒ 撤掉刚写的设置文件，不留半登记状态。
        let _ = std::fs::remove_file(&config_path);
        return Err(error);
    }
    Ok(Registration {
        profile,
        config_path,
        created: true,
    })
}

/// 把登记时的初始设置贴到实例设置上（只动这两个键，其余保持稀疏缺省）。
fn apply_settings(config: &mut InstanceConfig, settings: &InstanceSettings) {
    if let Some(isolation) = settings.isolation {
        config.isolation = Some(isolation);
    }
    if settings.game_dir.is_some() {
        config.game_dir = settings.game_dir.clone();
    }
}
