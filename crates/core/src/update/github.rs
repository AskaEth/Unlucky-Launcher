//! GitHub Releases 更新源（D50 定案：GitHub Releases 是唯一更新源）。
//!
//! # 地址形状（固定，不可配置）
//!
//! ```text
//! 清单   <base>/<owner>/<repo>/releases/latest/download/manifest.json
//! 产物   <base>/<owner>/<repo>/releases/latest/download/<清单里的产物文件名>
//! ```
//!
//! `releases/latest/download/<asset>` 是 GitHub 原生的「最新发布」地址：不用知道 tag、
//! 不用调 API（API 有速率限制、也要 token），发布侧只要把 `manifest.json` 和各平台产物
//! 挂在最新 release 上，客户端就能取到。清单里的 `url` 因此照 D49 的产物命名规则写成
//! **文件名**（如 `unlucky-launcher_0.6.0_windows_x64.exe`），由本源拼成上面的地址；
//! 清单若已经写了完整 `http(s)` 地址（例如临时挂到别处复测），原样使用。
//!
//! # 可配置面只有三个
//!
//! * `owner` / `repo`（默认 [`DEFAULT_OWNER`] / [`DEFAULT_REPO`]）；
//! * `base`（默认 [`DEFAULT_BASE`]；**只为测试存在**——把它指向本地假 HTTP 服务，
//!   单测就不必联网）。
//!
//! # 网络全走 `core::download`
//!
//! * **产物**（几十 MB 的包体）：整段交给 [`DownloadEngine::download`]——镜像展开
//!   （国内 gh-proxy 前缀）、地区判定、健康度排序、失败退避、断点续传、原子落盘全部复用，
//!   本源不再造一套网络栈，也不自己拼镜像地址；
//! * **清单**（几 KB 的 JSON）：用引擎的候选规划 [`GitHubReleaseSource::candidates`] +
//!   共享客户端 [`DownloadEngine::client`] 逐候选取文本，与 `core::game::java` 的
//!   「一锤子 GET」同一手法。包体才需要退避/续传/分块，为几 KB 建一条下载流水线不划算。
//!
//! 两种路径都吃到同一条镜像策略：`mirrors.json` 的前缀规则 + 地区档案里生效的
//! `gh_proxy` 前缀（[`crate::download::mirrors::MirrorTable::gh_proxy_candidates`]），
//! 国内环境下 `https://github.com/...` 会被改写成 `https://gh-proxy.com/https://github.com/...`。
//!
//! # 渠道
//!
//! `releases/latest` 只有一个，所以本源**只服务 stable**。beta/alpha/nightly 的清单枚举
//! 保留但不发布（D50「先只发 stable」），因此非 stable 渠道一律 `Ok(None)` = 没有更新——
//! 这不是错误，别让「还没开通道」变成用户机器上的红字。
//!
//! # 校验
//!
//! 本源只管「把字节放到 dest」：长度与摘要由 [`super::verify`] 统一双检（与 [`super::DirSource`]
//! 一致）。所以包体请求只带 `expected_size`（给进度预算与门户页假阳性兜底），不带摘要——
//! 摘要判据只有一处，别在这里再写一套。
//!
//! # 运行时
//!
//! [`UpdateSource`] 是同步签名，本源在方法内部用 [`crate::download::block_on`] 桥接异步引擎
//! （CLI 与 presenter 都是这么用的）。因此**不要在已有的 tokio 运行时里直接调本源的
//! `fetch_*`**：tokio 不允许嵌套 `block_on`，会 panic。更新流程请放在独立线程上跑。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::download::cache::now_ms;
use crate::download::{
    Candidate, DownloadEngine, DownloadError, DownloadErrorKind, DownloadRequest, ResourceKind,
};

use super::channel::Channel;
use super::error::{UpdateError, UpdateErrorKind};
use super::manifest::{Artifact, ReleaseManifest};
use super::source::UpdateSource;

/// 默认仓库 owner。
pub const DEFAULT_OWNER: &str = "AskaEth";
/// 默认仓库名。
pub const DEFAULT_REPO: &str = "Unlucky-Launcher";
/// GitHub 站点根地址（`base` 的默认值）。
pub const DEFAULT_BASE: &str = "https://github.com";
/// 清单文件名（挂在最新 release 的产物里，地址由 D50 定死）。
pub const MANIFEST_ASSET: &str = "manifest.json";

/// GitHub Releases 更新源。
#[derive(Debug, Clone)]
pub struct GitHubReleaseSource {
    engine: Arc<DownloadEngine>,
    owner: String,
    repo: String,
    base: String,
    id: String,
}

impl GitHubReleaseSource {
    /// 用默认仓库（[`DEFAULT_OWNER`] / [`DEFAULT_REPO`]）与 GitHub 站点根地址构造。
    #[must_use]
    pub fn new(engine: Arc<DownloadEngine>) -> Self {
        Self::with_repo(engine, DEFAULT_OWNER, DEFAULT_REPO)
    }

    /// 指定仓库构造（fork / 自建镜像仓、测试用）。
    #[must_use]
    pub fn with_repo(
        engine: Arc<DownloadEngine>,
        owner: impl Into<String>,
        repo: impl Into<String>,
    ) -> Self {
        let owner = owner.into();
        let repo = repo.into();
        let id = format!("github:{owner}/{repo}");
        Self {
            engine,
            owner,
            repo,
            base: DEFAULT_BASE.to_string(),
            id,
        }
    }

    /// 换 `base`（站点根地址，末尾斜杠会去掉）。
    ///
    /// 生产不用这个口子；单测把它指向本地假服务（`http://127.0.0.1:<port>`）以保持离线。
    #[must_use]
    pub fn with_base(mut self, base: impl Into<String>) -> Self {
        self.base = base.into().trim_end_matches('/').to_string();
        self
    }

    /// 仓库 owner。
    #[must_use]
    pub fn owner(&self) -> &str {
        &self.owner
    }

    /// 仓库名。
    #[must_use]
    pub fn repo(&self) -> &str {
        &self.repo
    }

    /// 站点根地址。
    #[must_use]
    pub fn base(&self) -> &str {
        &self.base
    }

    /// 引擎（调用方要挂进度事件时用）。
    #[must_use]
    pub fn engine(&self) -> &Arc<DownloadEngine> {
        &self.engine
    }

    /// 某渠道的清单地址；非 stable 渠道没有清单（返回 `None`）。
    #[must_use]
    pub fn manifest_url(&self, channel: Channel) -> Option<String> {
        (channel == Channel::Stable).then(|| format!("{}/{MANIFEST_ASSET}", self.download_base()))
    }

    /// 把清单里的 `url` 解析成真实下载地址。
    ///
    /// 已经是 `http(s)://` 的地址原样使用（清单可以临时指向别处）；其余一律当成
    /// 「最新 release 里的产物文件名」，拼到本源的下载根路径下。
    ///
    /// # Errors
    ///
    /// `url` 为空是 [`UpdateErrorKind::InvalidManifest`]（清单坏，不是源的问题）；
    /// 非 http(s) 的其它协议是 [`UpdateErrorKind::UnsupportedSource`]。
    pub fn artifact_url(&self, artifact: &Artifact) -> Result<String, UpdateError> {
        let raw = artifact.url.trim();
        if raw.is_empty() {
            return Err(UpdateError::invalid_manifest("产物的 url 为空"));
        }
        if raw.starts_with("http://") || raw.starts_with("https://") {
            return Ok(raw.to_string());
        }
        if raw.contains("://") {
            return Err(UpdateError::new(
                UpdateErrorKind::UnsupportedSource,
                format!("GitHub 源只认 http(s) 地址或 release 内的产物文件名：`{raw}`"),
            ));
        }
        // `./name` 与 `/name` 都当成「release 里的文件名」，别把整串拼出个双斜杠路径。
        let name = raw.trim_start_matches(['.', '/']);
        Ok(format!("{}/{name}", self.download_base()))
    }

    /// 某地址在引擎眼里的候选源序列（镜像展开 + 健康度排序，**不发请求**）。
    ///
    /// 生产调用走引擎自己的规划；这里是给「镜像策略是否生效」这类断言用的观察窗。
    #[must_use]
    pub fn candidates(&self, url: &str) -> Vec<Candidate> {
        // `dest` 只是 `DownloadRequest` 的必填字段，`plan` 不看它。
        let request = DownloadRequest::new(ResourceKind::GitHubRelease, url, PathBuf::new());
        self.engine.plan(&request, now_ms())
    }

    /// 最新的 release 产物根路径（`<base>/<owner>/<repo>/releases/latest/download`）。
    fn download_base(&self) -> String {
        format!(
            "{}/{}/{}/releases/latest/download",
            self.base, self.owner, self.repo
        )
    }

    /// 逐候选取一段文本：`Ok(None)` = 所有候选都说「这份文件不存在」。
    fn fetch_text(&self, url: &str) -> Result<Option<String>, UpdateError> {
        let candidates = self.candidates(url);
        if candidates.is_empty() {
            return Err(UpdateError::new(
                UpdateErrorKind::Network,
                format!("没有可用候选源：{url}"),
            ));
        }
        let mut failure: Option<UpdateError> = None;
        let mut all_missing = true;
        for candidate in candidates {
            match self.get_candidate_text(&candidate.url) {
                Ok(Some(text)) => return Ok(Some(text)),
                // 某个候选 404 不代表没有：镜像可能比官方慢半拍，继续问下一个。
                Ok(None) => {}
                Err(error) => {
                    all_missing = false;
                    failure = Some(error);
                }
            }
        }
        if all_missing {
            Ok(None)
        } else {
            Err(failure.unwrap_or_else(|| {
                UpdateError::new(UpdateErrorKind::Network, format!("取 {url} 失败"))
            }))
        }
    }

    /// 单个候选地址取文本；`Ok(None)` = 404/410（该候选没有这份文件）。
    fn get_candidate_text(&self, url: &str) -> Result<Option<String>, UpdateError> {
        let client = self.engine.client();
        crate::download::block_on(async {
            let response = client.get(url).send().await.map_err(|error| {
                UpdateError::new(
                    UpdateErrorKind::Network,
                    format!("取更新清单失败：{error}（{url}）"),
                )
            })?;
            let status = response.status();
            if status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::GONE {
                return Ok(None);
            }
            if !status.is_success() {
                return Err(UpdateError::new(
                    UpdateErrorKind::Network,
                    format!("更新清单返回 HTTP {status}（{url}）"),
                ));
            }
            let text = response.text().await.map_err(|error| {
                UpdateError::new(
                    UpdateErrorKind::Network,
                    format!("读更新清单响应失败：{error}（{url}）"),
                )
            })?;
            if text.trim().is_empty() {
                return Err(UpdateError::invalid_manifest(format!(
                    "清单响应为空（{url}），发布脚本可能漏传了 {MANIFEST_ASSET}"
                )));
            }
            Ok(Some(text))
        })
    }
}

impl UpdateSource for GitHubReleaseSource {
    fn source_id(&self) -> &str {
        &self.id
    }

    fn fetch_manifest(&self, channel: Channel) -> Result<Option<ReleaseManifest>, UpdateError> {
        let Some(url) = self.manifest_url(channel) else {
            return Ok(None);
        };
        let Some(text) = self.fetch_text(&url)? else {
            return Ok(None);
        };
        let manifest = ReleaseManifest::from_json(&text).map_err(|mut error| {
            error.message = format!("{}（来源 {url}）", error.message);
            error
        })?;
        if manifest.channel != channel {
            return Err(UpdateError::new(
                UpdateErrorKind::ChannelMismatch,
                format!(
                    "请求的是 {channel} 渠道，清单 {url} 自报 {}",
                    manifest.channel
                ),
            ));
        }
        Ok(Some(manifest))
    }

    fn fetch_artifact(&self, artifact: &Artifact, dest: &Path) -> Result<u64, UpdateError> {
        let url = self.artifact_url(artifact)?;
        if let Some(parent) = dest.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| UpdateError::io_at(parent, &error))?;
            }
        }
        // 摘要不在这里传：`update::verify` 统一做长度/摘要双检（trait 契约，别各写一套）。
        let request = DownloadRequest::new(ResourceKind::GitHubRelease, url.clone(), dest)
            .with_size(artifact.size);
        let outcome = crate::download::block_on(self.engine.download(request))
            .map_err(|error| download_error(&error, &url))?;
        Ok(outcome.bytes)
    }
}

/// 把下载引擎的错误翻译成自更新的错误分类（两套分类各有各的职责，别混用）。
fn download_error(error: &DownloadError, url: &str) -> UpdateError {
    let kind = match error.kind {
        DownloadErrorKind::NotFound => UpdateErrorKind::SourceMissing,
        DownloadErrorKind::Integrity => UpdateErrorKind::DigestMismatch,
        DownloadErrorKind::Io => UpdateErrorKind::Io,
        DownloadErrorKind::InvalidRequest => UpdateErrorKind::UnsupportedSource,
        DownloadErrorKind::AllSourcesFailed if all_candidates_missing(error) => {
            UpdateErrorKind::SourceMissing
        }
        DownloadErrorKind::ServerError
        | DownloadErrorKind::Timeout
        | DownloadErrorKind::Network
        | DownloadErrorKind::RangeUnsupported
        | DownloadErrorKind::AllSourcesFailed => UpdateErrorKind::Network,
    };
    let mut message = format!("{}（{url}）", error.message);
    if !error.suppressed.is_empty() {
        message.push_str(&format!("；各源：{}", error.suppressed.join("；")));
    }
    UpdateError::new(kind, message)
}

/// 所有候选是不是都回「这份文件不存在」。
///
/// 引擎把逐源失败压成一条 [`DownloadErrorKind::AllSourcesFailed`]，原始的 404/410 分类只剩在
/// 失败摘要文本里（`download::http::classify_status` 的文案：`HTTP 404`）。不还原这一层，
/// 「发布清单里挂了个没上传的产物」会被报成「网络失败 + 可重试」，重试多少次都一样。
fn all_candidates_missing(error: &DownloadError) -> bool {
    !error.suppressed.is_empty()
        && error
            .suppressed
            .iter()
            .all(|line| line.contains("HTTP 404") || line.contains("HTTP 410"))
}
