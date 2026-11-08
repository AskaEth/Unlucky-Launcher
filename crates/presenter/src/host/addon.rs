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
//! | 密钥不进仓库 / 日志 | CF 的 `X-API-KEY` 只从 `ULL_CF_API_KEY` 读，日志只打 `set`/`unset`，DTO 里只有一个 `bool` |
//!
//! ## 模块
//!
//! | 文件 | 职责 |
//! |---|---|
//! | [`sources`] | 两个站点的 JSON 方言 → [`crate::dto::ContentHit`] |
//! | `http` | 取文本：无 header 走引擎、带 header 走 client + 退避、百分号编码 |
//! | `install` | 一键装入：解析文件 → 下载到实例目录 → 进度回流 |
//!
//! ## 整合包与 CF 的边界（都写进报告）
//!
//! - 整合包**不做**「一键装入」：`ContentKind::Modpack.dir()` 是 `None`，UI 侧按钮禁用带原因，
//!   host 侧再挡一次（就算绕过 UI 发指令也只回一条 `Failed`）。真流程是 M111 的导入管线。
//! - CurseForge 本轮只做**检索**（需要用户配 `ULL_CF_API_KEY`），文件解析未接通。

mod http;
mod install;
// M113: import/export —— 整合包导入/导出（拆成子目录：父文件只留既有内容 + mod）
mod pack;
mod sources;

pub(super) use pack::{cancel_pack, pack_epoch};

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
const KEY_UNSET: &str = "ULL_CF_API_KEY unset（CurseForge 源不可用）";
const KEY_SET: &str = "ULL_CF_API_KEY set（CurseForge 源可用）";

/// CurseForge 的密钥（环境变量；空串 / 空白按「没配」处理）。
///
/// 为什么不放设置项：core 的 `setting` schema 不在本 mission 的 scope（已登记 finding）。
/// 环境变量这条路至少满足「不进仓库、不进日志」两条硬要求。
fn curseforge_key() -> Option<String> {
    std::env::var("ULL_CF_API_KEY")
        .ok()
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
}

/// CF 源此刻可用吗（页面据此禁用那条来源）。
#[must_use]
pub fn curseforge_ready() -> bool {
    curseforge_key().is_some()
}

/// 打一行「配没配」的日志（**不含**密钥内容；host 装配时调一次）。
pub fn log_curseforge_state() {
    eprintln!(
        "[content] {}",
        if curseforge_ready() {
            KEY_SET
        } else {
            KEY_UNSET
        }
    );
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
