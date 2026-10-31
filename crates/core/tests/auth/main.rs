//! `core::auth` 的集成测试入口。
//!
//! 每个子模块对应 mission 的一项验收：
//! - `offline_uuid`：离线 UUID 与 JDK（= HMCL 算法）**逐字节对拍**的已知向量；
//! - `store_separation`：元数据/凭据分文件、原子写、写序、容错与孤儿清理；
//! - `microsoft_flow`：device-code 全链（含 pending / slow_down / 取消 / 超时 / XErr）；
//! - `microsoft_diagnostics`：非 2xx 正文诊断（JSON/HTML/空体分流）+ 失败日志落盘（M38）；
//! - `microsoft_refresh`：refresh 轮换、profile 变脸防线、validate 语义、404 二义；
//! - `authlib_api`：Yggdrasil 四动作的报文级对接。

mod support;

mod auth_failure_journal;
mod authlib_api;
mod microsoft_diagnostics;
mod microsoft_flow;
mod microsoft_refresh;
mod offline_uuid;
mod store_separation;
