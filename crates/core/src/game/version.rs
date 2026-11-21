//! 版本清单（`version_manifest_v2.json`）模型、分类与合并。
//!
//! 设计依据：`DevDocs/analyse/03-version-download.md` §2（清单处理）、§2.4（三层分类）、
//! §2.5（排序与相等）、§2.2（v2 的 `sha1`/`size` 校验字段）。
//!
//! 与 HMCL 的差异（有意为之）：
//! - **直接用 v2 端点**并保留 `sha1`/`size`，让"清单被镜像篡改/截断"在下载阶段就暴露；
//! - 分类枚举保留 `pending`/`unobfuscated`/`modified`，不再压进 `OLD` 桶后又让过滤器不一致；
//! - 未知 `type` 值不丢弃整条记录，落到 [`ReleaseType::Unknown`]（对应 HMCL 的 null）。

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::error::GameError;

/// Mojang 官方清单 v2 端点（带 `sha1`/`size`）。
pub const VERSION_MANIFEST_V2_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

/// 版本 JSON 里 `type` 字段的取值，见 analyse/03 §2.4。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum ReleaseType {
    /// 正式版。
    Release,
    /// 快照（含愚人节、`pending` 之外的实验构建）。
    Snapshot,
    /// 远古 beta。
    OldBeta,
    /// 远古 alpha（含 classic、`rd-`/`inf-` 系列归此）。
    OldAlpha,
    /// `pending`：已推送 JSON 但未正式发布的构建。
    Pending,
    /// `_unobfuscated` 形式的反混淆构建（官方 CDN 有、普通清单没有）。
    Unobfuscated,
    /// 第三方/魔改清单自定义分类。
    Modified,
    /// 未知取值——保留记录但无法分类（HMCL 里是 `null`）。
    #[default]
    Unknown,
}

impl ReleaseType {
    /// 从 JSON 原始字符串解析（未知值落到 `Unknown`，不报错）。
    pub fn from_json_str(raw: &str) -> Self {
        match raw {
            "release" => Self::Release,
            "snapshot" => Self::Snapshot,
            "old_beta" => Self::OldBeta,
            "old_alpha" => Self::OldAlpha,
            "pending" => Self::Pending,
            "unobfuscated" => Self::Unobfuscated,
            "modified" => Self::Modified,
            _ => Self::Unknown,
        }
    }

    /// 回到 JSON 原始字符串。
    pub fn as_json_str(self) -> &'static str {
        match self {
            Self::Release => "release",
            Self::Snapshot => "snapshot",
            Self::OldBeta => "old_beta",
            Self::OldAlpha => "old_alpha",
            Self::Pending => "pending",
            Self::Unobfuscated => "unobfuscated",
            Self::Modified => "modified",
            Self::Unknown => "unknown",
        }
    }

    /// UI 侧的归类（analyse/03 §2.4 的五态，去掉 `uncategorized` 之外的分歧）。
    pub fn category(self) -> VersionCategory {
        match self {
            Self::Release => VersionCategory::Release,
            Self::Snapshot => VersionCategory::Snapshot,
            Self::OldBeta | Self::OldAlpha | Self::Modified => VersionCategory::Old,
            Self::Pending => VersionCategory::Pending,
            Self::Unobfuscated => VersionCategory::Unobfuscated,
            Self::Unknown => VersionCategory::Uncategorized,
        }
    }
}

impl fmt::Display for ReleaseType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_json_str())
    }
}

impl Serialize for ReleaseType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_json_str())
    }
}

impl<'de> Deserialize<'de> for ReleaseType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(Self::from_json_str(&raw))
    }
}

/// 版本列表的 UI 归类（对应 HMCL `ComponentRemoteVersion.Type`，见 analyse/03 §2.4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VersionCategory {
    /// 正式版。
    Release,
    /// 快照。
    Snapshot,
    /// 远古版（old_beta / old_alpha / modified）。
    Old,
    /// 未正式发布的 pending 构建。
    Pending,
    /// 反混淆构建。
    Unobfuscated,
    /// 无法归类。
    Uncategorized,
}

/// 清单里的 `latest` 对象。Mojang 用它标注"当前最新正式版/快照"，与条目列表无关。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatestVersions {
    /// 最新正式版 id。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
    /// 最新快照 id。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,
}

/// 版本清单里的一条记录。
///
/// `sha1`/`size` 只在 v2 端点出现（v1 与内嵌 unlisted 清单没有），因此都是可选的。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteVersion {
    /// 版本 id（`1.20.1`、`12w01a`、`a1.1.1`…）。
    pub id: String,
    /// 分类。
    #[serde(rename = "type", default)]
    pub version_type: ReleaseType,
    /// 该版本 JSON 的下载地址。
    pub url: String,
    /// 该版本 JSON 的最后修改时间（ISO-8601 字符串）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    /// 该版本的发布时刻（ISO-8601 字符串，排序键）。
    #[serde(
        rename = "releaseTime",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub release_time: Option<String>,
    /// 版本 JSON 的 SHA-1（v2 才有）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    /// 版本 JSON 的字节数（v2 才有）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

impl RemoteVersion {
    /// 该条目是否带 v2 的校验信息。
    pub fn is_verifiable(&self) -> bool {
        self.sha1.is_some()
    }
}

/// 一份版本清单（官方 `version_manifest_v2.json` 或内嵌 unlisted 清单）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionList {
    /// `latest` 对象；unlisted 清单没有。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest: Option<LatestVersions>,
    /// 条目列表。
    #[serde(default)]
    pub versions: Vec<RemoteVersion>,
}

impl VersionList {
    /// 解析清单 JSON。
    pub fn from_json_str(json: &str) -> Result<Self, GameError> {
        serde_json::from_str(json).map_err(|e| GameError::json("version_manifest", e))
    }

    /// 序列化回 JSON（便于缓存落盘/测试对拍）。
    pub fn to_json_string(&self) -> Result<String, GameError> {
        serde_json::to_string(self).map_err(|e| GameError::json("version_manifest", e))
    }

    /// 按 id 查条目。
    pub fn find(&self, id: &str) -> Option<&RemoteVersion> {
        self.versions.iter().find(|v| v.id == id)
    }

    /// 已存在的版本 id 数。
    pub fn len(&self) -> usize {
        self.versions.len()
    }

    /// 是否为空清单。
    pub fn is_empty(&self) -> bool {
        self.versions.is_empty()
    }

    /// 合并补充清单：**同 id 时先插入者胜**（analyse/03 §2.5 的 `TreeSet` 语义）。
    ///
    /// 调用方应先合并 unlisted 再合并官方清单，这样社区清单里那些"官方没有/官方 URL 更差"
    /// 的条目会被保留。返回新插入的条目数。
    pub fn merge_missing(&mut self, extra: impl IntoIterator<Item = RemoteVersion>) -> usize {
        let mut added = 0;
        for item in extra {
            if self.find(&item.id).is_some() {
                continue;
            }
            self.versions.push(item);
            added += 1;
        }
        added
    }

    /// 按发布时间**倒序**排列；同时间再按 id 倒序（analyse/03 §2.5）。
    pub fn sort_newest_first(&mut self) {
        self.versions.sort_by(|a, b| {
            let at = a.release_time.as_deref().unwrap_or_default();
            let bt = b.release_time.as_deref().unwrap_or_default();
            bt.cmp(at).then_with(|| b.id.cmp(&a.id))
        });
    }

    /// 按 UI 归类过滤（保持当前顺序）。
    pub fn filter_category(&self, category: VersionCategory) -> Vec<&RemoteVersion> {
        self.versions
            .iter()
            .filter(|v| v.version_type.category() == category)
            .collect()
    }
}
