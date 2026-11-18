//! 内容库的宿主实现（M109 `CONTENTDL`）：**检索** + **装进指定实例**。
//!
//! ## 为什么这些代码在 presenter 而不是 core（临时落点，必须写清）
//!
//! `crates/core` 的 `addon` 模块今天只有两件事：加载器安装（`addon::loader`）与整合包**格式**
//! 解析（`addon::pack`，M110）。它**没有** CurseForge / Modrinth 的 API 客户端，也没有
//! 「远程整合包一键装进实例」的管线（M111/M112 才做）。而任务书的硬约束是「UI 层不许自己发
//! HTTP，新增能力一律经 presenter 的 host 转发」——所以本轮把客户端放在这里。
//!
//! **长期应下沉到 `core::addon`**（已报 finding）：那时本模块只剩「DTO ↔ 领域类型」的翻译，
//! 与 `host/versions.rs` 现在的角色一样。
//!
//! ## 三条纪律
//!
//! | 纪律 | 落点 |
//! |---|---|
//! | 网络只在后台线程 | [`spawn_search`] / [`spawn_install`] 走 `job::spawn`，UI 线程只收事件 |
//! | Modrinth 走 core 的下载引擎 | `addon/http.rs` 的 `download_bytes` 路径带**信号量限流 + 源健康度 + 指数退避**，不另写重试循环 |
//! | 密钥不进仓库 / 日志 | CF 的 `X-API-KEY` 只由 `sources::api_key` 交给 core 的 `ApiKey`，日志只打「配没配」，DTO 里只有一个 `bool` |
//!
//! ## 模块
//!
//! | 文件 | 职责 |
//! |---|---|
//! | [`sources`] | 两个站点的 JSON 方言 → [`crate::dto::ContentHit`] |
//! | `http` | 取文本：无 header 走引擎、带 header 走 client + 退避、百分号编码 |
//! | [`install`] | 一键装入：解析文件（含 `required` 前置）→ 下载到实例目录 → 进度回流 |
//! | [`downloads`] | 下载阶段（可测）：逐个下、失败进名单不中断、两处取消点 |
//! | [`plan`] | 依赖计划的宿主侧：扫目录、两档失败 → DTO 大类、人读文案 |
//!
//! ## 整合包与 CF 的边界（都写进报告）
//!
//! - 整合包**不做**「一键装入」：`ContentKind::Modpack.dir()` 是 `None`，UI 侧按钮禁用带原因，
//!   host 侧再挡一次（就算绕过 UI 发指令也只回一条 `Failed`）。真流程是 M111 的导入管线。
//! - CurseForge 的**检索**（`sources.rs`）、**导入补全**（core 的 `import::fetch::curse`）与
//!   **详情/版本列表**的分工写在各自文件头；Key 的判据全仓只有一处
//!   （`core::addon::curseforge::auth::resolve_key`：设置优先 + 环境变量兜底 + 空白算没配）。

mod downloads;
mod http;
mod install;
mod plan;
// M113: import/export —— 整合包导入/导出（拆成子目录：父文件只留既有内容 + mod）
mod pack;
mod sources;

pub(super) use pack::{cancel_pack, pack_epoch, take_terminal_epoch};

#[cfg(test)]
mod tests;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::dto::{
    ContentDetail, ContentDetailRequest, ContentFailure, ContentQuery, ContentResults,
};

use super::env::HostEnv;
use super::job::{spawn, HostTask, JobSender};

/// 日志里只出现这两种状态，**永远不打密钥本身**。
const KEY_UNSET: &str =
    "CurseForge Key 未配置（设置 online.curseforge.api_key 或 ULL_CF_API_KEY，CF 源不可用）";
const KEY_SET: &str = "CurseForge Key 已配置（CurseForge 源可用）";

/// CF 源此刻可用吗（页面据此禁用那条来源）。
///
/// **判据只有一处**：检索用的 [`sources::api_key`]（设置优先 + 环境变量兜底，且空白算没配）。
/// 本函数以前自己读环境变量，于是「只在设置里填了 Key」的用户看到的是「不可用」——
/// 同一份配置在一处算有、在另一处算没有（M317 复审点名的三条同源洞之一）。
#[must_use]
pub fn curseforge_ready(env: &HostEnv) -> bool {
    sources::api_key(env).is_some()
}

/// 「配没配」那一行的措辞：**判据与 [`curseforge_ready`] 是同一个调用**（不是再读一次环境变量），
/// 于是「页面说可用 / 日志说没配」这种自相矛盾在结构上不成立。
fn curseforge_state_line(env: &HostEnv) -> &'static str {
    if curseforge_ready(env) {
        KEY_SET
    } else {
        KEY_UNSET
    }
}

/// 打一行「配没配」的日志（**不含**密钥内容；host 装配时调一次）。
pub fn log_curseforge_state(env: &HostEnv) {
    eprintln!("[content] {}", curseforge_state_line(env));
}

/// 后台检索（网络在后台线程，UI 不卡）。
pub fn spawn_search(env: HostEnv, tx: JobSender, query: ContentQuery) {
    spawn("content", move || {
        let result: Result<ContentResults, ContentFailure> = sources::search(&env, &query);
        let _ = tx.send(HostTask::Content(Box::new(result)));
    });
}

/// 后台取项目详情 + 版本列表（M162；选中项目才发，一条独立通道）。
pub fn spawn_detail(env: HostEnv, tx: JobSender, request: ContentDetailRequest) {
    spawn("content-detail", move || {
        let project = request.project.clone();
        let result: Result<ContentDetail, ContentFailure> = sources::detail(&env, &request);
        let _ = tx.send(HostTask::ContentDetail {
            project,
            result: Box::new(result),
        });
    });
}

/// 后台「装进指定实例」。
pub fn spawn_install(
    env: HostEnv,
    tx: JobSender,
    request: crate::dto::ContentInstallRequest,
    cancel: Arc<AtomicBool>,
    epoch: u64,
) {
    spawn("content-install", move || {
        install::install(&env, &tx, &request, &cancel, epoch);
    });
}
