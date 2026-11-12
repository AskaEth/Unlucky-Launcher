//! 登记「已经铺在盘上的版本」（从其他启动器的 `.minecraft` 只读接入，M221）。
//!
//! 依据：`DevDocs/artifacts/m218/SURVEY-p4.md`（④ 从其他启动器导入数据）与
//! `DevDocs/00-债务台账.md` §21 的第 ④ 条；模型口径仍按 D20（实例隔离只改**运行目录**，
//! `versions/` `assets/` `libraries/` 恒共享，见 [`crate::game::isolation::SHARED_LAYERS`]）。
//!
//! # 一句话纪律
//!
//! **登记只读外部、只写自己**：本模块从不往游戏根写一个字节（不搬文件、不补清单、不改 mtime），
//! 唯一的写盘是 `profiles.json` + `config/instances/<id>.json`（+ 缺省运行目录的准备，若选了它）。
//! 因此「导入前后对 `versions/` `libraries/` 做全量哈希必须完全不变」是可以用测试钉死的事实。
//!
//! # 为什么要有它（现状的洞）
//!
//! [`GameLayout::installed_versions`] 早就会扫 `versions/`，但**没有任何代码把扫到的东西变成实例**。
//! 于是「别人的 `.minecraft` 已经装好了 1.20.1」这件事在 ULL 里看不见，用户只能重装一遍
//! （下载全部资源）。这个模块把那批版本变成实例档案，内容一个字节不搬。
//!
//! # 缺省隔离档**不能**沿用全局 `isolated`（本模块最要紧的一条）
//!
//! 全局 `instances.mode` 缺省是 `isolated`，而 `isolated` 的运行目录正是 `versions/<versionId>/`
//! （[`crate::game::isolation::derive_game_dir`]）。若登记沿用这条缺省，第一次启动就会在
//! **别人的版本目录**里长出 `mods/` `saves/` `config/` `logs/` —— 不覆盖任何文件，但与
//! 「多启动器共存」的意图相反。所以 [`AdoptTarget`] 的缺省值是
//! [`AdoptTarget::InstanceDir`]（落 ULL 数据区 `<data>/instances/<实例目录名>`），
//! 想共用/写进对方目录必须**显式**选，并且 [`target_notes`] 会把后果说清楚。
//!
//! # 分层
//!
//! - `candidate`：候选与逐项**结构化失败**（坏 JSON / 缺父 / 环 / 不可读 / 越界 / 符号链接）；
//! - `error`：整次调用失败（[`AdoptError`]，带稳定错误码）；
//! - `inspect`：只读扫描与逐项检查（[`scan`] / [`inspect`]）；
//! - `paths`：路径判据（版本 id 能不能当路径、符号链接有没有跑出游戏根）；
//! - `fspaths`：路径语义（真实化 / Win32 尾点尾空格归一）与写盘面判据（配置在 `.ULL` 内、
//!   默认写盘面不得与受保护游戏树重叠）；
//! - `run_dir`：运行目录派生（含**碰撞消歧**）与「缺省目录必须落在数据区」；
//! - `plan`：把候选 + 用户选择算成一次登记的落点（[`plan`]），真正落盘是 [`adopt`]。
//!
//! # 不做什么
//!
//! - **不读账户凭据**：`launcher_profiles.json` / `launcher_accounts.json` / HMCL 的
//!   `accounts.json` 一个都不碰（`AGENTS.md` §6 同族纪律）；
//! - **不做多根**（D20 未改）：扫的永远是**当前**游戏根（`core.game_home`），
//!   「同时管多个 `.minecraft`」是另一个更贵的模型改动，见 SURVEY-p4 §4.2；
//! - **不下载、不校验完整性**：版本 JSON 里 `libraries[].downloads.artifact.url` 指向 maven
//!   中心是正常形态，登记不因此拒绝，但也不假装校验过（人读输出里如实写成「未校验」）。

pub mod candidate;
mod error;
mod fspaths;
mod inspect;
mod paths;
mod plan;
mod run_dir;

pub use candidate::{AdoptCandidate, AdoptProblem, AdoptTarget, RegisteredInstance};
pub use error::AdoptError;
pub use inspect::{inspect, scan, AdoptScan};
pub use plan::{adopt, plan, target_notes, AdoptOutcome, AdoptPlan, AdoptRequest};
