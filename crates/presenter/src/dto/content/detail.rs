//! 内容库的**失败分类**与**详情/版本列表** DTO（M162：在线内容源接上真数据）。
//!
//! 从 `dto/content.rs` 拆出（那个文件已贴 400 行硬线）。两件事放一起，因为它们都是
//! 「检索之后」的话：一次请求失败了要能说清是哪一类，选中一个项目要能拿到版本列表。
//!
//! # 为什么失败要分大类
//!
//! 无网时画「未联网，检查网络后重试」、站点 404 时画「换个关键词」是**两种界面**。
//! 只有字符串就只能靠 `contains("网络")` 猜文案，一改文案就失效；所以大类在 core 那一侧
//! （`OnlineErrorKind`）就分好了，这里只做一一映射，不重新判。
//!
//! # 详情为什么单独一条通道
//!
//! 检索结果里只有「支持哪些版本/加载器」（facets），拿不到「有哪些版本可以装」。
//! 版本列表是另一个端点（`/v2/project/{id}/version`），而且**只在选中某个项目时才需要**——
//! 跟着检索一起拉等于每页多 N 次请求。所以详情是一条独立命令/事件，页面选中才发。

use serde::{Deserialize, Serialize};

use super::ContentSource;
use crate::dto::LoaderKind;

/// 一次检索/详情失败的**大类**（M162：把「没联网」与「查无此项」分开）。
///
/// 为什么不是 `Option<String>`：无网时画「未联网，检查网络后重试」、站点 404 时画「换个关键词」
/// 是两件事；只有字符串就只能靠 `contains("网络")` 猜，一改文案就失效。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContentFailureKind {
    /// 没联网 / 连不上站点（core 的 `OnlineErrorKind::NotConnected`）
    Offline,
    /// 站点说没有（404）
    NotFound,
    /// 站点出错（5xx / 退避后仍失败 / 限速）
    Server,
    /// 这条路走不通（缺密钥 / 没有可用下载地址 / 摘要格式不对）
    Unsupported,
    /// **站点有这个项目，但目标版本/加载器下没有条目**（M191；core 的 `FailureKind::Incompatible`）
    ///
    /// 与 [`Self::NotFound`] / [`Self::Offline`] 是**两种用户动作**：这里换版本或换加载器才有救，
    /// 重试同样的条件只会得到同一条结论。以前它落在 `Unsupported`（与「缺密钥」共用一档），
    /// 于是界面没法单独说「换个版本」——M191 把这一档单独拎出来。
    Incompatible,
    /// 其它（解析失败、本地 IO……）
    Other,
}

impl ContentFailureKind {
    /// 稳定字符串 id（日志 / 用例断言用，**不要改名**）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::NotFound => "not_found",
            Self::Server => "server",
            Self::Unsupported => "unsupported",
            Self::Incompatible => "incompatible",
            Self::Other => "other",
        }
    }

    /// 是不是「所选版本/加载器下没有条目」（换版本/加载器才有救，重试没用）。
    #[must_use]
    pub const fn is_incompatible(self) -> bool {
        matches!(self, Self::Incompatible)
    }

    /// 是不是「没联网」（页面据此画独立的离线态，而不是通用失败态）。
    #[must_use]
    pub const fn is_offline(self) -> bool {
        matches!(self, Self::Offline)
    }
}

/// 一次检索/详情失败（大类 + 一句人读原因）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentFailure {
    /// 大类
    pub kind: ContentFailureKind,
    /// 人读说明（原文来自 core / 站点；**不是**界面文案）
    pub detail: String,
}

impl ContentFailure {
    /// 构造。
    #[must_use]
    pub fn new(kind: ContentFailureKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    /// 没联网。
    #[must_use]
    pub fn offline(detail: impl Into<String>) -> Self {
        Self::new(ContentFailureKind::Offline, detail)
    }

    /// 这条路没通。
    #[must_use]
    pub fn unsupported(detail: impl Into<String>) -> Self {
        Self::new(ContentFailureKind::Unsupported, detail)
    }

    /// 没联网吗。
    #[must_use]
    pub const fn is_offline(&self) -> bool {
        self.kind.is_offline()
    }
}

impl std::fmt::Display for ContentFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}：{}", self.kind.as_str(), self.detail)
    }
}

/// 「取项目详情 + 版本列表」的请求（M162）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentDetailRequest {
    /// 项目 id
    pub project: String,
    /// 来源（详情端点目前只接了 Modrinth；CurseForge 会明确报「未接通」）
    pub source: ContentSource,
    /// 目标游戏版本（选版本用；`None` = 不限）
    pub game_version: Option<String>,
    /// 目标加载器（选版本用；`None` = 不限）
    pub loader: Option<LoaderKind>,
}

/// 一个可用版本（详情页「版本列表」的一行）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentVersionView {
    /// 版本 id
    pub id: String,
    /// 版本名
    pub name: String,
    /// 版本号
    pub number: String,
    /// 版本类型（`release` / `beta` / `alpha`）
    pub kind: String,
    /// 支持的游戏版本
    pub game_versions: Vec<String>,
    /// 支持的加载器
    pub loaders: Vec<String>,
    /// 发布日期（站点原文）
    pub published: String,
    /// 主文件名（`None` = 站点没给）
    pub file_name: Option<String>,
    /// 主文件字节数（`None` = 站点没给）
    pub file_size: Option<u64>,
}

/// 项目详情 + 版本列表（M162）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentDetail {
    /// 项目 id
    pub project: String,
    /// 标题
    pub title: String,
    /// 简介
    pub description: String,
    /// 长文（markdown 原文；页面只当纯文本画前几行）
    pub body: String,
    /// 可用版本（**最新在前**，已按请求的版本/加载器过滤）
    pub versions: Vec<ContentVersionView>,
}

impl ContentDetail {
    /// 版本数量（页面「N 个可用版本」直接用）。
    #[must_use]
    pub fn version_count(&self) -> usize {
        self.versions.len()
    }
}
