//! 远端补全与下载的**注入点**：导入管线自己不碰网络，只认这个 trait。
//!
//! # 为什么是 trait 而不是直接调 `DownloadEngine`
//!
//! 三条理由，每条都对应一个真实麻烦：
//!
//! 1. **测试不能联网**：三种格式的端到端用例必须离线跑（CI 没有网，也不该有）；
//! 2. **CurseForge 的 `files[]` 补全走用户自带的 Key**（M317）：CF 官方清单只有 `projectID/fileID`，
//!    要 `x-api-key` 的 `api.curseforge.com/v1/mods/<id>/files/<fid>` 才能换出文件名与地址。
//!    生产实现 [`DownloadEngineFetcher`] 配了 Key 就调 [`crate::addon::curseforge`]（见
//!    `curse` 子模块）；**没配 Key 时的结果是 [`FetchErrorKind::NotConfigured`]** —— 具名的
//!    「去填 Key 就能干」，不是 [`FetchErrorKind::Unsupported`] 的「这条路走不通」。两种都让
//!    **那一条**记成「缺文件 + 原因」，其余条目照装（整包 `Partial`，**不是** `Failed`）。
//!    无论哪种都**绝不猜下载 URL**：猜出来的地址一旦 404，用户看到的是"导入失败"而不是
//!    "这条暂时补不了"；
//! 3. **依赖方向**：`addon` 属于更高层的领域逻辑，让它直接持有一个 `DownloadEngine` 会把
//!    镜像/健康度/地区档案这些下载侧策略涌进导入链路。
//!
//! # 生产实现只走 `core::download`
//!
//! [`DownloadEngineFetcher`] 是本 crate 里**唯一**的生产实现，做两件事：
//!
//! 1. **[`PackFetcher::fetch_file`]** —— 交给 [`crate::addon::online::download_verified`]：
//!    sha1 由引擎流式校验、sha512 由 addon 侧落盘后复核（引擎的 `Integrity` 只认 Sha1/Sha256），
//!    失败按 [`crate::addon::online::RETRY_ATTEMPTS`] 重试且不留半截文件；
//! 2. **[`PackFetcher::modrinth_file_by_sha512`]** —— M162 的补全路径：按 sha512 反查
//!    Modrinth 拿同一个文件的官方 sha1（站点不认得就如实报 `NotFound`，由调用方计数）；
//! 3. **[`PackFetcher::curse_file`]** —— M317 的补全路径：CF 的 `projectID/fileID` 经用户自带
//!    的 Key 换出文件名 / 地址 / 摘要 / 类别（实现在 `fetch/curse.rs`）。
//!
//! 三条都经同一个引擎（`EngineTransport` / `DownloadRequest`），导入链路里**没有**第二个
//! `reqwest::Client`，也没有第二个重试循环。

mod curse;

pub(crate) use curse::curse_meta;
use curse::key_missing;

use std::future::Future;
use std::path::Path;
use std::pin::Pin;

use crate::addon::curseforge::{ApiKey, RetryPolicy};
use crate::addon::online::complete::identity_of;
use crate::addon::online::download::download_verified;
use crate::addon::online::modrinth::HashAlgorithm;
use crate::addon::online::{
    EngineArtifactFetcher, EngineTransport, Modrinth, OnlineError, OnlineErrorKind,
};
use crate::download::DownloadEngine;

/// "搞不到"的成因（**分级判据只在这里**）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchErrorKind {
    /// 源上确实没有（404 / 文件被删）。
    NotFound,
    /// 网络侧失败（超时、连不上、站点 5xx 退避后仍失败）。
    Network,
    /// **没联网**：连不上任何候选源（与 `Network` 分开，是因为「补全」见到它要立刻停，
    /// 而见到 404 / 5xx 不该停 —— 下一条可能补得出来）。
    Offline,
    /// 本地 IO（写盘失败、磁盘满）。
    Io,
    /// 内容与声明的摘要不符（字节被换过 / 下坏了）。
    Integrity,
    /// 站点给的**响应本身**读不懂：JSON 坏、不是 UTF-8、服务器忽略 Range。
    ///
    /// 与 [`Self::Integrity`] 是两件事（M323 之前并在一档，于是引擎那句「服务器不支持分段请求」
    /// 成了 `import_integrity` + 「摘要与清单声明不符」）。它的真含义是「站点给的东西这次用不上」，
    /// 所以**可容忍**——与同一个事实在 CurseForge 那条路上的去处一致（见 `fetch/curse.rs`）。
    Malformed,
    /// **有 Key 就能干**：CurseForge 要用户自带 API Key，而这条导入没配（M317）。
    ///
    /// 与 [`Self::Unsupported`] 是两种用户动作：这一档去设置里填 Key 就能补上，后者填了也没用。
    /// 两档都**可容忍**（那一条记缺、整包照装），但上层靠这个区别决定提示哪一句。
    NotConfigured,
    /// 这条路走不通（没有可用下载地址 / 站点没给文件名）。
    Unsupported,
}

impl FetchErrorKind {
    /// 稳定字符串 id。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "fetch_not_found",
            Self::Network => "fetch_network",
            Self::Offline => "fetch_offline",
            Self::Io => "fetch_io",
            Self::Integrity => "fetch_integrity",
            Self::Malformed => "fetch_malformed",
            Self::NotConfigured => "fetch_not_configured",
            Self::Unsupported => "fetch_unsupported",
        }
    }

    /// 是否属于"某个文件搞不到"（可容忍，实例照装）。[`Self::Malformed`] 在这里：响应读不懂
    /// ⇒ 这个文件这次拿不到（逐条记缺 + 原话），不是 [`Self::Integrity`] 那种"字节不对 ⇒ 回滚"。
    #[must_use]
    pub const fn is_tolerable(self) -> bool {
        matches!(
            self,
            Self::NotFound
                | Self::Network
                | Self::Offline
                | Self::NotConfigured
                | Self::Malformed
                | Self::Unsupported
        )
    }

    /// 是不是「先配 Key 再谈」的具名状态（缺 Key）。
    #[must_use]
    pub const fn is_not_configured(self) -> bool {
        matches!(self, Self::NotConfigured)
    }
}

/// 一次取数失败。
#[derive(Debug, Clone)]
pub struct FetchError {
    kind: FetchErrorKind,
    message: String,
}

impl FetchError {
    /// 构造。
    #[must_use]
    pub fn new(kind: FetchErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// 这条路走不通。
    #[must_use]
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(FetchErrorKind::Unsupported, message)
    }

    /// **有 Key 就能干**的具名状态（不是「这条路走不通」）。
    #[must_use]
    pub fn not_configured(message: impl Into<String>) -> Self {
        Self::new(FetchErrorKind::NotConfigured, message)
    }

    /// 内容与声明不符。
    #[must_use]
    pub fn integrity(message: impl Into<String>) -> Self {
        Self::new(FetchErrorKind::Integrity, message)
    }

    /// 响应读不懂（**不是**摘要不符）。
    #[must_use]
    pub fn malformed(message: impl Into<String>) -> Self {
        Self::new(FetchErrorKind::Malformed, message)
    }

    /// 大类。
    #[must_use]
    pub const fn kind(&self) -> FetchErrorKind {
        self.kind
    }

    /// 人读说明。
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for FetchError {}

// 这里**没有** `From<DownloadError> for FetchError`：生产链路上 `DownloadError` 一律先经
// `online::OnlineError::from`（`addon/online/transport.rs`，叶子成因与逐源摘要都在那里接线），
// 再由下面的 `From<OnlineError>` 落到本类型。曾经在这里另写一份 `DownloadError → FetchError`，
// 结果修在了没人跑的那份上（M319 复审 P2-2 / T35）：本类型的错误分类必须只走一条路。

impl From<OnlineError> for FetchError {
    fn from(error: OnlineError) -> Self {
        let kind = match error.kind() {
            OnlineErrorKind::NotFound => FetchErrorKind::NotFound,
            OnlineErrorKind::Integrity => FetchErrorKind::Integrity,
            // 拆开（M323）：`Integrity` 是「字节与声明不符 ⇒ 整包回滚、用户重下」，`Malformed` 是
            // 「响应读不懂 ⇒ 这一条补不出来」。并档时引擎那句「服务器不支持分段请求」
            // （`RangeUnsupported` → `OnlineErrorKind::Malformed`）会被报成 `import_integrity`。
            OnlineErrorKind::Malformed => FetchErrorKind::Malformed,
            OnlineErrorKind::Io => FetchErrorKind::Io,
            OnlineErrorKind::Unsupported => FetchErrorKind::Unsupported,
            // 「没联网」要能被「补全立刻停」认出来；站点 5xx 只是这一条补不出。
            OnlineErrorKind::NotConnected => FetchErrorKind::Offline,
            OnlineErrorKind::Server => FetchErrorKind::Network,
        };
        Self::new(kind, error.message().to_string())
    }
}

/// trait 方法返回的装箱 future（core 不引 `async-trait`，手写 `Pin<Box<dyn Future>>` 即可）。
pub type FetchFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// CurseForge 一个文件的元数据（`files[]` 补全的产物）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurseFileMeta {
    /// 文件名（`jei-1.20.1-15.2.0.27.jar`）。
    pub file_name: String,
    /// 下载地址；`None` = 作者关了 API 分发（CF 官方允许），需要用户手动提供。
    pub url: Option<String>,
    /// 官方给的 SHA-1。
    pub sha1: Option<String>,
    /// 字节数。
    pub size: Option<u64>,
    /// 项目类别（CF `classId`：6 = mods、12 = resource packs、6945 = shaders …）。
    pub class_id: Option<i64>,
}

/// Modrinth 一个文件的元数据（`files[]` 补全的产物，按 sha512 反查得到）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModrinthFileMeta {
    /// 站点给的 SHA-1（**同一个文件**的官方摘要，不是本地推算）
    pub sha1: Option<String>,
    /// 站点给的字节数
    pub size: Option<u64>,
    /// 站点给的直链（清单里没有 downloads 时才用得上）
    pub url: Option<String>,
    /// 站点给的文件名
    pub file_name: Option<String>,
}

/// 一次文件下载请求。
#[derive(Debug, Clone, Copy)]
pub struct FetchRequest<'a> {
    /// 逻辑下载地址（官方源）。
    pub url: &'a str,
    /// 落盘路径（导入器给的**暂存**路径，不是最终实例路径）。
    pub dest: &'a Path,
    /// 期望 SHA-1（清单声明的）。
    pub sha1: Option<&'a str>,
    /// 期望 SHA-512（mrpack 的必填摘要；引擎不认这一档，由 `online::download` 落盘后复核）。
    pub sha512: Option<&'a str>,
    /// 期望字节数。
    pub size: Option<u64>,
}

/// 一次下载的产物。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FetchedFile {
    /// 落盘字节数。
    pub bytes: u64,
}

/// 远端补全 + 下载（测试注入假件，生产走 [`DownloadEngineFetcher`]）。
pub trait PackFetcher: Send + Sync {
    /// CurseForge `projectID/fileID` → 文件名 / 下载地址 / 摘要 / 类别。
    ///
    /// 默认实现回 [`FetchErrorKind::NotConfigured`]（**具名的「缺 Key」**，可容忍）：
    /// 它手上既没有传输层也没有用户自带的 Key，所以它**只能是**「没配 Key 的 fetcher」。
    /// 配了 Key 的那条真路在 [`DownloadEngineFetcher`]（见 `fetch/curse.rs`）。
    ///
    /// 为什么不是 [`FetchErrorKind::Unsupported`]：那会让上层把「去设置里填 Key」说成「这条
    /// 真装不了」，用户就再也不会去填了。
    fn curse_file<'a>(
        &'a self,
        project_id: i64,
        file_id: i64,
    ) -> FetchFuture<'a, Result<CurseFileMeta, FetchError>> {
        Box::pin(async move { Err(key_missing(project_id, file_id)) })
    }

    /// 按 SHA-512 反查 Modrinth 上这个文件的元数据（**sha1 补全用**）。
    ///
    /// 默认实现返回 [`FetchErrorKind::Unsupported`]：没有远程源（离线导入、测试的 `StubFetcher`）
    /// 就补不出——这与 M160 的 `unknown_remote` 口径一致：补不出仍要如实计数，**不许猜**。
    fn modrinth_file_by_sha512<'a>(
        &'a self,
        sha512: &'a str,
    ) -> FetchFuture<'a, Result<ModrinthFileMeta, FetchError>> {
        Box::pin(async move {
            Err(FetchError::unsupported(format!(
                "没有接 Modrinth 源，无法按 sha512 补全这一条（{sha512}）"
            )))
        })
    }

    /// 把 `request.url` 下到 `request.dest`。
    fn fetch_file<'a>(
        &'a self,
        request: FetchRequest<'a>,
    ) -> FetchFuture<'a, Result<FetchedFile, FetchError>>;
}

/// 生产实现：把请求交给 `core::download` 的引擎。
///
/// `curse_key` 是用户自带的 CurseForge Key（M317）：**没有它不是一个错误**，而是「CF 条目补不出
/// 来」的具名状态（[`FetchErrorKind::NotConfigured`]）。它只在拼 CF 请求头那一步取值，见
/// [`crate::addon::curseforge::ApiKey`]（没有公开取值方法、`Debug` 恒打 `<redacted>`），
/// 所以「顺手把 Key 打进日志 / 错误串」在本类型上不成立。
#[derive(Debug, Clone)]
pub struct DownloadEngineFetcher<'a> {
    engine: &'a DownloadEngine,
    curse_key: Option<ApiKey>,
}

impl<'a> DownloadEngineFetcher<'a> {
    /// 绑定引擎（没配 CF Key：CF 条目会得到具名的「缺 Key」状态）。
    #[must_use]
    pub const fn new(engine: &'a DownloadEngine) -> Self {
        Self {
            engine,
            curse_key: None,
        }
    }

    /// 配上用户自带的 CurseForge Key（宿主从设置 `online.curseforge.api_key` 读出后递进来）。
    #[must_use]
    pub fn with_curse_key(mut self, key: ApiKey) -> Self {
        self.curse_key = Some(key);
        self
    }
}

impl PackFetcher for DownloadEngineFetcher<'_> {
    fn curse_file<'a>(
        &'a self,
        project_id: i64,
        file_id: i64,
    ) -> FetchFuture<'a, Result<CurseFileMeta, FetchError>> {
        Box::pin(async move {
            // 传输层复用引擎的 `reqwest::Client`（同一条连接池 / 代理 / UA），不新建 client。
            let transport = crate::addon::curseforge::EngineTransport::new(self.engine);
            curse_meta(
                &transport,
                self.curse_key.as_ref(),
                RetryPolicy::standard(),
                project_id,
                file_id,
            )
            .await
        })
    }

    fn modrinth_file_by_sha512<'a>(
        &'a self,
        sha512: &'a str,
    ) -> FetchFuture<'a, Result<ModrinthFileMeta, FetchError>> {
        Box::pin(async move {
            let transport = EngineTransport::new(self.engine);
            let client = Modrinth::new(&transport);
            let version = client
                .version_by_hash(HashAlgorithm::Sha512, sha512)
                .await
                .map_err(FetchError::from)?;
            let identity = identity_of(&version, sha512).ok_or_else(|| {
                FetchError::new(
                    FetchErrorKind::NotFound,
                    format!("站点返回的版本里没有 sha512 与 {sha512} 相同的文件"),
                )
            })?;
            Ok(ModrinthFileMeta {
                sha1: Some(identity.sha1),
                size: identity.size,
                url: identity.url,
                file_name: version
                    .primary_file()
                    .map(|file| file.filename.clone())
                    .filter(|name| !name.trim().is_empty()),
            })
        })
    }

    fn fetch_file<'a>(
        &'a self,
        request: FetchRequest<'a>,
    ) -> FetchFuture<'a, Result<FetchedFile, FetchError>> {
        Box::pin(async move {
            // sha1 交给引擎流式校验、sha512 由 `download_verified` 落盘后复核；
            // 失败重试与「不留半截文件」也在那一层（见 `online::download` 的模块文档）。
            let fetcher = EngineArtifactFetcher::new(self.engine);
            let done = download_verified(
                &fetcher,
                request.url,
                request.sha1,
                request.sha512,
                request.size,
                request.dest,
            )
            .await
            .map_err(FetchError::from)?;
            Ok(FetchedFile { bytes: done.bytes })
        })
    }
}
