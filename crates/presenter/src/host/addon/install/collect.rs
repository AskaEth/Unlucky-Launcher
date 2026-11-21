//! 解析之后的公共尾巴：进度 → 逐个下载 → 收口（失败名单 / 计划留存）。
//!
//! 从 `install.rs` 拆出（r3 复审的接线守卫加完 `Ports` 之后父文件到 415 行，越过 400 行硬线）。
//! 拆法同既有 `-p1/-p2`：**纯搬移**，调用方路径不变（`install.rs` 仍 `pub(super) use` 这三个名字）。
//!
//! 为什么不把 `Ports` 再往里藏：`collect` 是**要能在用例里真跑**的那一段（r3 复审 p2：
//! 「失败路径真的去取了名字」原来零覆盖）。下载器与取名字器从参数进来，假实现就能驱动整条
//! 收口路径，不碰网络。

use std::path::Path;
use std::sync::atomic::AtomicBool;

use unlucky_core::addon::online::{ArtifactFetcher, DependencyPlan};

use crate::dto::{ContentFailure, ContentInstallRequest, InstallProgressView, InstallStage};

use crate::host::env::HostEnv;
use crate::host::job::{HostTask, JobSender};

use super::{downloads, plan};

/// `collect` 要用的「外部世界」：宿主环境 + 下载器 + 取名字器。
///
/// 三样都从这里进，是为了让**收口那段能在用例里真跑**：假下载器 + 假取名字器就能驱动
/// 「下载 → 收口 → 失败名单」整条路，于是「失败路径真的去取了不兼容档的名字」这件事有用例
/// 钉住（r3 复审 p2：那一行原来删掉也全绿）。
pub(in crate::host::addon) struct Ports<'a> {
    /// 宿主环境（记「上一轮的计划」要用它的数据根）
    pub(in crate::host::addon) env: &'a HostEnv,
    /// 下载器（生产 = 引擎；用例 = 假实现，不碰网络）
    pub(in crate::host::addon) fetcher: &'a dyn ArtifactFetcher,
    /// 取项目显示名（生产 = [`plan::fetch_title`]；用例 = 假实现）
    pub(in crate::host::addon) title: &'a dyn Fn(&str) -> Option<String>,
}

/// 「装到哪 + 那次装入的根是谁」——总是一起出现，捆成一个参数（否则 `collect` 顶破 8 个）。
pub(in crate::host::addon) struct Placement<'a> {
    /// 落盘目录（`mods/` / `resourcepacks/` / `shaderpacks/`）
    pub(in crate::host::addon) dir: &'a Path,
    /// 当初那次装入的根项目（id + 显示名）；重试轮里由 `plan::remembered` 提供
    ///
    /// **必须**带 id：重试那一轮 `plan.request.roots` 是上一轮的失败名单，不能拿它判「是不是根」
    /// （r2 复审 P2-2 就是这条：第一前置被贴上了根的标题）。
    pub(in crate::host::addon) root: plan::Root<'a>,
}

/// 解析之后**两条入口共用**的尾巴：进度 → 逐个下载 → 收口。
///
/// 收口的两件事在同一个地方做，是因为它们必须同时发生：
///
/// | 结果 | 计划 | 可重试名单 |
/// |---|---|---|
/// | 有失败 | 记下（`plan::remember`，重试的输入） | 填（界面据此画「只重试这 N 个」） |
/// | 全成功 | 忘掉（`plan::forget`） | 清空（界面**不渲染**那条入口） |
pub(in crate::host::addon) fn collect(
    ports: &Ports<'_>,
    request: &ContentInstallRequest,
    mut dependency_plan: DependencyPlan,
    placement: Placement<'_>,
    cancel: &AtomicBool,
    progress: &mut InstallProgressView,
    tx: &JobSender,
    epoch: u64,
) -> Result<String, ContentFailure> {
    progress.stage = InstallStage::Downloading;
    progress.files_total = dependency_plan.nodes.len() as u64;
    progress.files_done = 0;
    progress.message = if dependency_plan.nodes.is_empty() {
        "没有要下载的文件".to_string()
    } else {
        format!(
            "{} 个文件（含 {} 个前置）",
            dependency_plan.nodes.len(),
            dependency_plan
                .nodes
                .iter()
                .filter(|node| !node.is_root())
                .count()
        )
    };
    let _ = tx.send(HostTask::Install {
        progress: Box::new(progress.clone()),
        epoch,
    });

    // 下载阶段在 `downloads.rs`：可测（假下载器能驱动），失败进名单而不中断。
    let installed = downloads::download_stage(
        &mut dependency_plan,
        placement.dir,
        ports.fetcher,
        cancel,
        progress,
        tx,
        epoch,
    )?;

    // 解析与下载两段的失败在这里一起收口：两档分成两句话（不合并成「下载失败」），
    // 并列出「可只重试这 N 个」。
    if let Some(failure) = plan::failure_of_plan(&dependency_plan, installed.len()) {
        // 「不兼容」那一档项目**存在**（站点只是没有匹配目标版本/加载器的条目），所以在这里
        // 现取一次名字——只在失败路径上发、一个项目一次 GET，取不到就退回 id（见 `names_for_incompatible`）。
        let names = plan::names_for_incompatible(&dependency_plan, ports.title);
        progress.retry = plan::retry_items(&dependency_plan, placement.root, &names);
        plan::remember(
            ports.env,
            &request.instance,
            placement.dir,
            placement.root.id,
            placement.root.title,
            &dependency_plan,
        );
        return Err(failure);
    }
    progress.retry.clear();
    plan::forget(ports.env, &request.instance);
    Ok(plan::summary(&dependency_plan, &installed))
}
