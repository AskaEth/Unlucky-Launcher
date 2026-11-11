//! 依赖计划的**宿主侧那一半**：扫目录、把两档失败映成 DTO 大类、拼给用户看的那句话。
//!
//! # 为什么判定在 core、说话在 presenter
//!
//! 「哪个前置不兼容、哪个前置取不到」由 `unlucky_core::addon::online::resolve` 判定并可离线
//! 测试（录制 JSON 夹具）。本文件只做三件宿主才知道的事：
//!
//! | 事 | 为什么只能在这里 |
//! |---|---|
//! | 目标目录里有什么 | core 不读盘（`PresentFile` 由调用方扫） |
//! | 失败大类怎么映 | DTO 在 presenter（`ContentFailureKind`） |
//! | 文案怎么说 | 界面文案不归 core |
//!
//! # 两档失败**分开说**
//!
//! 「不兼容所选版本/加载器」与「取不到（网络 / 项目不存在）」是两句话、两种用户动作，
//! 所以这里既不合并计数也不合并文案；DTO 大类按「用户接下来该做什么」选：离线优先说
//! 检查网络，其次查无此项，只有**全是**不兼容时才说「这条路走不通」（换版本/加载器才有救）。
//!
//! # 「可只重试这些」的名单也在这里（M191）
//!
//! [`retry_items`] 把计划里的失败项摊成 DTO 的 [`InstallFailureView`]（项目键 + 可读标签 +
//! 档位 + 原话），界面据此分组画两段；[`retry_plan`] 是**真正重试**那一步——它调 core 的
//! `DependencyPlan::retry_request()`，于是重试只请求失败的那几个项目，**不是**重新发起根项目安装。
//!
//! 可读标签由 [`label_of`] 定（根项目用请求标题、下载失败用文件名、解析失败留空），
//! **不许**拿 `failure.message` 顶上——那是 core 的中文散文，界面照画会踩 i18n R2。
//!
//! # 不兼容有自己的一档（M191 起）
//!
//! M186 把它映成 `Unsupported`（当时 DTO 没有这一档，与「缺密钥」共用），于是界面没法单独说
//! 「换个版本/加载器」。M191 给 `ContentFailureKind` 加了 `Incompatible`，[`kind_of`] 直接映射，
//! **不再借 `Unsupported`**。

use std::path::Path;

use unlucky_core::addon::online::{
    DependencyFailure, DependencyPlan, FailureKind, OnlineError, PlannedNode, PresentFile,
    ResolveRequest,
};

use crate::dto::{ContentFailureKind, InstallFailureKind, InstallFailureView};

use super::sources::failure_of;

mod labels;
mod remembered;

pub(super) use labels::{fetch_title, names_for_incompatible, Root};
pub(super) use remembered::{forget, remember, remembered};

/// 扫目标目录（不递归）里已存在的文件名。
///
/// 只给名字，不算摘要：为了「跳过」把整个 `mods/` 哈希一遍不划算，同名就是同一份 jar 的
/// 最常见形态（我们落盘用站点给的文件名，不重命名）。
pub(super) fn present_files(dir: &Path) -> Vec<PresentFile> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| entry.path().is_file())
        .map(|entry| PresentFile::new(entry.file_name().to_string_lossy()))
        .collect()
}

/// 一次装入要用的解析请求（过滤条件与落盘目录一致）。
pub(super) fn request(
    project: &str,
    game_version: &str,
    loader: Option<&str>,
    present: Vec<PresentFile>,
) -> ResolveRequest {
    ResolveRequest {
        roots: vec![project.to_string()],
        game_version: Some(game_version.to_string()),
        loader: loader.map(str::to_string),
        present,
        ..ResolveRequest::default()
    }
}

/// 计划里有失败吗 ⇒ 一条带大类的失败（`None` = 干净）。
///
/// `installed` = **真落盘的**文件数（下载失败的不算）：失败文案里要能说清「主文件已装、
/// 缺的是谁的前置」，所以不能拿 `plan.nodes.len()` 冒充。
pub(super) fn failure_of_plan(
    plan: &DependencyPlan,
    installed: usize,
) -> Option<crate::dto::ContentFailure> {
    if plan.is_clean() {
        return None;
    }
    Some(crate::dto::ContentFailure::new(
        overall_kind(plan),
        report(plan, installed),
    ))
}

/// 整体大类：按「用户接下来该做什么」排序，离线 > 查无此项 > 站点错 > 其它 > 不兼容。
fn overall_kind(plan: &DependencyPlan) -> ContentFailureKind {
    let kinds: Vec<ContentFailureKind> = plan.failures.iter().map(kind_of).collect();
    for wanted in [
        ContentFailureKind::Offline,
        ContentFailureKind::NotFound,
        ContentFailureKind::Server,
        ContentFailureKind::Other,
        ContentFailureKind::Unsupported,
    ] {
        if kinds.contains(&wanted) {
            return wanted;
        }
    }
    // 只剩「不兼容」这一档：换版本/加载器才有救（M191 起它有自己的大类，不再借 `Unsupported`）
    ContentFailureKind::Incompatible
}

/// 单条失败的 DTO 大类（**复用** `sources::failure_of` 的映射表，不另写一份判据）。
fn kind_of(failure: &DependencyFailure) -> ContentFailureKind {
    match failure.cause {
        Some(cause) => failure_of(OnlineError::new(cause, failure.message.clone())).kind,
        // 不兼容那一档没有网络成因：它要用户换版本/加载器，不是「再试一次」
        None => ContentFailureKind::Incompatible,
    }
}

/// 可只重试的失败项 → DTO 名单（M191；界面按档分组画的那几条）。
///
/// 档位**一一映射** core 的 [`FailureKind`]：两档在 DTO 上分开，界面才有两段文案。
/// 顺序沿用计划里的顺序（`record_failure` 已按项目键去重，同一项目只出现一次）。
#[must_use]
pub(super) fn retry_items(
    plan: &DependencyPlan,
    root: labels::Root<'_>,
    names: &labels::Names,
) -> Vec<InstallFailureView> {
    plan.failures
        .iter()
        .map(|failure| {
            InstallFailureView::new(
                failure.requested_as.clone(),
                labels::label_of(plan, failure, root, names),
                failure_kind_of(failure.kind),
                failure.message.clone(),
            )
        })
        .collect()
}

/// core 的档位 → DTO 的档位（**穷尽 match**：谁把两档合并，这条会报错或让用例红）。
const fn failure_kind_of(kind: FailureKind) -> InstallFailureKind {
    match kind {
        FailureKind::Incompatible => InstallFailureKind::Incompatible,
        FailureKind::Unreachable => InstallFailureKind::Unreachable,
    }
}

/// 下一次解析的请求：**只含失败项**（M191）。
///
/// 这就是「只重试这些」的全部实现——请求来自 core 的
/// `DependencyPlan::retry_request()`（它只把失败项列进 `roots`），过滤条件沿用上次那份。
/// `present` 必须**重新扫**：这一轮新落盘的文件不在旧名单里，不重扫会被当成「还没有」再下一份
/// （core 的 `retry_request` 文档明确要求）。
///
/// 拿不到计划时（换进程 / 那次是别的实例）调用方**不许**退回「整单重装」：那会把已经装好的
/// 文件再下一遍，且请求数放大——正是本函数要避免的行为。
#[must_use]
pub(super) fn retry_request(plan: &DependencyPlan, present: Vec<PresentFile>) -> ResolveRequest {
    ResolveRequest {
        present,
        ..plan.retry_request()
    }
}

/// 用「只含失败项」的请求重新解析（**生产**用它；用例注入假解析器断言只请求了哪几个项目）。
///
/// 解析器用闭包注入而不是直接调 core：这样「重试请求里到底有哪几个项目、发了几次」在没有网络
/// 的用例里也能钉住（M191 的变异守门：把 `retry_request()` 换成根项目请求，用例必须红）。
pub(super) fn retry_plan(
    plan: &DependencyPlan,
    present: Vec<PresentFile>,
    resolve: impl FnOnce(&ResolveRequest) -> DependencyPlan,
) -> DependencyPlan {
    let request = retry_request(plan, present);
    resolve(&request)
}

/// 失败清单（两档各起一段，各自的条数分开数）。
fn report(plan: &DependencyPlan, installed: usize) -> String {
    let mut lines: Vec<String> = Vec::new();
    let incompatible: Vec<String> = plan
        .failures_of(FailureKind::Incompatible)
        .map(describe)
        .collect();
    let unreachable: Vec<String> = plan
        .failures_of(FailureKind::Unreachable)
        .map(describe)
        .collect();
    if !incompatible.is_empty() {
        lines.push(format!(
            "不兼容所选版本/加载器 {} 个：{}",
            incompatible.len(),
            incompatible.join("；")
        ));
    }
    if !unreachable.is_empty() {
        lines.push(format!(
            "取不到（网络 / 项目不存在）{} 个：{}",
            unreachable.len(),
            unreachable.join("；")
        ));
    }
    if installed > 0 {
        lines.push(format!("已装入 {installed} 个文件（已下好的不回滚）"));
    }
    if !plan.skipped.is_empty() {
        lines.push(format!("跳过 {} 个已存在的文件", plan.skipped.len()));
    }
    if let Some(note) = unresolved_note(plan) {
        lines.push(note);
    }
    if !plan.truncated.is_empty() {
        lines.push(format!(
            "依赖层数超过上限，未展开：{}",
            plan.truncated.join("、")
        ));
    }
    // 可重试名单：只列失败项（重试时也只重解这些，不重下已装好的）
    let retry = plan.retry_request().roots;
    if !retry.is_empty() {
        lines.push(format!(
            "可只重试这 {} 个：{}",
            retry.len(),
            retry.join("、")
        ));
    }
    lines.join("\n")
}

fn describe(failure: &DependencyFailure) -> String {
    if failure.required_by.is_empty() {
        format!("{}（{}）", failure.requested_as, failure.message)
    } else {
        format!(
            "{}（{}；被 {} 要求）",
            failure.requested_as,
            failure.message,
            failure.required_by.join("、")
        )
    }
}

/// 「有 required 但没有项目 id」那一档的提示（解析不了，但也不是没事发生）。
fn unresolved_note(plan: &DependencyPlan) -> Option<String> {
    if plan.unresolved_required.is_empty() {
        return None;
    }
    let items: Vec<String> = plan
        .unresolved_required
        .iter()
        .map(|item| {
            let what = item
                .version_id
                .clone()
                .or_else(|| item.file_name.clone())
                .unwrap_or_else(|| "未给版本".to_string());
            format!("{}（被 {} 要求）", what, item.parent)
        })
        .collect();
    Some(format!(
        "有 {} 条必备前置只给了版本/文件名、没有项目 id，无法自动解析：{}",
        items.len(),
        items.join("；")
    ))
}

/// 全部装成之后的落盘描述（含跳过与前置的条数、「为什么装」的父项目）。
///
/// `installed` 是**真落盘**的那些（下载失败的不在里面），所以条数与文件名都从它来，
/// 不从 `plan.nodes` 来——否则「装成了 N 个」会把下载失败的也数进去。
pub(super) fn summary(plan: &DependencyPlan, installed: &[PlannedNode]) -> String {
    if installed.is_empty() && plan.skipped.is_empty() && plan.unresolved_required.is_empty() {
        return "没有要装的文件".to_string();
    }
    let mut parts: Vec<String> = Vec::new();
    if !installed.is_empty() {
        let dependencies = installed.iter().filter(|node| !node.is_root()).count();
        // 「为什么装」：前置标出要求它的项目（根是用户自己点的，不标）
        let names: Vec<String> = installed
            .iter()
            .map(|node| {
                if node.is_root() {
                    node.file.filename.clone()
                } else {
                    format!(
                        "{}（前置，被 {} 要求）",
                        node.file.filename,
                        node.required_by.join("、")
                    )
                }
            })
            .collect();
        let head = if dependencies == 0 {
            format!("装了 {} 个文件", installed.len())
        } else {
            format!(
                "装了 {} 个文件（含 {} 个前置）",
                installed.len(),
                dependencies
            )
        };
        parts.push(format!("{head}：{}", names.join("、")));
    }
    if !plan.skipped.is_empty() {
        let names: Vec<&str> = plan
            .skipped
            .iter()
            .map(|node| node.file_name.as_str())
            .collect();
        parts.push(format!(
            "跳过 {} 个（目录里已存在，没有重复下载）：{}",
            plan.skipped.len(),
            names.join("、")
        ));
    }
    if let Some(note) = unresolved_note(plan) {
        parts.push(note);
    }
    if !plan.truncated.is_empty() {
        parts.push(format!(
            "依赖层数超过上限，未展开：{}",
            plan.truncated.join("、")
        ));
    }
    parts.join("\n")
}
