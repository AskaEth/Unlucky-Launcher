//! CurseForge 官方 API 客户端（P2 · M316）——**用户自带 Key** 的那一半。
//!
//! # 这个模块补的是哪条缝
//!
//! `online`（M162）明确留了这一半没做：「CurseForge 需要 `x-api-key`，本轮不碰」。后果是量化过
//! 的：CF 官方清单只有 `projectID/fileID`，没有客户端就换不出文件名与下载地址
//! （`addon::import::fetch` 的 `PackFetcher` 把这类条目如实记成「缺文件」），于是一个真实的
//! CurseForge 整合包**一个 mod 都装不上**。本模块就是那个缺口：检索 / 详情 / 文件列表 /
//! 下载地址解析四条路。
//!
//! # Key：用户自带，缺了是一个**具名状态**
//!
//! CurseForge 官方 API 的每一次请求都要 `x-api-key`。闭源启动器把自家 Key 编进二进制，本项目
//! **不这样做**：那是别人的配额与授权，分发一份进去等于全世界的检索都挂在同一把钥上，钥一被吊销
//! 所有人一起坏。所以 Key 只能来自用户——宿主先读设置 `online.curseforge.api_key`，环境变量
//! `ULL_CF_API_KEY` 只是兜底（**core 不读环境变量**，也不读设置：它只收一个 [`ApiKey`]）。
//!
//! 于是「没配 Key」不是异常，是一个**必须能被上层区分出来**的正常状态：
//!
//! - 类型是 [`CfErrorKind::NotConfigured`]（不是 panic、不是空列表、不是静默 403）；
//! - 判定发生在**发请求之前**（[`CurseForge::new`] 造出的客户端一个字节都不发）；
//! - 上层可以不花钱地预判（[`CurseForge::key_state`] → [`CfKeyState`]），只在配了 Key 时才画可点的按钮。
//!
//! 三条各自对应的错误做法都让用户去做一件没用的事：把「没配 Key」画成「没有匹配的内容」⇒ 他去换
//! 关键词；画成「服务端出错」⇒ 他一直点重试；答一个静默 403 ⇒ 他根本不知道去哪里改。
//!
//! # 错误分类
//!
//! 每一档对应一个不同的用户动作，见 [`CfErrorKind`] 的表：缺 Key（去填）/ Key 无效（去换）/
//! 限流（等一会儿，带站点的 `Retry-After`）/ 网络 / 查无此项 / 站点错 / 解析失败 / 这条路走不通。
//! 全部是**结构化**的（不是字符串），presenter 直接一一映射成 `ContentFailureKind`，文案留给上层。
//!
//! # 网络层（零新依赖）
//!
//! 只用仓内已有的 `reqwest`（经由 `core::download` 引擎的 client）+ `tokio` + `serde_json`。
//! 生产传输层 [`EngineTransport`] **只发一次请求**、不做决策；重试与退避在 [`RetryPolicy`]
//! （有上界、可关、听站点的 `Retry-After`）。本模块不新建 `reqwest::Client`，也不持有第二个连接池。
//!
//! # 模块
//!
//! | 文件 | 职责 |
//! |------|------|
//! | [`model`] | 站点的 JSON 方言（`id`/`name`/`summary`/`downloadCount`/`latestFilesIndexes`）+ 检索请求 |
//! | [`transport`] | HTTP 出口 trait（**带请求头**、回状态码与 `Retry-After`）+ 生产实现 |
//! | [`client`] | 六个端点：URL 拼装、Key 闸门、重试循环、响应解析 |
//! | [`error`] | 状态码 → 结构化错误（**分级判据只在这里**） |
//! | [`auth`] | [`ApiKey`]（`Debug` 脱敏、无公开取值路）+ [`CfKeyState`] |
//! | [`retry`] | [`RetryPolicy`]：次数上界、指数退避、`Retry-After` 优先、可关 |
//! | [`record`] | 远端条目 → [`crate::diagnose::collectors::ModRecord`]（数据层落地） |
//!
//! # 已知边界（本切片**不做**的事）
//!
//! - **presenter 的检索已接线（M317）**：`host::addon::sources` 不再自己拼 CF 的 URL、不再带
//!   请求头、也不再手写解析——它委托 [`CurseForge::search`]，DTO 映射走
//!   `dto::content::curseforge`。导入侧的 `PackFetcher::curse_file` 同样接到本模块（见
//!   `addon::import::fetch::curse`）。仍未接的是 **UI 的设置页**（Key 的输入框在另一条 mission）
//!   与 CF 的**详情/版本列表**（`host::addon::sources::detail` 仍是明确的 `Unsupported`）。
//! - **不做 CF 的依赖端点**（`/mods/{id}/dependencies`）：于是 [`record`] 产出的 `ModRecord`
//!   `deps` 恒为空（安全的一侧，见该文件）。
//! - **不做指纹反查**（`/mods/search?gameId=&index=`，CF 的按指纹查要求 `fingerprint` 参数）：
//!   导出侧的 [`crate::addon::export::RemoteCatalog`] 走的是另一条（同内容不同接口）的路。

pub mod auth;
pub mod client;
pub mod error;
pub mod model;
pub mod record;
pub mod retry;
pub mod transport;

pub use auth::{ApiKey, CfKeyState};
pub use client::CurseForge;
pub use error::{CfError, CfErrorKind};
pub use model::{
    parse_download_url, parse_file, parse_files, parse_mod, parse_page, CfClass, CfLoader,
    CfReleaseType, CfSort, FileIndex, Mod, ModFile, ModFileQuery, ModPage, ModQuery, CF_API,
    MAX_PAGE_SIZE, MINECRAFT_GAME_ID,
};
pub use retry::RetryPolicy;
pub use transport::{CfFuture, CfResponse, CfTransport, EngineTransport};

#[cfg(test)]
mod tests;
