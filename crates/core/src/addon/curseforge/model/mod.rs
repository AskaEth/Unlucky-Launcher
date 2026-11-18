//! CurseForge v1 的响应形状与请求（**按站点原文建模，不翻译成「通用内容模型」**）。
//!
//! | 类型 | 端点 |
//! |------|------|
//! | [`ModQuery`] → [`ModPage`] | `GET /mods/search` |
//! | [`Mod`] | `GET /mods/{id}` |
//! | [`ModFileQuery`] → [`Vec<ModFile>`] | `GET /mods/{id}/files` |
//! | [`ModFile`] | `GET /mods/{id}/files/{fileId}` |
//! | `String`（下载地址） | `GET /mods/{id}/files/{fileId}/download-url` |
//!
//! # 为什么不与 Modrinth 共用一套模型
//!
//! 两家的 JSON 方言几乎没有一处同名：`id`（数字）vs `project_id`（base62）、`name`/`summary` vs
//! `title`/`description`、`downloadCount` vs `downloads`、版本列表还要再套一层 `latestFilesIndexes`。
//! 共用一套要翻译两次、错一处两边都错，所以各按各的形状建模（与 `online::model` 的定稿一致），
//! 差异在**上报给上层时**才收敛（presenter 把两者都映成 `ContentHit`）。
//!
//! # 拆三个文件
//!
//! 请求形状（[`query`]）、响应形状（[`types`]）、把前者读成后者的解析（[`parse`]）分开，
//! 因为「站点字段名与 DTO 字段名不是一回事」这类错误只会在 `parse` 里出现，混在一起最容易看漏。

mod parse;
mod query;
mod types;

pub use parse::{
    parse_class_id, parse_download_url, parse_file, parse_files, parse_mod, parse_page,
};
pub use query::{
    CfClass, CfLoader, CfReleaseType, CfSort, ModFileQuery, ModQuery, CF_API, MAX_PAGE_SIZE,
    MINECRAFT_GAME_ID,
};
pub use types::{FileIndex, Mod, ModFile, ModPage};
