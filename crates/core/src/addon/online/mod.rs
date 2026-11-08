//! 在线内容源（M162）：**只做匿名可用的那半**——Modrinth v2。
//!
//! # 为什么形状是 Modrinth 的，而不是「内容源通用抽象」
//!
//! CurseForge 的检索/文件端点要 `x-api-key`（D10 §5 的客户端还没写），而它的 JSON 方言与
//! Modrinth 几乎没有一处同名（`project_id` vs `id`、`description` vs `summary`、
//! `downloads` vs `downloadCount`、版本列表还要再套一层 `latestFilesIndexes`）。为了让两者
//! 共用而发明一套谁都不像的抽象，代价是两边都得翻译两次、错一处两边都错。
//!
//! 所以本模块**按 Modrinth 的真实形状建模**（`/v2/search` 的 facets、`/v2/project/{id}`、
//! `/v2/project/{id}/version`、`/v2/version_file/{hash}`），把「平台差异」留到将来真要加第二个
//! 源时再抽。留的缝只有两处：
//!
//! | 缝 | 在哪 | 换源时要动什么 |
//! |----|------|----------------|
//! | [`Transport`] | 唯一的 HTTP 出口（trait） | 换/加一个实现；测试注入录制 JSON |
//! | [`Modrinth::new`] | `Modrinth<'a, T: Transport + ?Sized>` | 传 `&dyn Transport` 就能换站点客户端，一行业务代码不改 |
//!
//! # 网络层（零新依赖）
//!
//! `Cargo.lock` 里已有的 `reqwest` 0.12 + `tokio` + `serde_json` 就是全部。生产实现
//! [`EngineTransport`] 走 `DownloadEngine::download_bytes`，于是**信号量限流、源健康度、
//! 指数退避、错误分类**全由 `core::download` 提供，本模块不写第二个重试循环，也不持有第二个
//! `reqwest::Client`（与 `addon::import::fetch` 同一条纪律）。
//!
//! 没有 POST：Modrinth 的批量接口（`POST /v2/version_files`）需要 POST 请求体，而引擎的
//! `DownloadRequest` 没有请求体字段。补全路径因此按**单摘要 GET** 逐条查（循环在
//! `crate::addon::import::complete`，本模块只用 [`identity_of`] 认文件），
//! mrpack 里缺 `sha1` 的条目本来就是少数，不值得为它引入第二条 HTTP 路径。
//!
//! # 离线不是「空结果」
//!
//! 无网时上层必须能说清「未联网」而不是「没有匹配的内容」，所以错误是枚举不是字符串：
//! 引擎把叶子成因（404 / 5xx / 网络）放在 `DownloadError::leaf` 上，[`OnlineError`] 按它映射
//! 成 `NotFound` / `Server` / `NotConnected`（见 `transport::From<DownloadError>` 的映射表），
//! presenter 再翻成 DTO 的 `ContentFailureKind::{Offline,NotFound,Server}`。
//! 没有叶子时保守归到 `NotConnected`——宁可说「检查网络」，也不声称「没有匹配的内容」。
//!
//! # 模块
//!
//! | 文件 | 职责 |
//! |------|------|
//! | [`model`] | Modrinth 的响应形状 + 检索请求（facets / 排序 / 分页） |
//! | [`transport`] | HTTP 出口 trait + 生产实现（走下载引擎）+ 错误分类 |
//! | [`modrinth`] | 端点拼装与响应解析（纯逻辑，测试直接喂录制 JSON） |
//! | [`download`] | 取下载地址之后那一步：引擎下载 + **sha1/sha512 复核** + 重试 + 不留半截文件 |
//! | [`complete`] | 按摘要认文件（[`identity_of`]）：站点返回的版本里哪条才是我们要的那个 |

pub mod complete;
pub mod download;
pub mod model;
pub mod modrinth;
pub mod transport;

pub use complete::identity_of;
pub use download::{
    digest_file, download_verified, ArtifactFetcher, Digest, Downloaded, EngineArtifactFetcher,
    FetchArgs, RETRY_ATTEMPTS,
};
pub use model::{
    ContentType, FileHashes, ProjectDetail, ProjectHit, SearchPage, SearchQuery, SortIndex,
    Version, VersionFile, VersionQuery, VersionType,
};
pub use modrinth::{HashAlgorithm, Modrinth, MODRINTH_API};
pub use transport::{EngineTransport, OnlineError, OnlineErrorKind, OnlineFuture, Transport};

#[cfg(test)]
mod tests;
