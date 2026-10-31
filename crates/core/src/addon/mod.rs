//! Mod 与整合包：加载器安装（Fabric/Quilt/NeoForge/Forge）、整合包双格式导入导出、
//! 本地 Mod 管理、CurseForge / Modrinth 客户端。
//!
//! 设计定稿：`DevDocs/PlanDoc/08-mod-and-recommend.md`（决策 D10）。
//! 安全要点：整合包路径穿越双检；AI 推荐走「结构化过滤在前 + LLM 排序在后 + 来源链接防幻觉」。
//!
//! # 当前公共面
//!
//! - [`loader`]（P2 · M32）：加载器安装——官方元数据 → 带 `inheritsFrom` 的 loader 版本 JSON
//!   → 补库清单，落盘与实例登记复用 `core::game::install`（M27）。
//!
//! 规划中：整合包导入导出（D10 §2）、本地 mod 管理（§3）、CF/Modrinth 客户端（§5）。

pub mod loader;

pub use loader::{
    LoaderError, LoaderErrorKind, LoaderInstallPlan, LoaderKind, LoaderPlanInput, LoaderRelease,
    LoaderSource, LoaderSupport,
};
