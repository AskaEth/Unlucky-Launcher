//! 「这条失败该显示什么名字」——**语言中立**的标签来源（M191 r1 P2-1 / r2 P2-1 续）。
//!
//! 从 `plan.rs` 拆出（加完失败路径上取名字那次 GET 之后父文件到 414 行，越过 400 行硬线）。
//! 拆的口径是「一件事一个文件」：这里只管**名字从哪来**，判定与说话仍在 `plan.rs`。
//!
//! 三个来源，按可信度排（细节在 [`label_of`]）：原始根项目的标题、失败文件的文件名、
//! 失败路径上现取的项目名；都没有就留空，界面只画项目 id。
//!
//! **不许**拿 `DependencyFailure::message` 顶：那是 core 的中文散文，画到界面上就是 i18n R2 的违规。

use unlucky_core::addon::online::{
    DependencyFailure, DependencyPlan, EngineTransport, FailureKind, Modrinth, OnlineError,
    ProjectDetail,
};
use unlucky_core::download::blocking::block_on;

use super::super::super::env::HostEnv;

/// 这一次装入的**原始根项目**（id + 显示名）。
///
/// 「是不是根」只能按 `id` 比（r2 复审 P2-2）：重试那一轮的 plan 来自
/// `DependencyPlan::retry_request()`，它的 `request.roots` **就是上一轮的失败名单**——
/// 第一个是前置，拿它当「根」会把根的标题贴到前置头上。所以判据是「这条失败的 id 是不是
/// 当初那次装入的根 id」，两个值都由 [`remembered`] / `ContentInstallRequest` 提供。
#[derive(Clone, Copy)]
pub(in crate::host::addon) struct Root<'a> {
    /// 当初那次装入的根项目 id（`ContentInstallRequest.project`）
    pub id: &'a str,
    /// 它的显示名（`ContentInstallRequest.title`；只配给 `id` 用）
    pub title: &'a str,
}

/// 失败路径上现取的项目名（id → title）。取不到的项目不在表里。
pub(in crate::host::addon) type Names = Vec<(String, String)>;

/// 为「不兼容」档现取项目名（**只在失败路径上**，一个项目一次 GET）。
///
/// 为什么只给这一档：`FailureKind::Incompatible` 是「站点**有这个项目**，但目标版本/加载器下
/// 没有条目」——项目确实存在，所以这次 GET 有东西可拿。「取不到」档里 404 的那部分项目根本
/// 不存在，给它发请求只会再吃一个 404，于是那一档仍显示 id（合理兜底）。
///
/// `lookup` 由调用方注入（生产是 `Modrinth::project`，用例是闭包）：于是「取名字失败也要能降级」
/// 这件事在没有网络的用例里也能钉住。取不到 / 拿到空串的项一律**不进**名单。
pub(in crate::host::addon) fn names_for_incompatible(
    plan: &DependencyPlan,
    lookup: impl Fn(&str) -> Option<String>,
) -> Names {
    plan.failures_of(FailureKind::Incompatible)
        .filter_map(|failure| {
            let title = lookup(&failure.requested_as)?;
            let title = title.trim();
            (!title.is_empty()).then(|| (failure.requested_as.clone(), title.to_string()))
        })
        .collect()
}

/// 一条失败的可读标签（**语言中立**：站点标题、文件名或现取的项目名；拿不到就是空串）。
///
/// 三个来源，**按可信度排**：
///
/// | 情况 | 标签 | 为什么 |
/// |---|---|---|
/// | 这条失败的 id == 原始根 id | 请求里的 `title` | 用户刚才点的就是它 |
/// | 计划里有这个节点（下载失败） | `node.file.filename` | 「哪个文件没下来」比项目名更具体 |
/// | 不兼容档 | [`names_for_incompatible`] 现取的名字 | 项目存在，一次 GET 拿得到 |
/// | 其余（解析 404 等） | 空 | 项目不存在，显示 id 是合理兜底 |
///
/// **不许**拿 `failure.message` 顶：那是 core 的中文散文，画到界面上就是 i18n R2 的违规。
#[must_use]
pub(super) fn label_of(
    plan: &DependencyPlan,
    failure: &DependencyFailure,
    root: Root<'_>,
    names: &Names,
) -> String {
    // ① 根项目自己失败：**按 id 比**，不看 `plan.request.roots`（重试轮里那是失败名单）
    if failure.requested_as == root.id {
        return root.title.to_string();
    }
    // ② 计划里有这个节点（下载失败）：文件名最具体
    if let Some(node) = plan
        .nodes
        .iter()
        .find(|node| node.requested_as == failure.requested_as)
    {
        return node.file.filename.clone();
    }
    // ③ 不兼容档：失败路径上现取的名字；取不到就空 ⇒ 界面只画 id
    if failure.kind == FailureKind::Incompatible {
        return names
            .iter()
            .find(|(id, _)| id == &failure.requested_as)
            .map(|(_, title)| title.clone())
            .unwrap_or_default();
    }
    String::new()
}

/// 取一个项目的显示名（**只在失败路径上**发）。
///
/// 一次 GET，折成「有名字 / 没名字」的那一步在 [`title_from`]（那里才是能坏的地方，因此那一步
/// 单独可测；本函数只有「怎么发这一枪」）。
pub(in crate::host::addon) fn fetch_title(env: &HostEnv, id: &str) -> Option<String> {
    let core_engine = crate::host::engine::engine(env);
    let transport = EngineTransport::new(&core_engine);
    let client = Modrinth::new(&transport);
    title_from(|id| block_on(async { client.project(id).await }), id)
}

/// 把「一次项目查询」折成「有名字 / 没名字」——**降级就在这一层**（r3 复审 p2 要的守卫）。
///
/// 三种「没名字」都算 `None`，一个都不许往外冒：
///
/// | 情况 | 结果 |
/// |---|---|
/// | 查询失败（没网 / 404 / 坏响应） | `None`（**不是 panic，也不是错误**） |
/// | 站点给了空标题 | `None` |
/// | 站点给了全空白标题 | `None` |
///
/// 为什么单独抽出来：这条链路上唯一会「坏」的就是这一句折叠。抽成参数化的纯函数之后，
/// 「查询失败仍退回 id」与「空白不算名字」都能在没有网络的用例里钉住（变异：`ok` 换 `expect`
/// ⇒ 红；去掉空串过滤 ⇒ 红）。
fn title_from(
    query: impl FnOnce(&str) -> Result<ProjectDetail, OnlineError>,
    id: &str,
) -> Option<String> {
    query(id)
        .ok()
        .map(|project| project.title)
        .filter(|title| !title.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detail(title: &str) -> ProjectDetail {
        ProjectDetail {
            id: "P7dR8mSH".to_string(),
            title: title.to_string(),
            ..ProjectDetail::default()
        }
    }

    /// **降级层**：查询失败 / 空标题 / 全空白 ⇒ 一律 `None`（界面退回只画 id），不 panic。
    ///
    /// 抓得住：把 `.ok()` 换成 `expect`（查询失败即 panic）⇒ 第一条红；
    /// 去掉 `.filter(..)`（空白也算名字）⇒ 后两条红。
    #[test]
    fn a_failed_or_blank_lookup_yields_no_name() {
        // ① 查询失败（没网 / 404 / 坏响应）
        assert!(title_from(|_| Err(OnlineError::offline("断网")), "P7dR8mSH").is_none());
        // ② 站点给了空标题
        assert!(title_from(|_| Ok(detail("")), "P7dR8mSH").is_none());
        // ③ 站点给了全空白标题
        assert!(title_from(|_| Ok(detail("   ")), "P7dR8mSH").is_none());
        // ④ 正常情况：原样给出
        assert_eq!(
            title_from(|_| Ok(detail("Entity Culling")), "P7dR8mSH"),
            Some("Entity Culling".to_string())
        );
        // 传进去的 id 就是查询用的那个（不要拿别的键去问）
        let mut asked = String::new();
        let _ = title_from(
            |id| {
                asked.push_str(id);
                Ok(detail("X"))
            },
            "P7dR8mSH",
        );
        assert_eq!(asked, "P7dR8mSH");
    }
}
