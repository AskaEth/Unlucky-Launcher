//! 「能否被远程替代」的**注入点**：两条查询 + 一个离线默认实现。
//!
//! # 为什么是 trait 而不是直接调 API
//!
//! 与 M111 的 `PackFetcher` 同一条理由：测试不能联网；CF 的查表要 `x-api-key`（D10 §5 的客户端
//! 还没写）；而"证明不了"必须能被如实表达——查表失败 ≠ 文件有问题。所以导出管线只认这个 trait，
//! 生产实现将来接 Modrinth `/v2/version_file/{sha1}` 与 CF `/v1/fingerprints/432`。
//!
//! # 两条查询的语义差别（别合并成一个）
//!
//! | 查询 | 键 | 命中能给出什么 | 谁用得上 |
//! |------|-----|----------------|----------|
//! | [`RemoteCatalog::modrinth_by_sha1`] | 文件 SHA-1 | `downloads[]` + `hashes`（含 `sha512`）+ `fileSize` | mrpack（它要求 sha512） |
//! | [`RemoteCatalog::curseforge_by_fingerprint`] | CF 指纹 | `projectID`/`fileID`（+ 可选直链） | CF 清单（它只认这两个 id） |
//!
//! 只命中一条时，另一格式通常用不上它——这正是 [`crate::addon::export::report::FileReason`]
//! 要区分 `CurseOnlyMatch` / `ModrinthOnlyMatch` 的原因：报告要说清"证明了，但目标格式表达不了"，
//! 而不是笼统地写"找不到"。
//!
//! # 哨兵值
//!
//! CF 指纹 `811513880` 会误命中（HMCL issue #4597），由**调用方**在建表前短路：
//! 见 `curseforge::fingerprint::is_sentinel`。

use std::collections::BTreeMap;

/// 这条命中来自哪个目录。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteSource {
    /// Modrinth（按 SHA-1 查）。
    Modrinth,
    /// CurseForge（按文件指纹查）。
    CurseForge,
}

/// 一次远程命中的结果。
///
/// 字段刻意按"能写进目标清单"组织，而不是按某个 API 的响应形状：
/// mrpack 要 `downloads` + `hashes`，CF 要 `projectID`/`fileID`，两个都给了就是两份清单都能写。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteHit {
    /// 来源目录。
    pub source: RemoteSource,
    /// 直链候选（多源；mrpack 写进 `downloads[]`，CF 清单没有这个字段但也留着做日志）。
    pub downloads: Vec<String>,
    /// 摘要表（键名小写）。Modrinth 侧必须含 `sha1` 与 `sha512`——后者是 mrpack 的硬要求。
    pub hashes: BTreeMap<String, String>,
    /// 服务端记录的文件字节数（与本地实际字节数不一致时以本地为准，见 `pipeline`）。
    pub size: Option<u64>,
    /// CurseForge `projectID`。
    pub project_id: Option<i64>,
    /// CurseForge `fileID`。
    pub file_id: Option<i64>,
}

impl RemoteHit {
    /// 一次 Modrinth 命中：直链 + 摘要表（必须有 `sha512`，否则 mrpack 写不出来）。
    #[must_use]
    pub fn modrinth(
        downloads: Vec<String>,
        hashes: BTreeMap<String, String>,
        size: Option<u64>,
    ) -> Self {
        Self {
            source: RemoteSource::Modrinth,
            downloads,
            hashes,
            size,
            project_id: None,
            file_id: None,
        }
    }

    /// 一次 CurseForge 命中：`projectID`/`fileID` 是 CF 清单的**全部**必需信息。
    #[must_use]
    pub fn curseforge(project_id: i64, file_id: i64) -> Self {
        Self {
            source: RemoteSource::CurseForge,
            downloads: Vec::new(),
            hashes: BTreeMap::new(),
            size: None,
            project_id: Some(project_id),
            file_id: Some(file_id),
        }
    }

    /// 顺带给一条 CF 直链（有就写进 `downloads[]`，没有也不影响 CF 清单）。
    #[must_use]
    pub fn with_download(mut self, url: impl Into<String>) -> Self {
        self.downloads.push(url.into());
        self
    }

    /// `hashes.sha512`。
    #[must_use]
    pub fn sha512(&self) -> Option<&str> {
        self.hashes.get("sha512").map(String::as_str)
    }

    /// 能不能写进 CF 的 `files[]`。
    #[must_use]
    pub fn curse_ids(&self) -> Option<(i64, i64)> {
        match (self.project_id, self.file_id) {
            (Some(project), Some(file)) => Some((project, file)),
            _ => None,
        }
    }

    /// 有没有能写进 mrpack 的 `downloads[]` 的非空直链。
    #[must_use]
    pub fn has_download(&self) -> bool {
        self.downloads.iter().any(|url| !url.trim().is_empty())
    }
}

/// 「这个文件在远端是谁」的查询接口。
///
/// 两个方法都必须**只做本地判据 + 调用方给的缓存**：实现里如果联网，那就是生产的
/// Modrinth/CF 客户端（D10 §5），本模块不关心它怎么拿到答案。
pub trait RemoteCatalog {
    /// 按 SHA-1 查 Modrinth 版本文件；查不到返回 `None`（**不要**用 `None` 表示网络失败以外的
    /// 「本地构造不了查询」——那种情况应当返回 `None` 并让文件内联）。
    fn modrinth_by_sha1(&self, sha1: &str) -> Option<RemoteHit>;

    /// 按 CurseForge 指纹查项目/文件；查不到返回 `None`。
    ///
    /// 调用方已按哨兵值（`811513880`）短路，实现不必重复判断。
    fn curseforge_by_fingerprint(&self, fingerprint: u32) -> Option<RemoteHit>;
}

/// 离线：什么都证明不了 ⇒ 所有文件内联。
///
/// 这是**默认**行为，也是本仓库当前的唯一实现：没有 CF 客户端（要 key）、没有 Modrinth 客户端
/// （D10 §5 规划中），联网查询在测试里也不允许。用它的导出结果是"一个完整、可导入、全部走
/// `overrides/` 的包"——不是残废包。
#[derive(Debug, Clone, Copy, Default)]
pub struct OfflineCatalog;

impl RemoteCatalog for OfflineCatalog {
    fn modrinth_by_sha1(&self, _sha1: &str) -> Option<RemoteHit> {
        None
    }

    fn curseforge_by_fingerprint(&self, _fingerprint: u32) -> Option<RemoteHit> {
        None
    }
}
