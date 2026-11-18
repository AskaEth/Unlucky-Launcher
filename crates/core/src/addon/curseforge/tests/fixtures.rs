//! 录制下来的 CurseForge v1 响应（**形状按站点原文，字段抄真**；数值是示意，不是某天的真值）。
//!
//! 三条刻意的细节，每一条都对着一个真实的坑：
//!
//! - 文件的 `gameVersions` 是**混装**的（`["1.20.1", "1.20.2", "Forge", "Client", "Server"]`）：
//!   整份当「支持的游戏版本」，页面上就会出现「支持的版本：Forge / Client」；
//! - 第二个文件 `downloadUrl` 是 `null`：作者关闭第三方分发时站点就是给 `null`（不是空串），
//!   这时只能问 `/download-url` 端点；
//! - `hashes` 里有 `algo: 99`（站点以后可能加算法）：认不出的算法不许让整页解析失败；
//!   认得出的 `algo: 1` 的十六进制是大写，解析要归一成小写（sha1 比对要能直接用）。

/// `GET /mods/search?gameId=432&classId=6&searchFilter=sodium`（一页两条）。
pub(super) const SEARCH: &str = include_str!("../fixtures/search_mods.json");

/// `GET /mods/394468`（单个 mod）。
pub(super) const MOD_DETAIL: &str = include_str!("../fixtures/mod_detail.json");

/// `GET /mods/238222/files`（两个文件，第二个没有 `downloadUrl`）。
pub(super) const MOD_FILES: &str = include_str!("../fixtures/mod_files.json");

/// `GET /mods/238222/files/4455668/download-url`（站点给的签名直链）。
pub(super) const DOWNLOAD_URL: &str = include_str!("../fixtures/download_url.json");

/// 同上，但站点答 `{"data": null}` = 这个文件没有可用下载地址。
pub(super) const DOWNLOAD_URL_ABSENT: &str = include_str!("../fixtures/download_url_absent.json");

/// `GET /mods/search` 的**残页**：条目里只有 `id`（字段缺失的容错判据）。
pub(super) const SEARCH_SPARSE: &str = include_str!("../fixtures/search_sparse.json");
