//! 在线内容源的离线用例（**不打真实网络**）。
//!
//! 全部素材是录制好的 Modrinth v2 响应（`fixtures.rs`）+ 假字节（`support::FakeArtifact`）：
//! 覆盖检索、项目详情、版本列表、按摘要反查、下载复核、补全路径与离线分类。
//!
//! 口径提醒：这些用例证明的是**解析与判定**，不证明「真实站点今天回什么」——后者写在
//! M162 报告的「没验的部分」里。

mod api;
mod classify;
mod download;
mod fixtures;
mod support;
