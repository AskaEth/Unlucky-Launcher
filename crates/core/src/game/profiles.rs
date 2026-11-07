//! 实例清单（`profiles.json`）模型：稳定 UUID + 实例档案。
//!
//! 依据：`DevDocs/PlanDoc/04-data-and-config.md` §4（实例模型预告）、§2（`.ULL/config/profiles.json`）。
//!
//! 两条硬约束（来自 analyse/01 §7.6 的教训）：
//! 1. **实例 id 用稳定 UUID，不用路径**——路径会变（用户挪目录、迁移数据目录），
//!    而 id 一旦写进档案就不能变；
//! 2. 档案只存**绑定关系**（版本 id、独立目录、Java 覆盖），不存派生的绝对路径结论；
//!    全局/实例/临时三层设置的合并归 `core::setting`，这里不重复。
//!
//! 落盘归 `core::setting` 的原子写 + schema 迁移（M12），本模块只提供模型与不变量。

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

/// `profiles.json` 的 schema 版本（迁移链由 `core::setting` 驱动）。
pub const PROFILE_SCHEMA_VERSION: u32 = 1;

/// 实例 id：UUID 形状的稳定标识。
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProfileId(String);

impl ProfileId {
    /// 生成一个新 id（v4 形状）。
    ///
    /// **不是密码学随机**：由纳秒时间戳 + 进程 id + 进程内计数器经 SplitMix64 混合而来，
    /// 足以保证单机唯一；删除实例后不复用即可。真要密码学随机，阶段 B 引 `uuid` crate。
    pub fn generate() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut seed = nanos ^ u64::from(std::process::id()).rotate_left(17) ^ seq.rotate_left(41);

        let mut bytes = [0u8; 16];
        for chunk in bytes.chunks_mut(8) {
            seed = split_mix64(seed);
            let source = seed.to_le_bytes();
            chunk.copy_from_slice(&source[..chunk.len()]);
        }
        // RFC 4122 v4 形状：版本位 + 变体位。
        bytes[6] = (bytes[6] & 0x0F) | 0x40;
        bytes[8] = (bytes[8] & 0x3F) | 0x80;

        let mut hex = String::with_capacity(36);
        for (index, byte) in bytes.iter().enumerate() {
            if matches!(index, 4 | 6 | 8 | 10) {
                hex.push('-');
            }
            hex.push_str(&format!("{byte:02x}"));
        }
        Self(hex)
    }

    /// 从字符串构造（不校验，用于读回历史档案；校验用 [`ProfileId::is_uuid_shaped`]）。
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    /// 字符串视图。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 是否是 UUID 形状（`8-4-4-4-12` 十六进制）。
    pub fn is_uuid_shaped(&self) -> bool {
        let bytes = self.0.as_bytes();
        if bytes.len() != 36 {
            return false;
        }
        bytes.iter().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => *byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
    }
}

fn split_mix64(mut state: u64) -> u64 {
    state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl fmt::Display for ProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for ProfileId {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_owned()))
    }
}

impl From<String> for ProfileId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// 一个实例档案。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InstanceProfile {
    /// 稳定 id。
    pub id: ProfileId,
    /// 展示名。
    #[serde(default)]
    pub name: String,
    /// 绑定的版本 id（`versions/<id>/`）。
    #[serde(default)]
    pub version_id: String,
    /// 独立游戏目录；`None` 表示使用全局 `.minecraft`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_dir: Option<PathBuf>,
    /// 工作目录覆盖（HMCL 的"版本隔离"）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub running_dir: Option<PathBuf>,
    /// Java 路径覆盖。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_path: Option<PathBuf>,
    /// 创建时间（RFC-3339 字符串，由调用方填写）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// 最近启动时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_played_at: Option<String>,
    /// 未识别字段原样保留（向前兼容）。
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl InstanceProfile {
    /// 建一个最小档案（新实例默认值）。
    pub fn new(name: impl Into<String>, version_id: impl Into<String>) -> Self {
        Self {
            id: ProfileId::generate(),
            name: name.into(),
            version_id: version_id.into(),
            ..Self::default()
        }
    }
}

/// `profiles.json` 的内容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileList {
    /// schema 版本。
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// 档案列表（顺序即 UI 顺序）。
    #[serde(default)]
    pub profiles: Vec<InstanceProfile>,
}

fn default_schema_version() -> u32 {
    PROFILE_SCHEMA_VERSION
}

impl Default for ProfileList {
    fn default() -> Self {
        Self {
            schema_version: PROFILE_SCHEMA_VERSION,
            profiles: Vec::new(),
        }
    }
}

impl ProfileList {
    /// 按 id 查。
    pub fn get(&self, id: &ProfileId) -> Option<&InstanceProfile> {
        self.profiles.iter().find(|p| &p.id == id)
    }

    /// 按 id 改。
    pub fn get_mut(&mut self, id: &ProfileId) -> Option<&mut InstanceProfile> {
        self.profiles.iter_mut().find(|p| &p.id == id)
    }

    /// 插入或替换同 id 档案，返回被替换的旧档案。
    pub fn upsert(&mut self, profile: InstanceProfile) -> Option<InstanceProfile> {
        match self.profiles.iter().position(|p| p.id == profile.id) {
            Some(index) => Some(std::mem::replace(&mut self.profiles[index], profile)),
            None => {
                self.profiles.push(profile);
                None
            }
        }
    }

    /// 删除档案，返回被删的档案。
    pub fn remove(&mut self, id: &ProfileId) -> Option<InstanceProfile> {
        let index = self.profiles.iter().position(|p| &p.id == id)?;
        Some(self.profiles.remove(index))
    }

    /// 绑定到某版本的档案（同一版本可以有多个实例：隔离目录不同）。
    pub fn for_version(&self, version_id: &str) -> Vec<&InstanceProfile> {
        self.profiles
            .iter()
            .filter(|p| p.version_id == version_id)
            .collect()
    }

    /// 档案数。
    pub fn len(&self) -> usize {
        self.profiles.len()
    }

    /// 是否没有档案。
    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }

    /// 解析 `profiles.json`。
    ///
    /// # Errors
    ///
    /// JSON 非法时返回 [`super::error::GameError::Json`]。
    pub fn from_json_str(json: &str) -> Result<Self, super::error::GameError> {
        serde_json::from_str(json).map_err(|e| super::error::GameError::json("profiles", e))
    }

    /// 序列化（格式化输出，便于用户手改与 diff）。
    ///
    /// # Errors
    ///
    /// 序列化失败时返回 [`super::error::GameError::Json`]。
    pub fn to_json_string(&self) -> Result<String, super::error::GameError> {
        serde_json::to_string_pretty(self).map_err(|e| super::error::GameError::json("profiles", e))
    }
}
