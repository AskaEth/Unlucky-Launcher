//! 实例登记：`config/profiles.json` + `config/instances/<id>.json`（稀疏、幂等）。
//!
//! 依据：`DevDocs/PlanDoc/04-data-and-config.md` §4（D20 实例模型）、§2（`.ULL/config/`）、
//! `DevDocs/PlanDoc/research/04-launcher-configs.md` §5（实例键草案）。
//!
//! # 为什么是两份文件
//!
//! - `profiles.json` = **档案列表**（UI 顺序、展示名、绑定版本），是"有哪些实例"的真相源；
//! - `instances/<id>.json` = **每实例设置**，稀疏：只写用户显式改过的键。
//!
//! §5 的关键结论：`isolation` 默认 `inherit` 且**不落盘**——这样改全局模式对存量实例立即生效，
//! 直接避开 PCL2"默认值被固化成实例值"的缺陷。因此 [`InstanceConfig::new`] 的
//! `isolation` 是 `None`（序列化时整个键消失）；档位的**派生与切换检测**在
//! [`crate::game::isolation`]（M30 落地），本模块只管结构与落盘。
//!
//! # 幂等
//!
//! 同一版本重复 `install`：按 `version_id` + 展示名复用已有档案，返回 `created = false`，
//! 不产生第二个实例、不改已有实例的 id（id 一旦写进档案就是稳定的）。登记本身（含
//! [`InstanceSettings`] 与 `register_with`）在同目录的 `register` 子模块里。

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::super::error::GameError;
use super::super::isolation::{IsolationMode, ModeChange, ModeChangeWarning};
use super::super::launch_overrides::LaunchOverrides;
use super::super::profiles::ProfileList;
use crate::platform::atomic::write_atomic;
use crate::platform::layout::LauncherLayout;

mod register;

pub use register::{register, register_with, InstanceSettings, Registration};

/// 每实例设置文件的 schema 版本。
///
/// 与 `setting::instance_keys::instance_version` 的 minor 对应：D20 加了 `lastModeChange`，
/// 老文件缺这个键即「没切换过」，不需要迁移步骤。
pub const INSTANCE_CONFIG_VERSION: u32 = 1;

/// 版本隔离档位（`isolation`）。
///
/// 缺省 `inherit` **不是**这个枚举的一个值，而是"键不存在"（[`InstanceConfig::isolation`]
/// 为 `None`）——这样稀疏序列化天然表达"跟随全局"。
///
/// 派生规则见 [`crate::game::isolation::derive_game_dir`]：`isolated` → `versions/<versionId>/`、
/// `shared` → 游戏根 `.minecraft/`、`custom` → 用户指定的 `gameDir`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Isolation {
    /// 独立游戏目录（派生 `versions/<versionId>/`）。
    #[serde(alias = "per_version")]
    Isolated,
    /// 与全局共用 `.minecraft`（HMCL 传统形态）。
    Shared,
    /// 用户指定的目录（此时 `gameDir` 必须有值）。
    Custom,
}

impl Isolation {
    /// 稳定字符串 id。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Isolated => "isolated",
            Self::Shared => "shared",
            Self::Custom => "custom",
        }
    }

    /// 从字符串解析（未知值返回 `None`，由调用方决定是报错还是忽略）。
    ///
    /// `per_version` 是 M12 键草案里的旧字面量，读兼容映射到 `isolated`（语义相同）。
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "isolated" | "per_version" => Some(Self::Isolated),
            "shared" => Some(Self::Shared),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }
}

/// 实例可覆盖的设置键（enum 白名单）。
///
/// research/04 §5 的教训：**不要用字符串集合**，拼错会静默不生效；这里用枚举，
/// 后续 M28 接语义时编译器会逼着把每个键处理干净。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OverrideKey {
    /// 最大堆内存。
    MaxMemory,
    /// Java 运行时选择。
    JavaType,
    /// 工作目录覆盖。
    RunningDirectory,
}

/// 每实例设置（`config/instances/<id>.json`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceConfig {
    /// schema 版本（迁移链入口）。
    #[serde(default = "default_version")]
    pub schema_version: u32,
    /// 实例 id（稳定 UUID，与文件名一致）。
    pub id: String,
    /// 绑定的版本 id（`versions/<versionId>/`）。
    pub version_id: String,
    /// 隔离档位；`None` = `inherit`（跟随全局，不落盘）。
    ///
    /// 显式写 `"inherit"` 也照读（归一成 `None`）——见
    /// [`deserialize_isolation`]：schema 白名单允许用户/AI 写这个字面量，
    /// 读路径必须闭环，否则「显式写回默认值」的实例文件会让启动直接报 unknown variant。
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_isolation"
    )]
    pub isolation: Option<Isolation>,
    /// 自定义游戏目录（仅 `isolation = custom` 时有值）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_dir: Option<PathBuf>,
    /// 该实例覆盖了哪些设置键。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<OverrideKey>,
    /// 实例级启动参数覆盖（M157）：JVM/游戏参数、内存、Java、图形后端五项，逐项回落。
    ///
    /// 空段（全 `None`/空白）**不落盘**——与 `isolation` 同一套稀疏语义，清空输入框即「跟随全局」。
    /// 回落规则与生效值见 [`crate::game::launch_overrides`]。
    #[serde(default, skip_serializing_if = "LaunchOverrides::is_empty")]
    pub launch: LaunchOverrides,
    /// 上次隔离档位切换的记录（`lastModeChange`；没切过就不落盘）。
    ///
    /// 由 [`crate::game::isolation::observe_mode`] 维护，只记事实（含双侧探测），**不迁移数据**。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_mode_change: Option<ModeChange>,
    /// 未识别字段原样保留（向前兼容）。
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

fn default_version() -> u32 {
    INSTANCE_CONFIG_VERSION
}

/// `isolation` 的反序列化：显式 `"inherit"` 归一成「键不存在」（`None`）。
///
/// 为什么需要：`setting::instance_keys` 的白名单里有 `inherit`（读/显示要用），
/// 用户或 AI 把档位写回默认值时就会落一个 `"isolation": "inherit"`；从枚举角度它
/// 是**没有变体**的（`inherit` 的真相是稀疏 —— 键缺失），所以在这里折叠成 `None`，
/// 顺带把 `per_version` 旧字面量也接住（`Isolation::parse` 负责映射）。
fn deserialize_isolation<'de, D>(deserializer: D) -> Result<Option<Isolation>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    match Option::<String>::deserialize(deserializer)? {
        None => Ok(None),
        Some(text) if text.trim() == "inherit" => Ok(None),
        Some(text) => Isolation::parse(&text).map(Some).ok_or_else(|| {
            serde::de::Error::unknown_variant(&text, &["inherit", "isolated", "shared", "custom"])
        }),
    }
}

impl InstanceConfig {
    /// 新建一份最小设置：`isolation` 留空（= inherit，不落盘）。
    #[must_use]
    pub fn new(id: impl Into<String>, version_id: impl Into<String>) -> Self {
        Self {
            schema_version: INSTANCE_CONFIG_VERSION,
            id: id.into(),
            version_id: version_id.into(),
            isolation: None,
            game_dir: None,
            overrides: Vec::new(),
            launch: LaunchOverrides::default(),
            last_mode_change: None,
            extra: BTreeMap::new(),
        }
    }

    /// 是否跟随全局隔离模式（`isolation` 键缺失）。
    #[must_use]
    pub fn inherits_isolation(&self) -> bool {
        self.isolation.is_none()
    }

    /// 生效隔离档位（覆盖链第二层落到全局第一层，`inherit` 在这里被解析掉）。
    #[must_use]
    pub fn effective_isolation(&self, global: IsolationMode) -> Isolation {
        self.isolation.unwrap_or_else(|| global.as_isolation())
    }

    /// 取走待展示的切换提示（取走后 `warned` 置真，调用方负责落盘）。
    pub fn take_mode_change_warning(&mut self) -> Option<ModeChangeWarning> {
        self.last_mode_change.as_mut()?.take_warning()
    }

    /// 解析。
    ///
    /// # Errors
    ///
    /// JSON 非法时返回 [`GameError::Json`]。
    pub fn from_json_str(json: &str) -> Result<Self, GameError> {
        serde_json::from_str(json).map_err(|e| GameError::json("instance_config", e))
    }

    /// 序列化（格式化，便于用户手改与 diff）。
    ///
    /// # Errors
    ///
    /// 序列化失败时返回 [`GameError::Json`]。
    pub fn to_json_string(&self) -> Result<String, GameError> {
        serde_json::to_string_pretty(self).map_err(|e| GameError::json("instance_config", e))
    }
}

/// 读实例清单（文件缺失 = 空列表，不算错误）。
///
/// # Errors
///
/// 读盘失败、非 UTF-8 或 JSON 非法。
pub fn read_profiles(layout: &LauncherLayout) -> Result<ProfileList, GameError> {
    let path = layout.profiles_config();
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ProfileList::default());
        }
        Err(error) => return Err(GameError::io(&path, error)),
    };
    let Ok(text) = String::from_utf8(bytes) else {
        return Err(GameError::io(
            &path,
            std::io::Error::new(std::io::ErrorKind::InvalidData, "profiles.json 不是 UTF-8"),
        ));
    };
    ProfileList::from_json_str(&text)
}

/// 写实例清单（原子写）。
///
/// # Errors
///
/// 序列化失败或原子写失败。
pub fn write_profiles(layout: &LauncherLayout, profiles: &ProfileList) -> Result<(), GameError> {
    let text = profiles.to_json_string()?;
    write_atomic(&layout.profiles_config(), text.as_bytes())
        .map_err(|error| GameError::io(layout.profiles_config(), std::io::Error::other(error)))
}

/// 写每实例设置（原子写）。
///
/// # Errors
///
/// 实例 id 不合法、序列化失败或原子写失败。
pub fn write_instance_config(
    layout: &LauncherLayout,
    config: &InstanceConfig,
) -> Result<PathBuf, GameError> {
    let path = instance_config_path(layout, &config.id)?;
    let text = config.to_json_string()?;
    write_atomic(&path, text.as_bytes())
        .map_err(|error| GameError::io(&path, std::io::Error::other(error)))?;
    Ok(path)
}

/// 读每实例设置；缺失返回 `None`。
///
/// # Errors
///
/// 实例 id 不合法、读盘失败或 JSON 非法。
pub fn read_instance_config(
    layout: &LauncherLayout,
    id: &str,
) -> Result<Option<InstanceConfig>, GameError> {
    let path = instance_config_path(layout, id)?;
    match std::fs::read_to_string(&path) {
        Ok(text) => InstanceConfig::from_json_str(&text).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(GameError::io(&path, error)),
    }
}

/// `config/instances/<id>.json`（实例 id 白名单校验在布局层，挡掉 `../` 穿越）。
///
/// # Errors
///
/// 实例 id 不合法时返回 [`GameError::Io`]（`PlatformError` 没有对应变体）。
fn instance_config_path(layout: &LauncherLayout, id: &str) -> Result<PathBuf, GameError> {
    layout
        .instance_config(id)
        .map_err(|error| GameError::io(layout.config(), std::io::Error::other(error)))
}
