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

use serde::{Deserialize, Serialize};

mod create;
mod loader;
mod runtime;

#[cfg(test)]
mod tests;

pub use create::CreateRequest;
pub use loader::{LoaderInfo, LoaderKind};
pub use runtime::{InstanceRuntime, InstanceSummary, LaunchStage, Progress};

pub use super::launch_failure::{
    AdvisoryEvidence, AdvisoryTier, JavaGuardDirection, LaunchBlockedView, LaunchFailureCause,
    LaunchFailureStage, LaunchFailureView,
};

/// 实例标识：core::game 生成的稳定 id，app 视为不透明字符串。
pub type InstanceId = String;

/// **自动发现**实例的 id 前缀（M273）：`versions/<id>` 里扫出来、没有档案的版本用
/// `ext-<hash>` 作实例 id。
///
/// 为什么让它是公开常量而不给 [`InstanceSummary`] 加字段：加字段要改 24 处结构体字面量
/// （横跨 presenter / agent / cli / app-iced），而「这个实例是不是自动发现的」在界面上的
/// 用途只有一处——详情页那句「为什么它按独立目录 / 共用目录跑」。前缀是**稳定契约**，
/// 由 `host::game` 的 `external_instance_id` 唯一生产（M274 多根时前缀不变，哈希里加根）。
pub const DISCOVERED_ID_PREFIX: &str = "ext-";

/// 这个实例是不是「从盘上自动发现」的（没有用户 / 向导建过的档案）。
#[must_use]
pub fn is_discovered_instance(id: &str) -> bool {
    id.starts_with(DISCOVERED_ID_PREFIX)
}

/// 实例列表的**首屏三态**（M302）：区分「还没扫完」和「确实一个都没有」。
///
/// ## 为什么必须要这一位（复审 r1 P2-1）
///
/// 扫盘搬进后台线程之后，`App::dispatch(RefreshAll)` 返回时快照还没到（`pump` 只抽队列、
/// 不 `tick`），所以**首帧**的 `AppState::instances` 是空的。用「`instances.is_empty()`」去猜
/// 状态就必然把「还没扫完」画成「还没有实例」——一句假话，还配一个「导入已有版本」的实心 CTA
/// （用户可能真去点）。真源必须是 host 自己知道的那一位，不是列表长度。
///
/// ## 语义（三态都要能一句话说清）
///
/// | 值 | 什么时候是它 | 界面 |
/// |---|---|---|
/// | [`Self::Scanning`] | **初值**：这一进程还没有任何一份实例快照落地 | 占位「正在扫描实例…」，不写计数、不画空态 CTA |
/// | [`Self::Stalled`] | 扫描起跑后超过 `host::game::SCAN_STALL` 仍没有快照（线程 panic / 卡在慢盘） | 占位 + 「重新扫描」按钮 |
/// | [`Self::Ready`] | 收到过至少一份 `CoreEvent::InstancesChanged` | 列表（空的也行——那就是真数据） |
///
/// **`Ready` 不表示「列表非空」**，也**不表示「扫描成功」**（没有失败态：扫描不返回错误，
/// 只有「还没回来」）：空列表 + `Ready` = 确实没有实例，那时才画「还没有实例」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub enum InstancePhase {
    /// 首个快照还没到（进程刚起来，或刚点过「重新扫描」）
    #[default]
    Scanning,
    /// 首个快照迟迟没到（有界兜底，见 `host::game::SCAN_STALL`）
    Stalled,
    /// 收到过快照：列表内容就是真数据（可能是空）
    Ready,
}
