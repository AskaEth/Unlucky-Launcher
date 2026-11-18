//! LiteLoader 安装链路的集成测试（`tests/liteloader*` 这个 test target 的入口）。
//!
//! 判据链：**真实元数据夹具 → 解析 → 合成清单 → 计划 → 落盘 → 继承解析 → 启动参数顺序**。
//! 全程不联网：LiteLoader 的库没有 sha1/size，`execute` 的跳过判据是「存在即信」，
//! 所以先把每个待下文件按计划里的落点写一份占位字节，执行期就只做「跳过 + 写版本 JSON」。
//!
//! 夹具来源（可追溯）：
//! - `fixtures/liteloader-versions.json` = 2026-11 从 `https://dl.liteloader.com/versions/versions.json`
//!   真取的响应（200 / 49129 字节 / 16 个原版版本）裁剪到两个原版版本，**字段名与嵌套结构原样**；
//! - `metadata.xml` = 同一次探测里 `repo.mumfrey.com/…/liteloader/1.12.2-SNAPSHOT/maven-metadata.xml`
//!   的原样响应（200 / 358 字节）；
//! - 原版 / Forge 1.12.2 的版本 JSON 是按官方清单形状写的最小夹具（Forge 那侧的
//!   `minecraftArguments` 与 `crates/core/src/addon/loader/tests/forge.rs` 的实测夹具一致）。
//!
//! 未覆盖（如实留档）：真实下载（走 `core::download` 引擎的镜像/退避，已有 M13 的单测覆盖）、
//! 真实 MC 启动、`dl.liteloader.com` / `repo.mumfrey.com` 的持续可用性。

mod guards;
mod install;
mod support;
