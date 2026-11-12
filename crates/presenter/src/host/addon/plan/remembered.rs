//! 「上一次装入留下的计划」表（M191）：`retry_plan()` 的输入。
//!
//! 从 `plan.rs` 拆出（父文件加了 `label_of` 之后顶到 429 行，越过 400 行硬线）。
//! 拆法与本仓既有 `-p1/-p2` 一致：**纯搬移**，一行不改语义；调用方路径不变
//! （`plan.rs` 仍然 `pub(super) use` 这三个函数）。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use unlucky_core::addon::online::DependencyPlan;

use super::super::super::env::HostEnv;

/// 「上一次装入留下的计划」表的一行（M191）：`只重试这些` 的输入。
///
/// ## 为什么要一张静态表（说清楚这笔债）
///
/// 重试要的输入是**上一轮那份 `DependencyPlan`**（`retry_request()` 是它的方法）。它现在活在
/// `CoreHost`（`host/dispatch.rs`）之外，因为那个文件不在本 mission 的 scope；与
/// `addon/pack.rs` 的 `PACK_STATE` 同一手法：每 host 一格（键 = 数据根），于是测试里两个 host
/// 互不干扰。`host/mod.rs` 一旦进 scope，把这张表搬成一个字段即可（这表就是那个字段）。
///
/// 键里带 `instance`：同一台机器上往两个实例装过东西，重试只该重试这个实例那一次的计划。
/// 成功之后由 [`forget`] 清掉——「全成功」时不该留下任何可重试的东西。
///
/// ## 为什么连**原始根项目 id** 一起记（r2 复审 P2-2）
///
/// 「这一条是不是根项目自己失败」不能靠 `plan.request.roots.first()` 判：重试那一轮的 plan 来自
/// `DependencyPlan::retry_request()`，它的 `roots` 就是**上一轮的失败名单**——第一个是前置，
/// 拿它跟根的显示名一比，就会把 `Sodium` 贴到一个前置头上。所以判据只能是「这个 id 是不是
/// 当初那次装入的根 id」，那个 id 得跟计划一起记下来。
struct Entry {
    host: PathBuf,
    instance: String,
    /// 落盘目录（重试要按它重新扫 `present`，且**不许**换目录）
    dir: PathBuf,
    /// 当初那次装入的根项目 id（`ContentInstallRequest.project`，判「是不是根」的唯一依据）
    root: String,
    /// 当初那次装入的显示名（只配给 `root` 用）
    title: String,
    plan: DependencyPlan,
}

static REMEMBERED: Mutex<Vec<Entry>> = Mutex::new(Vec::new());

/// 取回的那一份记录（拆成结构体而不是四元组：调用点读得懂哪一个是哪个）。
pub(in crate::host::addon) struct Remembered {
    /// 落盘目录
    pub dir: PathBuf,
    /// 当初那次装入的根项目 id
    pub root: String,
    /// 当初那次装入的显示名
    pub title: String,
    /// 上一轮的依赖计划（`retry_request()` 的来源）
    pub plan: DependencyPlan,
}

/// 记下这一轮的计划（**覆盖**同一个 host + 实例的上一份）。
pub(in crate::host::addon) fn remember(
    env: &HostEnv,
    instance: &str,
    dir: &Path,
    root: &str,
    title: &str,
    plan: &DependencyPlan,
) {
    let host = env.layout.home().to_path_buf();
    let mut table = REMEMBERED
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    table.retain(|item| !(item.host == host && item.instance == instance));
    table.push(Entry {
        host,
        instance: instance.to_string(),
        dir: dir.to_path_buf(),
        root: root.to_string(),
        title: title.to_string(),
        plan: plan.clone(),
    });
}

/// 忘掉这个实例的计划（装成功 / 没有失败项时调：没有可重试的东西了）。
pub(in crate::host::addon) fn forget(env: &HostEnv, instance: &str) {
    let host = env.layout.home().to_path_buf();
    let mut table = REMEMBERED
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    table.retain(|item| !(item.host == host && item.instance == instance));
}

/// 这个实例上一次失败的计划（+ 落盘目录 + 原始根 id 与显示名）；`None` = 没有可重试的记录。
pub(in crate::host::addon) fn remembered(env: &HostEnv, instance: &str) -> Option<Remembered> {
    let host = env.layout.home().to_path_buf();
    let table = REMEMBERED
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    table
        .iter()
        .find(|item| item.host == host && item.instance == instance)
        .map(|item| Remembered {
            dir: item.dir.clone(),
            root: item.root.clone(),
            title: item.title.clone(),
            plan: item.plan.clone(),
        })
}
