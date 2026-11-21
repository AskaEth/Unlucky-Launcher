//! CurseForge 客户端的用例（**一条都不出网**）。
//!
//! 素材是录制好的 CF v1 响应（`fixtures.rs`）+ 一个按前缀回放的传输层（`support.rs`），
//! 覆盖检索、详情、文件列表、下载地址解析、六档失败分类、重试的上下界、以及「远端条目 →
//! `ModRecord`」那一步。
//!
//! `transport.rs` 是唯一的例外口径：它不注入替身，而是起**本机回环**假服务端去跑真的
//! `EngineTransport`（真 `reqwest`）——因为「`send()` / `text()` 有没有上界」这件事，
//! `async` 直接返回的替身根本走不到（M423）。
//!
//! 口径提醒：这些用例证明的是**拼装、解析与判定**，不证明「真实站点今天回什么」——
//! 后者写在 M316 报告的「没验的部分」里。

mod api;
mod errors;
mod fixtures;
mod record;
mod retry;
mod support;
mod transport;
