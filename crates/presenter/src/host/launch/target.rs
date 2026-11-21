//! 启动受理用的实例解析（M304）：**从已发布快照里查**，不在调用方线程扫盘。
//!
//! ## 改的是什么
//!
//! M302 把 `refresh_instances` 的扫盘搬进了后台线程，但「点启动」这条路上还留着两次全量扫盘
//! （`launch_target` 与 `running_pid` 各自 `game::instances(&env)`）：官方根 200 个版本 236 ms、
//! 叠加 10 份外部档案 1.35 s，全部落在 UI 线程上。用户侧的表现就是「点了 Play，界面先僵一下」。
//!
//! 现在读**已发布快照**（`CoreHost::send` 把每份 `InstancesChanged` 留一份）：
//!
//! - 成本从「#根 次 `versions/` 全扫」降到一次内存查表 + 一次**单实例**复核
//!   （`game::Scan::revalidate`）；
//! - 语义也更准：用户在列表里点的**就是**快照里的那一条。同步扫盘拿的是「此刻的盘」，
//!   它可能已经和屏幕上那条不是同一个实例了（两次刷新之间盘变了）。
//!
//! ## 为什么不是「异步解析」
//!
//! 异步会把「点下去没反应」换个形状：受理（按钮进 Busy）得等解析回来。本 mission 的硬要求是
//! **同一拍**给出可见反馈，所以解析必须同步且足够便宜——便宜的手段是查快照（+ 单实例复核），
//! 不是再起一条线程等它回来。
//!
//! ## 快照过期时会发生什么（这条路的真风险）
//!
//! | 情形 | 判据落点 | 结果 |
//! |---|---|---|
//! | 实例在别处被删 / 绑的版本被改 | `Scan::revalidate` | 拒绝 + 具名原因 + 立刻重扫 |
//! | 快照里还没有内容（冷启动第一拍） | [`resolve`] 的空快照分支 | 拒绝 +「还在扫描中」 |
//! | 名字撞上两个实例 | [`find`] 的唯一命中判据 | 拒绝 +「不能替你猜」 |
//! | `versions/<id>` 被删 | `Scan::revalidate`（`adopt::inspect`） | 拒绝 + 具名原因 |
//!
//! 「拒绝」是**一字节都不碰**：不进 `pending`、不发 Busy、不起线程、不碰盘上的任何东西
//! （除了那一次只读复核）。

use crate::dto::InstanceSummary;

use super::super::game;
use super::super::HostEnv;
use super::LaunchTarget;

/// 拒绝受理：给用户看的一句具名原因 + 要不要顺手重扫一遍实例目录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejection {
    /// 人读原因（presenter 没有 i18n 依赖，文案在与 `toast` 同一层拼好）
    pub reason: String,
    /// 快照明确落后于盘面（实例没了 / 版本改了 / 版本目录没了）⇒ 值得立刻重扫一遍；
    /// 「还在扫描中」与「名字撞了」都不是过期，重扫也解决不了，所以是 `false`。
    pub stale: bool,
}

/// 在快照里把用户点的那个实例解析成启动目标。
///
/// `key` 是界面传来的实例 id（磁贴带的就是它）；名字兜底只为兼容按名字调用的老路径。
///
/// # Errors
///
/// 见模块文档那张表：空快照 / 快照里没有 / 名字撞多个 / 盘面已过期（[`Rejection::stale`]）。
pub fn resolve(
    env: &HostEnv,
    snapshot: &[InstanceSummary],
    key: &str,
    account: Option<String>,
) -> Result<LaunchTarget, Rejection> {
    let Some(instance) = find(snapshot, key)? else {
        // 空快照 = 首次扫描还没落地（M302 的占位那一态）。用户此刻其实是**看不到**可点的磁贴的，
        // 但这条路径在事件回流前也可能被点到（快捷键 / 双击），所以拒绝也要有话说，且**不装死**。
        return Err(if snapshot.is_empty() {
            Rejection {
                reason: "实例列表还在扫描中，稍等一下再点（正在扫描实例…）".to_string(),
                stale: false,
            }
        } else {
            Rejection {
                reason: format!("实例列表里没有 `{key}`（可能刚在别处被改过）"),
                stale: true,
            }
        });
    };
    game::Scan::revalidate(env, instance).map_err(|reason| Rejection {
        reason,
        stale: true,
    })?;
    Ok(LaunchTarget {
        id: instance.id.clone(),
        name: instance.name.clone(),
        version_id: instance.version_id.clone(),
        account,
    })
}

/// id 精确优先；名字兜底**只在唯一命中时**用。
///
/// 为什么名字撞了要拒绝而不是取第一个：两个同名实例（多根之后很常见：两个根里都叫 `A`）取第一个
/// 就是在**猜**，而猜错的代价是「用户看着 A，起来的却是 B」——启动是这条路上唯一不可撤销的动作。
fn find<'a>(
    snapshot: &'a [InstanceSummary],
    key: &str,
) -> Result<Option<&'a InstanceSummary>, Rejection> {
    if let Some(exact) = snapshot.iter().find(|instance| instance.id == key) {
        return Ok(Some(exact));
    }
    let mut by_name = snapshot.iter().filter(|instance| instance.name == key);
    let first = by_name.next();
    if by_name.next().is_some() {
        let count = snapshot
            .iter()
            .filter(|instance| instance.name == key)
            .count();
        return Err(Rejection {
            reason: format!("有 {count} 个实例都叫 `{key}`，不能替你猜——请从列表里点那一条"),
            stale: false,
        });
    }
    Ok(first)
}
