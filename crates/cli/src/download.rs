//! CLI 侧的下载引擎：把 CLI 自己的 `launcher.json` 接到 core 的下载调优上（M203）。
//!
//! `install` / `java` / `pack` 三条命令此前各自 `DownloadEngine::new(EngineConfig::default(), …)`：
//! 用户在设置里配的 `download.concurrency` / `download.retry_limit` / `download.gh_proxy` 一概不生效。
//! M193 把出厂并发从 64 改成 8 之后，它们还**静默**跟着降了——「设置项说假话」的同期病灶
//! （`docs/download-settings.md` §3.1 记着这三条 ❌）。第四条路径 `update`（自更新）复用 `install`
//! 的装配，也随之吃设置；它只下一个 artifact，所以并发在那条路径上不可观测。
//!
//! 现在四条一律经 [`engine`] 取引擎（进程内共享，见 `core::download::shared`）：
//!
//! ```text
//! <数据根>/config/launcher.json（与 `ull config set` 同一份） →  setting store  →  DownloadTuning
//!                                                            （默认值/夹取/回落在 core）
//!   → shared::engine_with_tuning(<cache 根>, tuning) → 引擎的 Semaphore / attempts_for()
//! ```
//!
//! **回落是静默的**：`SettingStore::load` 永不失败（坏值只进 `problems()`）；类型不对 → `None`，
//! 越界 → `DownloadTuning::from_settings` 的区间过滤。两种情况都落成出厂值，**不报错、不中断安装**。

use std::sync::Arc;

use unlucky_core::download::config::{
    DownloadTuning, KEY_CONCURRENCY, KEY_GH_PROXY, KEY_RETRY_LIMIT,
};
use unlucky_core::download::{shared, DownloadEngine};
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;

use crate::context::Ctx;

/// 从 CLI 自己的 setting store 读下载调优（`launcher.json` 的三个 `download.*` 键）。
///
/// 读的是 `ctx.launcher_config()`——与 `ull config set download.concurrency` 的落点同一份文件
/// （设置键的落点分流见 [`Ctx::setting_path_for`]）。gh-proxy 空串 = 跟随内置默认，**不是**关。
#[must_use]
pub(crate) fn tuning(ctx: &Ctx) -> DownloadTuning {
    let store = SettingStore::load(ctx.launcher_config(), launcher_schema());
    let doc = store.doc();
    let gh_proxy = doc
        .effective_str(KEY_GH_PROXY)
        .unwrap_or_default()
        .to_string();
    DownloadTuning::from_settings(
        doc.effective_i64(KEY_CONCURRENCY),
        doc.effective_i64(KEY_RETRY_LIMIT),
        Some(gh_proxy.as_str()),
    )
}

/// 建该数据根的下载引擎：参数来自 [`tuning`]，引擎本体由 core 按缓存根共享。
#[must_use]
pub(crate) fn engine(ctx: &Ctx) -> Arc<DownloadEngine> {
    shared::engine_with_tuning(ctx.layout().cache(), tuning(ctx))
}

#[cfg(test)]
mod tests;
