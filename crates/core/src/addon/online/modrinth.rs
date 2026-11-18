//! Modrinth v2 的四个端点（**只有一个站点，所以不做「源分派」那一层**）。
//!
//! | 方法 | 端点 | 拿什么 |
//! |------|------|--------|
//! | [`Modrinth::search`] | `GET /v2/search` | 一页命中项（分页游标在 [`SearchQuery`] 里） |
//! | [`Modrinth::project`] | `GET /v2/project/{id or slug}` | 项目详情（含长文与链接） |
//! | [`Modrinth::versions`] | `GET /v2/project/{id}/version` | 版本列表（可按版本/加载器过滤） |
//! | [`Modrinth::version_by_hash`] | `GET /v2/version_file/{hash}` | 某个文件的所属版本（**补全路径用的那条**） |
//!
//! # 端点拼装与解析都分离出来
//!
//! `*_url` 是纯函数（单测直接比字符串），`search/project/...` 只做「取文本 → 解析」。
//! 于是录制 JSON 的离线夹具不需要任何 HTTP 桩就能覆盖全部分支。
//!
//! # 为什么按摘要查是 GET
//!
//! Modrinth 的批量接口是 `POST /v2/version_files`（请求体是一个 JSON 对象）。引擎的
//! `DownloadRequest` 没有请求体字段，为它单开一条直发 `client()` 的路径就得自己写退避与限流
//! （presenter 里为 CF 的 header 已经付过这笔账）。补全路径一次只看一两个摘要，逐条 GET 更划算。
//!
//! # 百分号编码
//!
//! 只为 query 串写一个最小实现（`A-Za-z0-9-._~` 不编码），**不引第三方 crate**：
//! `facets` 与 `game_versions` 都是 JSON 文本，方括号引号逗号全要编码，否则站点会当成
//! 空过滤条件静默忽略——那会让「筛选条说 Fabric、结果里有 Forge」这种谎话成立。

use super::model::{
    parse_project, parse_search, parse_version, parse_versions, ProjectDetail, SearchPage,
    SearchQuery, Version, VersionQuery,
};
use super::transport::{OnlineError, Transport};

/// Modrinth API 的根（匿名可用，无需 key）。
pub const MODRINTH_API: &str = "https://api.modrinth.com/v2";

/// 按哪个摘要查文件。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HashAlgorithm {
    /// SHA-1（mrpack 的 `files[].hashes.sha1`）
    #[default]
    Sha1,
    /// SHA-512（mrpack 的必填摘要）
    Sha512,
}

impl HashAlgorithm {
    /// 站点 `algorithm` 参数值。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sha1 => "sha1",
            Self::Sha512 => "sha512",
        }
    }

    /// 十六进制长度（格式自检用；长度不对就别发请求）。
    #[must_use]
    pub const fn hex_len(self) -> usize {
        match self {
            Self::Sha1 => 40,
            Self::Sha512 => 128,
        }
    }

    /// 摘要是否**形状**合法（小写归一 + 十六进制 + 长度）。
    #[must_use]
    pub fn accepts(self, hex: &str) -> bool {
        hex.len() == self.hex_len() && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
    }
}

/// Modrinth 客户端（只持有 [`Transport`]，不持有客户端/连接池）。
#[derive(Debug, Clone, Copy)]
pub struct Modrinth<'a, T: Transport + ?Sized> {
    transport: &'a T,
}

impl<'a, T: Transport + ?Sized> Modrinth<'a, T> {
    /// 绑定传输层。
    #[must_use]
    pub const fn new(transport: &'a T) -> Self {
        Self { transport }
    }

    /// 检索的 URL（纯函数，单测直接钉）。
    #[must_use]
    pub fn search_url(&self, query: &SearchQuery) -> String {
        let query = query.normalized();
        let mut url = format!(
            "{MODRINTH_API}/search?limit={}&offset={}&index={}",
            query.limit,
            query.offset,
            query.index.as_str()
        );
        let facets = query.facets();
        if facets != "[]" {
            url.push_str(&format!("&facets={}", encode(&facets)));
        }
        if !query.text.is_empty() {
            url.push_str(&format!("&query={}", encode(&query.text)));
        }
        url
    }

    /// 项目详情的 URL。
    #[must_use]
    pub fn project_url(&self, id_or_slug: &str) -> String {
        format!("{MODRINTH_API}/project/{}", encode(id_or_slug))
    }

    /// 版本列表的 URL。
    #[must_use]
    pub fn versions_url(&self, query: &VersionQuery) -> String {
        let mut url = format!("{MODRINTH_API}/project/{}/version", encode(&query.project));
        let mut first = true;
        if let Some(version) = query
            .game_version
            .as_deref()
            .filter(|item| !item.is_empty())
        {
            url.push_str(&format!(
                "{}game_versions={}",
                separator(&mut first),
                encode(&format!("[\"{version}\"]"))
            ));
        }
        if let Some(loader) = query.loader.as_deref().filter(|item| !item.is_empty()) {
            url.push_str(&format!(
                "{}loaders={}",
                separator(&mut first),
                encode(&format!("[\"{loader}\"]"))
            ));
        }
        url
    }

    /// 按文件摘要查所属版本的 URL。
    #[must_use]
    pub fn version_file_url(&self, algorithm: HashAlgorithm, hex: &str) -> String {
        format!(
            "{MODRINTH_API}/version_file/{}?algorithm={}",
            encode(&hex.to_ascii_lowercase()),
            algorithm.as_str()
        )
    }

    /// 检索一页。
    ///
    /// # Errors
    /// 传输层失败（见 [`OnlineErrorKind`]）或响应形状不符。
    pub async fn search(&self, query: &SearchQuery) -> Result<SearchPage, OnlineError> {
        let normalized = query.normalized();
        let url = self.search_url(&normalized);
        let body = self.transport.get_text(&url).await?;
        let mut page = parse_search(&body, normalized.offset, normalized.limit)?;
        // 站点没回显时用请求值补齐，分页游标才不会因为缺字段而卡死。
        if page.limit == 0 {
            page.limit = normalized.limit;
        }
        Ok(page)
    }

    /// 项目详情。
    ///
    /// # Errors
    /// 传输层失败或响应形状不符。
    pub async fn project(&self, id_or_slug: &str) -> Result<ProjectDetail, OnlineError> {
        let url = self.project_url(id_or_slug);
        let body = self.transport.get_text(&url).await?;
        parse_project(&body)
    }

    /// 版本列表（站点的顺序 = 最新在前）。
    ///
    /// # Errors
    /// 传输层失败或响应形状不符。
    pub async fn versions(&self, query: &VersionQuery) -> Result<Vec<Version>, OnlineError> {
        let url = self.versions_url(query);
        let body = self.transport.get_text(&url).await?;
        parse_versions(&body)
    }

    /// 按文件摘要查所属版本。
    ///
    /// # Errors
    /// 摘要形状不对时**不发请求**（[`OnlineErrorKind::Unsupported`]）；站点 404 是
    /// [`OnlineErrorKind::NotFound`]（= 这个摘要不在 Modrinth 上，补全要如实记「补不出」）。
    pub async fn version_by_hash(
        &self,
        algorithm: HashAlgorithm,
        hex: &str,
    ) -> Result<Version, OnlineError> {
        let hex = hex.trim();
        if !algorithm.accepts(hex) {
            return Err(OnlineError::unsupported(format!(
                "摘要不是合法的 {}：长度 {}，要求 {}",
                algorithm.as_str(),
                hex.len(),
                algorithm.hex_len()
            )));
        }
        let url = self.version_file_url(algorithm, hex);
        let body = self.transport.get_text(&url).await?;
        parse_version(&body)
    }
}

/// 第一个参数用 `?`，后续用 `&`。
fn separator(first: &mut bool) -> &'static str {
    if *first {
        *first = false;
        "?"
    } else {
        "&"
    }
}

/// 最小百分号编码（query 串用；`A-Za-z0-9-._~` 原样，其余按字节编码）。
#[must_use]
pub fn encode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(char::from(byte));
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
