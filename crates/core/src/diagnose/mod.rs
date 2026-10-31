//! 崩溃诊断：五类信号采集（含 hs_err_pid 解析）、YAML 规则引擎（59 条规则起步，可热更新）、
//! exit0 静默失败检测、结构化诊断结论。
//!
//! 设计定稿：`DevDocs/PlanDoc/06-launch-and-diagnostics.md` §2（采集 / 规则 / 流水线）。
//! 定位：规则引擎为主、AI 补位；AI 拿到的应是「规则结论 + 相关片段」而非裸日志
//! （`DevDocs/PlanDoc/02-ai-agent-design.md` §3.2）。
//!
//! 规划中的公共面（M15）：信号采集、规则匹配、诊断报告、规则热加载。

// 只有这五个顶层模块；子模块（collectors 下的 signals/sources/environment、engine 下的
// finding/synthetic、hs_err 下的 hs_err_report、rules 下的 fix_action/fix_params）由各自的
// 父文件用 #[path] 声明。这里**不要**重复声明，否则同一文件会被编译成两份不同类型。
pub mod collectors;
pub mod engine;
pub mod hs_err;
pub mod loader;
pub mod rules;
