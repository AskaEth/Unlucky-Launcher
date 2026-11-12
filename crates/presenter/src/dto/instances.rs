//! 实例相关 DTO：磁贴与列表视图的唯一数据源。
//!
//! 按关注点拆成四个子模块（M222 为了给 LiteLoader 加入口而拆，纯搬移 + 新增一支）：
//!
//! | 子模块 | 内容 |
//! |---|---|
//! | [`loader`] | 加载器词表（磁贴徽标 + 向导可选）、[`LoaderInfo`] |
//! | [`runtime`] | [`Progress`] / [`LaunchStage`] / [`InstanceRuntime`] / [`InstanceSummary`] |
//! | [`create`] | 新建实例向导的提交请求 [`CreateRequest`] |
//!
//! 路径对外**一个字没变**（`unlucky_presenter::dto::InstanceSummary` 等仍从这里取），
//! 拆的是文件不是 API。

mod create;
mod loader;
mod runtime;

#[cfg(test)]
mod tests;

pub use create::CreateRequest;
pub use loader::{LoaderInfo, LoaderKind};
pub use runtime::{InstanceRuntime, InstanceSummary, LaunchStage, Progress};

pub use super::launch_failure::{LaunchFailureCause, LaunchFailureStage, LaunchFailureView};

/// 实例标识：core::game 生成的稳定 id，app 视为不透明字符串。
pub type InstanceId = String;
