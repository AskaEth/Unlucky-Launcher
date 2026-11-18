//! 下载引擎的请求/结果类型：资源类型、摘要、请求、候选源、结果。
//!
//! 调用方（game / launch / agent）只需要描述「逻辑资源 + 落盘路径 + 期望校验」，
//! 镜像展开、地区模式、健康度排序、重试、续传、分块全部由引擎内部完成。

use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// 资源类型。
///
/// 同时用作三件事：健康度统计的第二维度（`源 × 资源类型`）、分块下载的准入条件、
/// 进度事件的分类标签。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    /// 资源对象（`assets/objects/<xx>/<hash>`），海量小文件。
    Asset,
    /// 库文件（MB 级）。
    Library,
    /// 客户端 jar（MB~几十 MB）。
    ClientJar,
    /// 版本清单 / 版本 JSON（强时效，短 TTL）。
    VersionManifest,
    /// 资源索引 `assets/indexes/<id>.json`。
    AssetIndex,
    /// JDK 运行时（~180MB），分块下载的主力场景。
    JavaRuntime,
    /// GitHub release 产物（自更新 / mod）。
    GitHubRelease,
    /// mod 文件（CurseForge / Modrinth）。
    ModFile,
    /// 其它。
    Other,
}

impl ResourceKind {
    /// 稳定字符串 id（持久化与日志用，**不要改名**）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Asset => "asset",
            Self::Library => "library",
            Self::ClientJar => "client_jar",
            Self::VersionManifest => "version_manifest",
            Self::AssetIndex => "asset_index",
            Self::JavaRuntime => "java_runtime",
            Self::GitHubRelease => "github_release",
            Self::ModFile => "mod_file",
            Self::Other => "other",
        }
    }

    /// 是否默认走多连接分块下载（大文件）。
    #[must_use]
    pub const fn prefers_chunked(self) -> bool {
        matches!(self, Self::JavaRuntime | Self::ModFile)
    }

    /// 是否属于「小文件」：小文件不开分块（续传开销大于收益）。
    #[must_use]
    pub const fn is_small(self) -> bool {
        matches!(
            self,
            Self::Asset | Self::VersionManifest | Self::AssetIndex | Self::Other
        )
    }
}

/// 摘要算法。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HashAlgo {
    /// SHA-1（Mojang 全线使用）。
    Sha1,
    /// SHA-256（自更新产物与部分镜像）。
    Sha256,
}

impl HashAlgo {
    /// 稳定字符串 id（缓存目录名与索引键用）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sha1 => "sha1",
            Self::Sha256 => "sha256",
        }
    }

    /// 十六进制摘要的字符数。
    #[must_use]
    pub const fn hex_len(self) -> usize {
        match self {
            Self::Sha1 => 40,
            Self::Sha256 => 64,
        }
    }
}

/// 期望校验值。构造时自动归一化为小写十六进制。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Integrity {
    /// 算法。
    pub algo: HashAlgo,
    /// 小写十六进制摘要。
    pub hex: String,
}

impl Integrity {
    /// 构造 SHA-1 校验值。
    #[must_use]
    pub fn sha1(hex: impl Into<String>) -> Self {
        Self {
            algo: HashAlgo::Sha1,
            hex: hex.into().trim().to_ascii_lowercase(),
        }
    }

    /// 构造 SHA-256 校验值。
    #[must_use]
    pub fn sha256(hex: impl Into<String>) -> Self {
        Self {
            algo: HashAlgo::Sha256,
            hex: hex.into().trim().to_ascii_lowercase(),
        }
    }

    /// 长度与字符集是否合法（不合法时调用方应显式传 `None`，别让引擎白跑一趟）。
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        self.hex.len() == self.algo.hex_len() && self.hex.bytes().all(|b| b.is_ascii_hexdigit())
    }

    /// 给日志/事件用的短前缀（前 8 位）。
    #[must_use]
    pub fn short_prefix(&self) -> &str {
        let end = self.hex.len().min(8);
        &self.hex[..end]
    }
}

/// 断点续传策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResumePolicy {
    /// 保守判定（默认）：长度、ETag/Last-Modified 都对得上才续传（承 HMCL `HttpResumeContext`）。
    #[default]
    Conservative,
    /// 从不续传（强时效资源、或镜像对 Range 支持可疑时）。
    Never,
}

/// 单次下载的关联 id（进度事件靠它归并）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DownloadId(pub u64);

impl DownloadId {
    /// 进程内单调递增的 id。
    #[must_use]
    pub fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }

    /// 原始数值。
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

impl fmt::Display for DownloadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// 一个下载请求：逻辑资源描述，不含任何镜像/重试策略。
#[derive(Debug, Clone)]
pub struct DownloadRequest {
    /// 资源类型。
    pub kind: ResourceKind,
    /// **逻辑 URL（官方源）**。镜像按镜像表 + 地区模式展开成候选序列。
    pub url: String,
    /// 最终落盘路径（父目录自动创建）。
    pub dest: PathBuf,
    /// 期望校验值；`None` 表示只能靠长度兜底（不推荐，清单类资源必须带）。
    pub integrity: Option<Integrity>,
    /// 期望大小（进度预算 + 分块规划）。
    pub expected_size: Option<u64>,
    /// 续传策略。
    pub resume: ResumePolicy,
}

impl DownloadRequest {
    /// 构造请求（默认保守续传、无校验）。
    #[must_use]
    pub fn new(kind: ResourceKind, url: impl Into<String>, dest: impl Into<PathBuf>) -> Self {
        Self {
            kind,
            url: url.into(),
            dest: dest.into(),
            integrity: None,
            expected_size: None,
            resume: ResumePolicy::Conservative,
        }
    }

    /// 设置 SHA-1 校验。
    #[must_use]
    pub fn with_sha1(mut self, hex: impl Into<String>) -> Self {
        self.integrity = Some(Integrity::sha1(hex));
        self
    }

    /// 设置 SHA-256 校验。
    #[must_use]
    pub fn with_sha256(mut self, hex: impl Into<String>) -> Self {
        self.integrity = Some(Integrity::sha256(hex));
        self
    }

    /// 设置期望大小。
    #[must_use]
    pub fn with_size(mut self, size: u64) -> Self {
        self.expected_size = Some(size);
        self
    }

    /// 设置续传策略。
    #[must_use]
    pub fn with_resume(mut self, resume: ResumePolicy) -> Self {
        self.resume = resume;
        self
    }

    /// 请求本身是否自洽（运行期挡住明显错误的请求）。
    pub fn validate(&self) -> Result<(), super::error::DownloadError> {
        use super::error::{DownloadError, DownloadErrorKind};
        if self.url.is_empty() {
            return Err(DownloadError::new(
                DownloadErrorKind::InvalidRequest,
                "url 为空",
            ));
        }
        if !(self.url.starts_with("http://") || self.url.starts_with("https://")) {
            return Err(DownloadError::new(
                DownloadErrorKind::InvalidRequest,
                format!("仅支持 http(s) URL：{}", self.url),
            ));
        }
        if let Some(integrity) = &self.integrity {
            if !integrity.is_well_formed() {
                return Err(DownloadError::new(
                    DownloadErrorKind::InvalidRequest,
                    format!("校验值不合法：{}", integrity.hex),
                ));
            }
        }
        Ok(())
    }
}

/// 一个候选源（镜像展开产物）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// 真正要请求的 URL。
    pub url: String,
    /// 源标识（`official` / `bmclapi` / `tuna` / `gh_proxy` …），健康度按此归并。
    pub source: String,
}

impl Candidate {
    /// 构造候选。
    #[must_use]
    pub fn new(source: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            url: url.into(),
        }
    }
}

/// 官方源候选的固定 `source` 标识。
pub const SOURCE_OFFICIAL: &str = "official";

/// 下载成功的结果。
#[derive(Debug, Clone)]
pub struct DownloadOutcome {
    /// 关联 id。
    pub id: DownloadId,
    /// 落盘路径。
    pub path: PathBuf,
    /// 实际字节数（续传时含已存在的部分）。
    pub bytes: u64,
    /// 是否零网络命中内容寻址缓存。
    pub from_cache: bool,
    /// 实际成功的源。
    pub source: String,
    /// 续传起点（0 = 从头下载）。
    pub resumed_from: u64,
    /// 耗时。
    pub elapsed: Duration,
}
