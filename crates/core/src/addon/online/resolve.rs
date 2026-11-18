//! BFS 依赖解析（**只跟 `required`**）。形状在 [`super::dependency`]，这里只有算法。
//!
//! # 与参考实现（Python 版 Modrinth 下载器）的四处差别
//!
//! | 参考实现 | 这里 | 为什么 |
//! |---|---|---|
//! | 每个前置再 `GET /v2/project/{id}` 换 slug | **不查**，直接拿 `dependencies[].project_id` 当地址 | 依赖里给的就是项目 id，`/project/{id}/version` 认它；一次安装省掉每个前置一个请求 |
//! | 失败名单是字符串，靠括号里的中文区分「不兼容」与「网络异常」 | [`FailureKind`] 枚举 | 「哪一档」要能从类型上判，不能靠 `contains("网络")` |
//! | 一个 dict 兼做缓存与去重 | 键 = **规范项目 id**，别名单设一张表 | 同一项目可能一次以 slug、一次以 id 出现；只按请求串去重会把它装两遍 |
//! | 队列按 `dependencies` 原文顺序展开 | 每层按项目键**字典序**处理 | 顺序必须是数据决定的，否则测试靠 HashMap 运气 |
//!
//! # 失败是**数据**，不是 `Err`
//!
//! [`resolve`] 永远返回一个 [`DependencyPlan`]：某个前置取不到版本时，它进 `failures`，
//! 解析继续做别的分支。若中途 `return Err`，用户看到的是「装这个模组失败」，而真正该看到的
//! 是「主文件装上了，缺 fabric-api（不兼容 1.20.1/fabric）」＋一份可单独重试的名单。
//!
//! # 两张去重网
//!
//! 1. **图内**：同一项目只解一次版本（`nodes` 里每个项目最多一条），多条依赖路径折进
//!    [`PlannedNode::required_by`]；
//! 2. **盘上**：目标目录里已经有同名文件（`.disabled` 也算）就进 `skipped`、**不再下载**。
//!    这是「已装过的同一项目跳过并提示」里我们真能判的那一半；另一半（同项目**另一个版本**
//!    已装）需要实例侧的已装清单，本模块判不出来（见报告）。
//!
//! 盘上判定只比文件名（`present` 由调用方扫目录得到，core 不读盘）：内容级去重需要先算一份
//! sha1，为了「跳过」把整个 `mods/` 哈希一遍不划算；同名文件就是同一份 jar 的最常见形态
//! （我们落盘用站点给的文件名，不重命名）。

use std::collections::{BTreeMap, BTreeSet};

use super::dependency::{
    present_key, DependencyFailure, DependencyPlan, FailureKind, PlannedNode, PresentFile,
    ResolveRequest, SkipReason, SkippedNode, UnresolvedRequired,
};
use super::model::Version;
use super::modrinth::Modrinth;
use super::transport::{OnlineErrorKind, Transport};

/// 一层里还没处理的一项。
struct Wave {
    key: String,
    depth: u8,
    parents: Vec<String>,
}

/// BFS 解析。**不返回 `Err`**：失败进 [`DependencyPlan::failures`]（见模块文档）。
pub async fn resolve<T: Transport + ?Sized>(
    client: &Modrinth<'_, T>,
    request: &ResolveRequest,
) -> DependencyPlan {
    let mut plan = DependencyPlan::new(request);
    let present: BTreeSet<String> = request.present.iter().map(PresentFile::key).collect();
    // 请求键 → 规范 id（同一项目可能一次给 slug、一次给 id）
    let mut alias: BTreeMap<String, String> = BTreeMap::new();
    let mut resolved: BTreeSet<String> = BTreeSet::new();

    let mut layer: Vec<Wave> = Vec::new();
    for root in &request.roots {
        let key = root.trim();
        if !key.is_empty() {
            wave_in(&mut layer, key, 0, None);
        }
    }

    while !layer.is_empty() {
        // 同层按项目键字典序：顺序由数据决定，不靠插入顺序或 HashMap 的运气。
        layer.sort_by(|left, right| left.key.cmp(&right.key));
        let mut next: Vec<Wave> = Vec::new();
        for mut item in layer.drain(..) {
            normalize(&mut item.parents);
            // ① 已经解过（含「A 依赖 B、B 又依赖 A」的环）⇒ 只把父边折进已有节点
            if let Some(canonical) = alias.get(&item.key).cloned() {
                fold_parents(&mut plan, &canonical, &item.parents);
                continue;
            }
            let version = match client.versions(&request.version_query(&item.key)).await {
                Ok(versions) => match versions.into_iter().next() {
                    Some(version) => version,
                    None => {
                        let message = filter_message(request);
                        fail(
                            &mut plan,
                            &mut alias,
                            item,
                            FailureKind::Incompatible,
                            None,
                            message,
                        );
                        continue;
                    }
                },
                Err(error) => {
                    let message = format!(
                        "取版本失败（{}）：{}",
                        error.kind().as_str(),
                        error.message()
                    );
                    fail(
                        &mut plan,
                        &mut alias,
                        item,
                        FailureKind::Unreachable,
                        Some(error.kind()),
                        message,
                    );
                    continue;
                }
            };
            let Some(file) = version
                .primary_file()
                .filter(|file| !file.url.trim().is_empty())
                .cloned()
            else {
                let message = format!(
                    "{} 的版本 {} 里没有可下载的主文件",
                    item.key,
                    version_label(&version)
                );
                fail(
                    &mut plan,
                    &mut alias,
                    item,
                    FailureKind::Incompatible,
                    None,
                    message,
                );
                continue;
            };

            let canonical = canonical_of(&item.key, &version);
            alias.insert(item.key.clone(), canonical.clone());
            alias
                .entry(canonical.clone())
                .or_insert_with(|| canonical.clone());
            // ② 同一项目走了两条路（slug 与 id 各一次）⇒ 折边，不重复装
            if !resolved.insert(canonical.clone()) {
                fold_parents(&mut plan, &canonical, &item.parents);
                continue;
            }

            let file_name = file.filename.clone();
            if present.contains(&present_key(&file_name)) {
                plan.skipped.push(SkippedNode {
                    project: canonical.clone(),
                    requested_as: item.key.clone(),
                    file_name,
                    depth: item.depth,
                    required_by: item.parents.clone(),
                    reason: SkipReason::AlreadyPresent,
                });
            } else {
                plan.nodes.push(PlannedNode {
                    project: canonical.clone(),
                    requested_as: item.key.clone(),
                    version_id: version.id.clone(),
                    version_name: version.name.clone(),
                    version_number: version.version_number.clone(),
                    file,
                    depth: item.depth,
                    required_by: item.parents.clone(),
                });
            }

            // 子项：只跟 required（排序去重由 `required_project_ids` 保证）
            for dependency in version.required_project_ids() {
                if item.depth >= request.max_depth {
                    plan.truncated.push(dependency.to_string());
                } else {
                    wave_in(&mut next, dependency, item.depth + 1, Some(&canonical));
                }
            }
            // 只有版本 id、没有项目 id 的 required：没有地址可取，记下来不假装没看见
            for dependency in version.unresolved_required() {
                plan.unresolved_required.push(UnresolvedRequired {
                    parent: canonical.clone(),
                    version_id: dependency.version_id.clone(),
                    file_name: dependency.file_name.clone(),
                });
            }
        }
        plan.truncated.sort();
        plan.truncated.dedup();
        layer = next;
    }

    plan.unresolved_required.sort_by(|left, right| {
        (&left.parent, &left.version_id).cmp(&(&right.parent, &right.version_id))
    });
    plan.unresolved_required.dedup();
    plan
}

/// 记一条失败，并把这个请求键标成「已处理」。
///
/// 标键这一步是**失败侧的去重网**：成功路径靠 `alias` 短路，失败路径以前直接 `continue`，
/// 于是同一个项目被两条路径要求时会各取一次版本、各记一条失败，重试名单里出现两次。
/// 标记之后，后续同键请求在①处短路，走现成的 [`fold_parents`] 把父边折进那条失败。
///
/// # 这道去重按**请求键**，不是规范 id（已知边界，别当成全覆盖）
///
/// 规范 id（`project_id`）要拿到响应才知道；失败时没有响应，能用的只有请求键。所以同一个
/// 失败项目如果**先以 slug、后以 project_id** 被两个不同的父项目要求，仍会被取两次、名单里
/// 出现两个名字。真实载荷里 `dependencies[].project_id` 与用户点的 slug 混用的情况没有证据，
/// 且这一档本来就是「取不到」，重复一条的代价是名单多一行、不是多装一个东西——故本轮不消掉，
/// 只把边界写在这里（复审 r2 nit③）。
fn fail(
    plan: &mut DependencyPlan,
    alias: &mut BTreeMap<String, String>,
    item: Wave,
    kind: FailureKind,
    cause: Option<OnlineErrorKind>,
    message: String,
) {
    plan.record_failure(DependencyFailure {
        requested_as: item.key.clone(),
        required_by: item.parents,
        depth: item.depth,
        kind,
        cause,
        message,
    });
    alias.insert(item.key.clone(), item.key);
}

/// 把一项放进这一层的待处理表（同一个键已经排队 ⇒ 只追加父项目）。
fn wave_in(layer: &mut Vec<Wave>, key: &str, depth: u8, parent: Option<&str>) {
    if let Some(existing) = layer.iter_mut().find(|item| item.key == key) {
        if let Some(parent) = parent {
            existing.parents.push(parent.to_string());
        }
        return;
    }
    layer.push(Wave {
        key: key.to_string(),
        depth,
        parents: parent
            .map(|parent| vec![parent.to_string()])
            .unwrap_or_default(),
    });
}

/// 往已经建好的节点上折一条父边（节点可能在被跳过的那一档，也可能只出现在失败名单里）。
fn fold_parents(plan: &mut DependencyPlan, project: &str, parents: &[String]) {
    if parents.is_empty() {
        return;
    }
    let node = plan
        .nodes
        .iter_mut()
        .find(|node| node.project == project)
        .map(|node| &mut node.required_by);
    let node = node.or_else(|| {
        plan.skipped
            .iter_mut()
            .find(|node| node.project == project)
            .map(|node| &mut node.required_by)
    });
    if let Some(required_by) = node {
        required_by.extend(parents.iter().cloned());
        normalize(required_by);
        return;
    }
    if let Some(failure) = plan
        .failures
        .iter_mut()
        .find(|failure| failure.requested_as == project)
    {
        failure.required_by.extend(parents.iter().cloned());
        normalize(&mut failure.required_by);
    }
}

fn normalize(list: &mut Vec<String>) {
    list.sort();
    list.dedup();
}

/// 规范身份：站点回的 `project_id` 才是身份；没给就退回请求键。
fn canonical_of(key: &str, version: &Version) -> String {
    let project_id = version.project_id.trim();
    if project_id.is_empty() {
        key.to_string()
    } else {
        project_id.to_string()
    }
}

/// 「哪里没有匹配项」这句话（第一档的固定说法，带出用的是什么过滤条件）。
fn filter_message(request: &ResolveRequest) -> String {
    let game_version = request.game_version.as_deref().unwrap_or("不限");
    let loader = request.loader.as_deref().unwrap_or("不限");
    format!("没有匹配所选游戏版本/加载器（{game_version} / {loader}）的版本")
}

fn version_label(version: &Version) -> String {
    let number = version.version_number.trim();
    if number.is_empty() {
        version.id.clone()
    } else {
        number.to_string()
    }
}
