//! 更新源抽象：清单从哪来、包体从哪来，全部收在一个 trait 后面。
//!
//! 这一层刻意不写死任何 URL、任何厂商名：core 只认 [`UpdateSource`] 三个方法，接哪家由上层
//! 注入。真实源已定（D50）：[`super::GitHubReleaseSource`]，它在 `github.rs`，网络与镜像策略
//! 全交给 `crate::download`。
//!
//! 内置的另一个实现是 [`DirSource`]（本地目录 / `file://`），用途是测试、内网手工分发，
//! 以及给远端实现当对拍基准。
//!
//! # 本地源的目录约定
//!
//! ```text
//! <root>/stable.json     渠道清单（文件名 = 渠道名 + .json）
//! <root>/stable.json.sig 清单签名信封（M190；缺了它清单一律拒装，见 `signature`）
//! <root>/beta.json
//! <root>/<产物文件>       清单里 url 写相对路径时，相对 root
//! ```
//!
//! 清单里的 `url` 支持两种写法：`file://` 绝对地址，或相对 `<root>` 的路径。
//! 出现 `http://` / `https://` 一律 [`UpdateErrorKind::UnsupportedSource`]——那是别的
//! 实现的活，本地源不该假装能取远端字节。

use std::fs;
use std::path::{Path, PathBuf};

use super::channel::Channel;
use super::error::{UpdateError, UpdateErrorKind};
use super::manifest::{Artifact, ReleaseManifest};

/// 源交出来的「清单两份原文」：清单本身 + 配套签名信封（M190）。
///
/// 刻意是**原文**而不是解析后的结构：签名只能覆盖字节，先解析再验签就等于给「解析器认了、
/// 签名没认」的东西开了门。判据顺序因此固定为「先验签、再解析」，两者都在
/// [`verified_manifest`] 里，源实现自己不做任何验签。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestDocuments {
    /// 清单原文（`manifest.json` / `<channel>.json` 的字节，按 UTF-8 解释）。
    pub manifest: String,
    /// 签名信封原文（`<清单名>.sig`）；`None` = 源里没有这份文件（会被判 [`UpdateErrorKind::SignatureMissing`]）。
    pub signatures: Option<String>,
}

/// 更新源：清单与包体的来源。
///
/// 实现必须是 `Send + Sync`（下载与替换可能在不同线程上跑），且**不持有运行时**：
/// 同步签名是为了让 CLI 与 GUI 都能直接调用，异步实现自己在方法里桥接。
pub trait UpdateSource: Send + Sync {
    /// 源标识（日志/事件里的 `source` 字段，如 `file:D:\dist`；健康度也按它归并）。
    fn source_id(&self) -> &str;

    /// 取某渠道清单的两份原文；`Ok(None)` = 该渠道还没发布过（不是错误）。
    ///
    /// # Errors
    ///
    /// 读不到文件、响应为空、网络失败都返回 [`UpdateError`]。**这里不验签、不解析**
    /// （见 [`ManifestDocuments`]）——管线的下一站是 [`verified_manifest`]。
    fn fetch_manifest_documents(
        &self,
        channel: Channel,
    ) -> Result<Option<ManifestDocuments>, UpdateError>;

    /// 把产物取到 `dest`，返回写入的字节数。
    ///
    /// 实现只管「把字节放到 dest」；长度与摘要校验由 [`crate::update::verify`] 统一做，
    /// 别在实现里各写一套。
    ///
    /// # Errors
    ///
    /// 源里没有这个文件、读失败、写 `dest` 失败都返回 [`UpdateError`]。
    fn fetch_artifact(&self, artifact: &Artifact, dest: &Path) -> Result<u64, UpdateError>;
}

/// 取清单 → 验签 → 解析 → 渠道校验（**唯一**入口，GUI 与 CLI 共用）。
///
/// 它是个**自由函数**而不是 trait 的默认方法：trait 方法可被任何实现方覆写，那样「信任根的收口点」
/// 就只剩纪律（M190 r1 复审 P2-2）。删掉那条默认实现之后，调用方手里只有源交出来的**原文**
/// （[`ManifestDocuments`]），想绕过验签就必须自己重写一遍解析 + 渠道判据 —— 不再是「一行覆写」。
///
/// # Errors
///
/// 源里没有该渠道清单 ⇒ `Ok(None)`；签名/解析/渠道任一不过 ⇒ [`UpdateError`]。
/// 签名相关的分类见 [`UpdateErrorKind::is_signature`]，它们**不可重试**。
pub fn verified_manifest<S: UpdateSource + ?Sized>(
    source: &S,
    channel: Channel,
) -> Result<Option<ReleaseManifest>, UpdateError> {
    let Some(documents) = source.fetch_manifest_documents(channel)? else {
        return Ok(None);
    };
    // ① 先验签：拿到的是不是「我们发的那份字节」，在这里一次说清。
    let source_label = source.source_id().to_string();
    super::signature::verify_manifest(
        documents.manifest.as_bytes(),
        documents.signatures.as_deref(),
    )
    .map_err(|mut error| {
        error.message = format!("{}（来源 {source_label}）", error.message);
        error
    })?;
    // ② 再解析：到了这一步，字节已经是发布侧签过的那份，解析失败只可能是签名者也写错了。
    let manifest = ReleaseManifest::from_json(&documents.manifest).map_err(|mut error| {
        error.message = format!("{}（来源 {source_label}）", error.message);
        error
    })?;
    // ③ 渠道：清单自报的渠道与请求的对不上是源的问题，不能悄悄当成「没有更新」。
    if manifest.channel != channel {
        return Err(UpdateError::new(
            UpdateErrorKind::ChannelMismatch,
            format!(
                "请求的是 {channel} 渠道，清单（源 {}）自报 {}",
                source.source_id(),
                manifest.channel
            ),
        ));
    }
    Ok(Some(manifest))
}

/// `Box<dyn UpdateSource>` 也是 `UpdateSource`。
///
/// 为什么需要：壳层要**运行期**才决定用哪个源（生产走 GitHub、测试/内网走本地目录），而
/// [`super::UpdateChecker`] 与 [`super::verify::stage`] 都是按 trait 泛型化的。没有这条转发，
/// 调用方只能给每种源写一份检查/下载流程——那正是 mission 禁止的「两套逻辑」。
impl<T: UpdateSource + ?Sized> UpdateSource for Box<T> {
    fn source_id(&self) -> &str {
        (**self).source_id()
    }

    fn fetch_manifest_documents(
        &self,
        channel: Channel,
    ) -> Result<Option<ManifestDocuments>, UpdateError> {
        (**self).fetch_manifest_documents(channel)
    }

    fn fetch_artifact(&self, artifact: &Artifact, dest: &Path) -> Result<u64, UpdateError> {
        (**self).fetch_artifact(artifact, dest)
    }
}

/// 本地目录源（也接受 `file://` 根地址）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirSource {
    root: PathBuf,
    id: String,
}

impl DirSource {
    /// 用本地目录构造。
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let id = format!("file:{}", root.display());
        Self { root, id }
    }

    /// 用 `file://` 地址构造（`file:///D:/dist`、`file:///srv/ull`）。
    ///
    /// # Errors
    ///
    /// 不是 `file://` 开头的地址返回 [`UpdateErrorKind::UnsupportedSource`]。
    pub fn from_url(url: &str) -> Result<Self, UpdateError> {
        let rest = url
            .strip_prefix("file://")
            .ok_or_else(|| unsupported_source(url))?;
        Ok(Self::new(file_url_to_path(rest)))
    }

    /// 根目录。
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 某渠道的清单文件路径。
    #[must_use]
    pub fn manifest_path(&self, channel: Channel) -> PathBuf {
        self.root.join(format!("{}.json", channel.as_str()))
    }

    /// 某渠道清单的签名文件路径（`stable.json` → `stable.json.sig`）。
    #[must_use]
    pub fn signature_path(&self, channel: Channel) -> PathBuf {
        self.root.join(format!("{}.json.sig", channel.as_str()))
    }

    /// 把清单里的 `url` 解析成源侧本地路径。
    fn resolve(&self, url: &str) -> Result<PathBuf, UpdateError> {
        if let Some(rest) = url.strip_prefix("file://") {
            return Ok(file_url_to_path(rest));
        }
        if url.contains("://") {
            return Err(unsupported_source(url));
        }
        Ok(self.root.join(url))
    }
}

fn unsupported_source(url: &str) -> UpdateError {
    let hint = if url.starts_with("http://") || url.starts_with("https://") {
        "本地目录源不取远端字节，请换用实现远端源的 UpdateSource"
    } else {
        "只支持本地相对路径与 file:// 地址"
    };
    UpdateError::new(
        UpdateErrorKind::UnsupportedSource,
        format!("更新源不支持地址 `{url}`：{hint}"),
    )
}

fn file_url_to_path(rest: &str) -> PathBuf {
    if cfg!(windows) {
        // file:///D:/dist → 去掉盘符前的斜杠；Windows 上没有「以 / 开头的当前盘路径」这种写法。
        PathBuf::from(rest.trim_start_matches('/'))
    } else {
        PathBuf::from(rest)
    }
}

impl UpdateSource for DirSource {
    fn source_id(&self) -> &str {
        &self.id
    }

    fn fetch_manifest_documents(
        &self,
        channel: Channel,
    ) -> Result<Option<ManifestDocuments>, UpdateError> {
        let path = self.manifest_path(channel);
        let manifest = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(UpdateError::io_at(&path, &source)),
        };
        // 签名文件缺失**不是**「没有清单」：清单在、签名不在 ⇒ 上层判 SignatureMissing 并拒装。
        let signature_path = self.signature_path(channel);
        let signatures = match fs::read_to_string(&signature_path) {
            Ok(text) => Some(text),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => None,
            Err(source) => return Err(UpdateError::io_at(&signature_path, &source)),
        };
        Ok(Some(ManifestDocuments {
            manifest,
            signatures,
        }))
    }

    fn fetch_artifact(&self, artifact: &Artifact, dest: &Path) -> Result<u64, UpdateError> {
        let from = self.resolve(&artifact.url)?;
        if let Some(parent) = dest.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|error| UpdateError::io_at(parent, &error))?;
            }
        }
        match fs::copy(&from, dest) {
            Ok(bytes) => Ok(bytes),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Err(UpdateError::new(
                UpdateErrorKind::SourceMissing,
                format!("源里没有产物 `{}`", artifact.url),
            )
            .with_path(&from)),
            Err(source) => Err(UpdateError::io_at(&from, &source)),
        }
    }
}
