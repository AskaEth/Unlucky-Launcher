//! 本地 Mod 的**列表**与**启停**（M224 · MODTOGGLE）。
//!
//! 设计定稿：`DevDocs/PlanDoc/08-mod-and-recommend.md` §3（三条结论原样落地）。
//!
//! # 三层里的第一层
//!
//! §3 的结论是「Mod 启停 = 新建三层（core 读写目录 + presenter DTO + 实例详情页），不是给
//! 内容页接一个开关」，并特别点名**不要**给内容页加第五个 `ContentKind`（会打断 4 处穷尽 match）。
//! 本模块就是那第一层，另外两层见 `crates/presenter/src/dto/mods.rs` 与
//! `crates/app-iced/src/pages/instances/mods.rs`。
//!
//! # 唯一 `.disabled` 判据
//!
//! 收敛前仓里有两份逐字同构的后缀判据（`addon::export::scan::is_disabled_path` 与
//! `addon::import::plan::paths::suffix_variant`）。M218 的要求是「做这块时第一步先收敛成一份，
//! 别让第三份出生」，所以判据只有 [`suffix`] 一处，导出/导入两侧改为**委派**（签名与行为不变，
//! 收敛 diff 为零行为变化）。启停要改写文件名，比"判一下"多两件事——保住原始大小写、保住
//! `.old` 的位置——这两件也只能和判据同源，否则会出现「判禁用用一套、改名字用另一套」。
//!
//! # 单根语义（D20）
//!
//! 入口一律是**派生好的运行目录**（[`crate::game::isolation::derive_game_dir`] 的结果），
//! 本模块只碰 `<run_dir>/mods/`：
//!
//! - 不自己去猜游戏根，也不做「隔离侧 `versions/<id>/mods` + 共享侧 `.minecraft/mods` 合并」
//!   这种双根视图——派生是启动与列表**同一个**答案的唯一来源；
//! - 只往下取一层：`mods/` 下的子目录不递归（游戏也不递归）；
//! - `mods/` 本身是符号链接时**拒绝**（不跟随），否则改名会落到实例目录之外。
//!
//! # 模块
//!
//! | 文件 | 职责 |
//! |---|---|
//! | [`suffix`] | `.disabled` / `.old` 的唯一判据与改名目标名（含四态枚举） |
//! | [`list`] | 列 `<run_dir>/mods/`（只读、逐条登记跳过原因） |
//! | [`toggle`] | 启停 = 在 `mods/` 里改名（五条守卫、幂等、不覆盖） |
//!
//! # 已知边界
//!
//! - **不判断游戏是否在跑**：进程正持有这些 jar（Windows 上 rename 会失败），失败按
//!   [`error::ModError::Rename`] 如实报出来。要不要在上层挡「运行中不许改」是宿主层的策略
//!   （presenter 侧已挡，见 `host::mods`）。
//! - **不解析 jar**：列表给的是文件名，`-版本号` 猜解仍归 `diagnose::environment::ModRecord`，
//!   这里不复制第二份启发式。

mod error;
mod list;
mod suffix;
mod toggle;

pub use error::ModError;
pub use list::{mods_dir, scan, ModEntry, ModScan, ModSkip, SkipReason, MODS_DIR};
pub use suffix::{
    disabled_name, display_name, enabled_name, is_disabled_path, renamed_for, suffix_variant,
    SuffixVariant, DISABLED_SUFFIX, OLD_SUFFIX,
};
pub use toggle::{ensure_dir, set_enabled, ModToggle};

#[cfg(test)]
mod tests;
