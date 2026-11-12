//! 发布清单模型：一个渠道一份 JSON，列出该渠道已知的所有发布与各平台的产物。
//!
//! 字段形状取自 `DevDocs/analyse/07-settings-packaging-update-p2.md` §6.2 的扩展清单，
//! 但**只留与更新源无关的部分**：清单里没有任何 CDN 常量，`url` 是字符串，怎么解析由
//! [`crate::update::UpdateSource`] 的实现决定。
//!
//! ```json
//! {
//!   "channel": "stable",
//!   "releases": [
//!     {
//!       "version": "0.3.0",
//!       "channel": "stable",
//!       "released_at": "2026-01-02T03:04:05Z",
//!       "notes": "修掉启动崩溃",
//!       "force": false,
//!       "min_supported": "0.2.0",
//!       "artifacts": [
//!         {"target": "win-x64", "format": "portable-zip", "url": "ull-0.3.0-win-x64.zip",
//!          "size": 22100000, "sha256": "…"}
//!       ]
//!     }
//!   ]
//! }
//! ```
//!
//! 读的方向刻意严格（`deny_unknown_fields` + 逐字段校验）：清单是我们自己发的东西，
//! 拼错的字段名宁可在 CI 上炸掉，也别在用户机器上变成「查不到更新」。
//!
//! # 校验规则（`from_json` 会一次性查完）
//!
//! * `version` / `min_supported` 必须是合法 semver；
//! * 同一份清单里 `version` 不许重复（重复意味着「升到哪个」没有确定答案）；
//! * `size` 必须 > 0（0 字节的包只可能是发布脚本写漏了）；
//! * 每个产物必须带 `sha256` 或 `sha1`，且长度/字符集合法（两者都带时以 `sha256` 为准）；
//! * `url` 不许为空。
//!
//! `released_at` / `notes` 只做展示，不解析成时间类型（core 不引时间库，排序只看版本号）。

use serde::Deserialize;

use crate::download::Integrity;

use super::channel::Channel;
use super::error::{UpdateError, UpdateErrorKind};
use super::target::{PackageFormat, Target};
use super::version::Version;

/// 一个渠道的发布清单。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseManifest {
    /// 这份清单属于哪个渠道。
    pub channel: Channel,
    /// 已发布的版本（顺序无所谓：选版本的是 checker，不是清单的书写顺序）。
    pub releases: Vec<Release>,
}

impl ReleaseManifest {
    /// 解析清单 JSON 并做完整校验。
    ///
    /// # Errors
    ///
    /// JSON 语法错、字段名拼错、版本号/摘要/长度不合法都返回 [`UpdateError`]，
    /// 分类是 [`UpdateErrorKind::InvalidManifest`] 或 [`UpdateErrorKind::InvalidVersion`]。
    pub fn from_json(text: &str) -> Result<Self, UpdateError> {
        let raw: RawManifest = serde_json::from_str(text).map_err(|error| {
            UpdateError::invalid_manifest(format!("清单 JSON 解析失败：{error}"))
        })?;
        let channel = Channel::parse(&raw.channel)?;
        let mut releases = Vec::with_capacity(raw.releases.len());
        for raw_release in raw.releases {
            let release = Release::from_raw(channel, raw_release)?;
            if releases
                .iter()
                .any(|existing: &Release| existing.version == release.version)
            {
                return Err(UpdateError::invalid_manifest(format!(
                    "清单 {} 里版本 {} 出现多次",
                    channel, release.version
                )));
            }
            releases.push(release);
        }
        Ok(Self { channel, releases })
    }

    /// 清单里该目标平台的最高可用版本（不看渠道准入，纯查表；选择逻辑在 checker）。
    #[must_use]
    pub fn newest_artifact_for(&self, target: Target) -> Option<(&Release, &Artifact)> {
        self.releases
            .iter()
            .filter_map(|release| {
                release
                    .artifact_for(target)
                    .map(|artifact| (release, artifact))
            })
            .max_by(|left, right| left.0.version.cmp(&right.0.version))
    }
}

/// 一次发布。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// 版本号。
    pub version: Version,
    /// 这次发布自报的渠道（可以比清单渠道更稳，例如 beta 清单里放一个 stable 补丁版）。
    pub channel: Channel,
    /// 发布时间（RFC3339 字符串，仅展示）。
    pub released_at: Option<String>,
    /// 更新说明（纯文本，仅展示）。
    pub notes: Option<String>,
    /// 是否强制更新（不看版本号也得升）。
    pub force: bool,
    /// 客户端最低支持版本：当前版本低于它就必须升（发布侧收回老版本的支持）。
    pub min_supported: Option<Version>,
    /// 各平台的产物。清单里的顺序 = 同一目标多形态时的优先级。
    pub artifacts: Vec<Artifact>,
}

impl Release {
    /// 该目标平台的首选产物（清单顺序即优先级；一个都没有就返回 `None`）。
    #[must_use]
    pub fn artifact_for(&self, target: Target) -> Option<&Artifact> {
        self.artifacts
            .iter()
            .find(|artifact| artifact.target == target)
    }

    /// 当前版本是否低于本发布要求的最低支持版本。
    #[must_use]
    pub fn below_min_supported(&self, current: &Version) -> bool {
        self.min_supported
            .as_ref()
            .is_some_and(|minimum| current < minimum)
    }

    fn from_raw(manifest_channel: Channel, raw: RawRelease) -> Result<Self, UpdateError> {
        let version = Version::parse(&raw.version)?;
        let channel = match &raw.channel {
            Some(text) => Channel::parse(text)?,
            None => manifest_channel,
        };
        let min_supported = raw
            .min_supported
            .as_deref()
            .map(Version::parse)
            .transpose()?;
        let mut artifacts = Vec::with_capacity(raw.artifacts.len());
        for raw_artifact in raw.artifacts {
            artifacts.push(Artifact::from_raw(&version, raw_artifact)?);
        }
        Ok(Self {
            version,
            channel,
            released_at: raw.released_at,
            notes: raw.notes,
            force: raw.force,
            min_supported,
            artifacts,
        })
    }
}

/// 一份可下载的产物。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    /// 目标平台。
    pub target: Target,
    /// 包形态。
    pub format: PackageFormat,
    /// 更新源内部的地址（怎么解释由源实现决定：相对路径 / `file://` / `https://`）。
    pub url: String,
    /// 字节数（下载后必须**恰好**这么多，短了就是截断）。
    pub size: u64,
    /// 期望摘要（已归一化为小写十六进制）。
    pub integrity: Integrity,
}

impl Artifact {
    fn from_raw(version: &Version, raw: RawArtifact) -> Result<Self, UpdateError> {
        let target = Target::parse(&raw.target).ok_or_else(|| {
            UpdateError::invalid_manifest(format!(
                "版本 {version} 的产物 target `{}` 无法识别（形如 win-x64 / mac-arm64 / linux-x64）",
                raw.target
            ))
        })?;
        let format = PackageFormat::parse(&raw.format).ok_or_else(|| {
            UpdateError::invalid_manifest(format!(
                "版本 {version} 的产物 format `{}` 无法识别",
                raw.format
            ))
        })?;
        if raw.url.trim().is_empty() {
            return Err(UpdateError::invalid_manifest(format!(
                "版本 {version}（{target}）的产物 url 为空"
            )));
        }
        if raw.size == 0 {
            return Err(UpdateError::invalid_manifest(format!(
                "版本 {version}（{target}）的产物 size 为 0"
            )));
        }
        let integrity = match (raw.sha256, raw.sha1) {
            (Some(hex), _) => Integrity::sha256(hex),
            (None, Some(hex)) => Integrity::sha1(hex),
            (None, None) => {
                return Err(UpdateError::invalid_manifest(format!(
                    "版本 {version}（{target}）的产物既没有 sha256 也没有 sha1"
                )));
            }
        };
        if !integrity.is_well_formed() {
            return Err(UpdateError::new(
                UpdateErrorKind::InvalidManifest,
                format!(
                    "版本 {version}（{target}）的 {} 摘要格式不合法（应为 {} 位十六进制）",
                    integrity.algo.as_str(),
                    integrity.algo.hex_len()
                ),
            ));
        }
        Ok(Self {
            target,
            format,
            url: raw.url,
            size: raw.size,
            integrity,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    channel: String,
    #[serde(default)]
    releases: Vec<RawRelease>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRelease {
    version: String,
    channel: Option<String>,
    released_at: Option<String>,
    notes: Option<String>,
    #[serde(default)]
    force: bool,
    min_supported: Option<String>,
    #[serde(default)]
    artifacts: Vec<RawArtifact>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawArtifact {
    target: String,
    format: String,
    url: String,
    size: u64,
    sha256: Option<String>,
    sha1: Option<String>,
}
