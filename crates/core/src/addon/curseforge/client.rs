//! CurseForge v1 的六个端点（**只有一个站点，所以不做「源分派」那一层**）。
//!
//! | 方法 | 端点 | 拿什么 |
//! |------|------|--------|
//! | [`CurseForge::search`] | `GET /mods/search` | 一页命中项（关键词 / 版本 / 加载器 / 排序 / 分页） |
//! | [`CurseForge::project`] | `GET /mods/{id}` | 单个 mod 的详情 |
//! | [`CurseForge::project_class`] | `GET /mods/{id}` | 只要项目类别 `classId`（M317：导入侧的落盘目录） |
//! | [`CurseForge::files`] | `GET /mods/{id}/files` | 文件列表（可按游戏版本与加载器过滤） |
//! | [`CurseForge::file`] | `GET /mods/{id}/files/{fileId}` | 单个文件（含摘要与 `downloadUrl`） |
//! | [`CurseForge::download_url`] | `GET /mods/{id}/files/{fileId}/download-url` | **下载地址解析** |
//!
//! # 形状
//!
//! `*_url` 是纯函数（单测直接比字符串），`search/project/...` 只做「取正文 → 解析」。
//! 于是录制 JSON 的离线夹具不需要任何 HTTP 桩就能覆盖全部分支——本模块的每一条用例都**不联网**。
//!
//! [`CurseForge`] 只持有传输层（[`CfTransport`]）、Key（[`ApiKey`]，可缺）与重试策略
//! （[`RetryPolicy`]）。它**不**持有 `reqwest::Client`：连接池、代理、超时都在
//! `core::download` 的引擎里（[`crate::addon::curseforge::EngineTransport`] 借一个 `&DownloadEngine`）。
//!
//! # Key 缺失：发请求**之前**就返回
//!
//! [`CurseForge::new`] 造出来的是「没配 Key」的客户端，每一个方法第一句就是
//! 「有没有 Key」，没有就返回 [`super::CfErrorKind::NotConfigured`]。于是：
//!
//! - **不是 panic**（缺配置是正常状态，不是程序错误）；
//! - **不是空列表**（空列表会被页面画成「查无此项」，用户会去换关键词，而真正该做的是去填 Key）；
//! - **不是静默 403**（站点会把「没有 Key」与「Key 无效」都答成 401/403，本地若先发一次，
//!   用户就分不清是自己没配还是 Key 坏了）。
//!
//! 三条都不是「风格问题」：每一条都让用户去做一件没用的事。所以这条判定必须**在**网络之前。
//!
//! # 限流重试：有上界、可关、听站点的 `Retry-After`
//!
//! 429 / 5xx / 网络失败按 [`RetryPolicy`] 重试；401 / 403 / 404 **不重试**（重试只会更糟，
//! 还白白吃掉配额）。等多久优先听站点的 `Retry-After`，否则指数退避，两者都封顶。

use tokio::time::sleep;

use super::auth::{ApiKey, CfKeyState};
use super::error::CfError;
use super::model::{
    parse_download_url, parse_file, parse_files, parse_mod, parse_page, Mod, ModFile, ModFileQuery,
    ModPage, ModQuery, CF_API, MINECRAFT_GAME_ID,
};
use super::retry::RetryPolicy;
use super::transport::CfTransport;
// 百分号编码只有一份实现（Modrinth 那条路先写的），不为 CF 复制第二份：
// 复制的下场是两边对「空格 / 方括号」的处理迟早不同。
use crate::addon::online::modrinth::encode;

/// CurseForge 客户端。
#[derive(Clone, Copy)]
pub struct CurseForge<'a, T: CfTransport + ?Sized> {
    transport: &'a T,
    key: Option<&'a ApiKey>,
    policy: RetryPolicy,
}

impl<'a, T: CfTransport + ?Sized> CurseForge<'a, T> {
    /// 没配 Key 的客户端（每次调用都会得到 [`super::CfErrorKind::NotConfigured`]，**不发请求**）。
    #[must_use]
    pub const fn new(transport: &'a T) -> Self {
        Self {
            transport,
            key: None,
            policy: RetryPolicy::standard(),
        }
    }

    /// 配了用户自带 Key 的客户端。
    #[must_use]
    pub const fn with_key(transport: &'a T, key: &'a ApiKey) -> Self {
        Self {
            transport,
            key: Some(key),
            policy: RetryPolicy::standard(),
        }
    }

    /// 换一份重试策略（关掉、或跟随用户设置）。
    #[must_use]
    pub const fn with_policy(mut self, policy: RetryPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// 当前重试策略。
    #[must_use]
    pub const fn policy(&self) -> RetryPolicy {
        self.policy
    }

    /// Key 的配置状态（页面据此决定检索按钮能不能点，**不需要**先失败一次）。
    #[must_use]
    pub const fn key_state(&self) -> CfKeyState {
        if self.key.is_some() {
            CfKeyState::Configured
        } else {
            CfKeyState::Missing
        }
    }

    /// 检索的 URL（纯函数，单测直接钉）。
    #[must_use]
    pub fn search_url(&self, query: &ModQuery) -> String {
        let query = query.normalized();
        let mut url = format!(
            "{CF_API}/mods/search?gameId={MINECRAFT_GAME_ID}&classId={}&index={}&pageSize={}&sortField={}&sortOrder=desc",
            query.class.code(),
            query.index,
            query.page_size,
            query.sort.code()
        );
        if !query.text.is_empty() {
            url.push_str(&format!("&searchFilter={}", encode(&query.text)));
        }
        if let Some(version) = query
            .game_version
            .as_deref()
            .filter(|item| !item.is_empty())
        {
            url.push_str(&format!("&gameVersion={}", encode(version)));
        }
        if let Some(loader) = query.loader {
            url.push_str(&format!("&modLoaderType={}", loader.code()));
        }
        url
    }

    /// 单个 mod 详情的 URL。
    #[must_use]
    pub fn mod_url(&self, mod_id: u32) -> String {
        format!("{CF_API}/mods/{mod_id}")
    }

    /// 文件列表的 URL（游戏版本 + 加载器过滤都在 query 里）。
    #[must_use]
    pub fn files_url(&self, query: &ModFileQuery) -> String {
        let query = query.normalized();
        let mut url = format!(
            "{CF_API}/mods/{}/files?index={}&pageSize={}",
            query.mod_id, query.index, query.page_size
        );
        if let Some(version) = query
            .game_version
            .as_deref()
            .filter(|item| !item.is_empty())
        {
            url.push_str(&format!("&gameVersion={}", encode(version)));
        }
        if let Some(loader) = query.loader {
            url.push_str(&format!("&modLoaderType={}", loader.code()));
        }
        url
    }

    /// 单个文件的 URL。
    #[must_use]
    pub fn file_url(&self, mod_id: u32, file_id: u32) -> String {
        format!("{CF_API}/mods/{mod_id}/files/{file_id}")
    }

    /// 下载地址解析端点的 URL。
    #[must_use]
    pub fn download_url_url(&self, mod_id: u32, file_id: u32) -> String {
        format!("{CF_API}/mods/{mod_id}/files/{file_id}/download-url")
    }

    /// 检索一页。
    ///
    /// # Errors
    /// 见 [`super::CfErrorKind`]：缺 Key / Key 无效 / 限流 / 网络 / 站点 / 解析。
    pub async fn search(&self, query: &ModQuery) -> Result<ModPage, CfError> {
        let normalized = query.normalized();
        let body = self.get(&self.search_url(&normalized)).await?;
        let mut page = parse_page(&body)?;
        // 站点没回显 pageSize 时用请求值补齐，否则「还有下一页吗」会恒假（翻页卡死）。
        if page.page_size == 0 {
            page.page_size = normalized.page_size;
        }
        Ok(page)
    }

    /// 单个 mod 的详情。
    ///
    /// # Errors
    /// 见 [`super::CfErrorKind`]。
    pub async fn project(&self, mod_id: u32) -> Result<Mod, CfError> {
        let body = self.get(&self.mod_url(mod_id)).await?;
        parse_mod(&body)
    }

    /// 单个 mod 的**项目类别**（`classId`）。
    ///
    /// 为什么单开一条：文件端点（`/files/{fid}`）**不带** `classId`，而导入侧要靠它决定落盘目录
    /// （`mods/` 还是 `resourcepacks/`）。于是按 `projectID/fileID` 补全一条 CF 条目要两次 GET，
    /// 这是那条路的成本下限（不拉整份文件列表、不做指纹反查）。
    ///
    /// # Errors
    /// 见 [`super::CfErrorKind`]；站点没给 `classId` ⇒ `Ok(None)`（**不猜**类别）。
    pub async fn project_class(&self, mod_id: u32) -> Result<Option<u32>, CfError> {
        let body = self.get(&self.mod_url(mod_id)).await?;
        super::model::parse_class_id(&body)
    }

    /// 文件列表（站点的顺序 = 最新在前）。
    ///
    /// # Errors
    /// 见 [`super::CfErrorKind`]。
    pub async fn files(&self, query: &ModFileQuery) -> Result<Vec<ModFile>, CfError> {
        let body = self.get(&self.files_url(query)).await?;
        parse_files(&body)
    }

    /// 单个文件（拿摘要与站点给的 `downloadUrl` 就走这里）。
    ///
    /// # Errors
    /// 见 [`super::CfErrorKind`]。
    pub async fn file(&self, mod_id: u32, file_id: u32) -> Result<ModFile, CfError> {
        let body = self.get(&self.file_url(mod_id, file_id)).await?;
        parse_file(&body)
    }

    /// 解析一个文件的下载地址。
    ///
    /// 先看文件自己带的 `downloadUrl`（多数文件有）；为 `None` 时（作者关闭第三方分发后站点会
    /// 留空）才问 `/download-url` 端点。两步都是「没有就如实说没有」，不编一个猜测的 CDN 路径。
    ///
    /// # Errors
    /// 见 [`super::CfErrorKind`]；`/download-url` 答 `{"data": null}` 是 [`super::CfErrorKind::Unsupported`]。
    pub async fn download_url(&self, file: &ModFile) -> Result<String, CfError> {
        if let Some(url) = file
            .download_url
            .as_deref()
            .filter(|url| !url.trim().is_empty())
        {
            return Ok(url.to_string());
        }
        if file.mod_id == 0 || file.id == 0 {
            return Err(CfError::unsupported(
                "文件没有所属 mod id / 文件 id，无法解析下载地址",
            ));
        }
        let body = self
            .get(&self.download_url_url(file.mod_id, file.id))
            .await?;
        parse_download_url(&body)
    }

    /// 取一段正文：**先过 Key 闸门**，再按策略重试。
    async fn get(&self, url: &str) -> Result<String, CfError> {
        let Some(key) = self.key else {
            return Err(CfError::not_configured());
        };
        let headers = [
            ("x-api-key".to_string(), key.expose().to_string()),
            ("accept".to_string(), "application/json".to_string()),
        ];
        let mut attempt = 0u32;
        loop {
            let error = match self.transport.get(url, &headers).await {
                Ok(response) if response.is_success() => return Ok(response.body),
                Ok(response) => CfError::from_status(response.status, response.retry_after),
                Err(error) => error,
            };
            if !error.is_retryable() || attempt + 1 >= self.policy.attempts() {
                return Err(error);
            }
            let wait = self.policy.delay(attempt, error.retry_after());
            attempt += 1;
            if !wait.is_zero() {
                sleep(wait).await;
            }
        }
    }
}

impl<T: CfTransport + ?Sized> std::fmt::Debug for CurseForge<'_, T> {
    /// 手写 `Debug`，两条理由：
    ///
    /// 1. 派生的 `Debug` 会要求 `T: Debug`（「不完美的 derive」），于是只因为想在日志里打一行
    ///    客户端，就逼每个传输层实现去实现 `Debug`；
    /// 2. 更重要的：这条渲染路径**不碰传输层**（它可能持有连接池与内部 URL），Key 走
    ///    [`ApiKey`] 的脱敏 `Debug`。于是「顺手 `{:?}` 一下客户端」永远不会印出 Key。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CurseForge")
            .field("key", &self.key)
            .field("key_state", &self.key_state())
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}
