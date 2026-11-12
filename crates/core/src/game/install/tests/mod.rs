//! `game::install` 的单元测试：计划正确性 / 跳过与幂等 / 实例登记。
//!
//! 刻意**不打真实网络**：计划是纯计算，跳过判定只看盘，实例登记只看文件系统。
//! 需要"引擎"的用例只要求它"不联网也能跑"（全部文件已就位 → 全跳过）。

mod execute;
mod instance;
mod manifest_write;
mod plan;
