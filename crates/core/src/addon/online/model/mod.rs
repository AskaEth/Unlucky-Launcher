//! Modrinth v2 的响应形状与检索请求（**按站点原文建模，不翻译成「通用内容模型」**）。
//!
//! 每个公开类型都对着一个真实端点：
//!
//! | 类型 | 端点 |
//! |------|------|
//! | [`SearchQuery`] → [`SearchPage`] | `GET /v2/search` |
//! | [`ProjectDetail`] | `GET /v2/project/{id or slug}` |
//! | [`VersionQuery`] → [`Vec<Version>`] | `GET /v2/project/{id}/version` |
//! | [`Version`] | `GET /v2/version_file/{hash}?algorithm=sha1\|sha512` |
//!
//! # 解析纪律
//!
//! 站点会加字段、也会给 `""` 表示「没有」——所以：
//!
//! - 一律 `#[serde(default)]`，**没给就是空的**，缺字段不炸整页；
//! - 只把**形状**当硬要求（`hits` 必须是数组、project 必须是对象），形状不对就是
//!   [`super::OnlineErrorKind::Malformed`]，不静默给空页冒充「没结果」；
//! - 未知字段不保留（这里不是导入导出的无损通道，是只读检索）。
//!
//! # 为什么拆成三个文件
//!
//! 一个 600+ 行的 `model.rs` 已经越过本仓的 400 行硬线，而且三种东西混在一起时最容易看漏
//! 「站点字段名与 DTO 字段名不是一回事」：请求形状（[`query`]）、响应形状（[`types`]）、
//! 以及把前者读成后者的解析（[`parse`]）。拆开后 `parse` 里每一处 `#[serde(rename)]`/
//! `alias` 都能单独读。

mod parse;
mod query;
mod types;

pub use parse::{known_loaders, parse_project, parse_search, parse_version, parse_versions};
pub use query::{ContentType, SearchQuery, SortIndex, KNOWN_LOADERS};
pub use types::{
    FileHashes, ProjectDetail, ProjectHit, SearchPage, Version, VersionFile, VersionQuery,
    VersionType,
};
