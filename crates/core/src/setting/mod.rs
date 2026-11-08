//! 配置与数据目录：三模式数据根（Portable 默认 / Standard / Custom）、schema 化配置、原子写、迁移收据。
//!
//! 设计定稿：`DevDocs/PlanDoc/04-data-and-config.md`（决策 D6）。
//! 刻意把 setting 放在 core 而不是 UI 层——这是 HMCL 的架构债
//! （`DevDocs/analyse/01-architecture-overview.md` §3.5），我们反过来做。
//!
//! 与其它模块的契约：所有模块的配置读写统一走本模块的 schema API，
//! 模块自有配置键需先在 `DevDocs/SubAgentsTalk/` 报备（见该目录 README 的对齐点）。
//! 暴露给各模块的签名清单见 `DevDocs/SubAgentsTalk/setting-接口约定.md`。
//!
//! 公共面（M12 已落地）：
//! - [`schema`]：`$schema` / `schemaVersion` 标识、8 态兼容判定 → 三档访问权；
//! - [`spec`] + [`catalog`]：键元数据（类型/范围、默认值）与键目录；
//! - [`policy`]：AI 可写白名单与敏感分级（`setting::spec::AiPolicy` 仍是兼容别名）；
//! - [`keys`] / [`instance_keys`]：`launcher.json`、`profiles.json`、`instances/<id>.json` 的键表；
//! - [`appearance`]：D17「外观（高级）」键簇的取值快照与校验（主题三态 / 背景图 / 面板透明度 /
//!   玻璃拟态 / 强调色）；
//! - [`document`]：内存文档（点分键、脏字段、未知字段保留、默认值不落盘）；
//! - [`store`]：单文件加载降级 + 原子保存 + 坏文件备份；
//! - [`save`] / [`save_worker`]：200ms 去抖攒批与后台保存线程；
//! - [`migrate`]：`schemaVersion N → N+1` 迁移链 + 收据闸门。
//! - [`update_prefs`]：自更新偏好（旧 `cache/update/prefs.json`）→ `launcher.json` 的一次性迁移
//!   （M161：幂等、源指纹闸门、旧文件退役、收据可查）。

pub mod appearance;
pub mod catalog;
pub mod document;
pub mod error;
pub mod instance_keys;
pub mod json_path;
pub mod keys;
pub mod migrate;
pub mod policy;
pub mod save;
pub mod save_worker;
pub mod schema;
pub mod spec;
pub mod store;
pub mod update_prefs;
