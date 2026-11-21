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
//!   → shared::engine_with_tuning(<cache 根>, tuning) → 引擎的 Semaphore / attempts_for() / 三段超时
//! ```
//!
//! M294 r1：`download.region` / `download.mirror_preference` 走**同一条链**（入口是
//! [`DownloadTuning::with_source_selection`]，与 presenter 的 `tuning_of` 同一个），落成引擎的
//! `RegionProfile`（候选源的先后）。在此之前 CLI 的四条命令认不到这两个键，同一份
//! `launcher.json` 在 GUI 与 CLI 上表现不同——「设置项说假话」只是从 GUI 挪到了 CLI。
//!
//! M292：`download.*` 从三个键扩到六个（多了吞吐下限 / 无进展窗口 / 单次请求上界），装配链没变，
//! 变的是 [`tuning`] 取值的宽度与 [`engine`] 里那一行**仅在改过设置时**才打的 stderr 说明——用户在
//! 慢链路上改完设置，得能在终端里看见引擎实际拿到了什么（P21 的验收判据；为什么不是写
//! `logs/launcher.log` 见 [`announce`] 的文档）。
//!
//! **回落是静默的**：`SettingStore::load` 永不失败（坏值只进 `problems()`）；类型不对 → `None`，
//! 越界 → `DownloadTuning::from_raw` 的区间过滤。两种情况都落成出厂值，**不报错、不中断安装**。

use std::sync::Arc;

use unlucky_core::download::config::{
    DownloadTuning, RawTuning, KEY_CHUNK_STALL, KEY_CONCURRENCY, KEY_GH_PROXY,
    KEY_MIRROR_PREFERENCE, KEY_PROXY_MODE, KEY_PROXY_URL, KEY_REGION, KEY_RESPONSE_TIMEOUT,
    KEY_RETRY_LIMIT, KEY_THROUGHPUT_FLOOR,
};
use unlucky_core::download::{shared, DownloadEngine};
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;

use crate::context::Ctx;

/// 从 CLI 自己的 setting store 读下载调优（`launcher.json` 的十个 `download.*` 键）。
///
/// 读的是 `ctx.launcher_config()`——与 `ull config set download.concurrency` 的落点同一份文件
/// （设置键的落点分流见 [`Ctx::setting_path_for`]）。gh-proxy 空串 = 跟随内置默认，**不是**关。
///
/// **本函数只做「取裸值」**：默认值、区间、回落、KiB→字节的换算全在 core 的
/// [`DownloadTuning::from_raw`]（M292 起宿主不再逐个判断，也不再自己决定缺省值——那是「UI 说 8、
/// 引擎跑 64」长回来的地方）。
///
/// M294 r1：`download.region` / `download.mirror_preference` 也在这里读，但**不经过
/// [`RawTuning`]**——那两个是枚举，翻译层是 core 的 [`DownloadTuning::with_source_selection`]
/// （未知值回落 `auto`）。CLI 与 presenter 走的是同一个入口，别在宿主里再写一份字符串匹配：
/// 两份口径就是「GUI 认、CLI 不认」的下一次复发。
///
/// M295：代理两键（`download.proxy_mode` / `proxy_url`）读法与 presenter 的 `tuning_of` 逐行相同
/// ——同一个 `RawTuning`、同一个 `ProxySetting::from_raw`，CLI 侧**零**字符串匹配。
#[must_use]
pub(crate) fn tuning(ctx: &Ctx) -> DownloadTuning {
    let store = SettingStore::load(ctx.launcher_config(), launcher_schema());
    let doc = store.doc();
    let gh_proxy = doc
        .effective_str(KEY_GH_PROXY)
        .unwrap_or_default()
        .to_string();
    DownloadTuning::from_raw(RawTuning {
        concurrency: doc.effective_i64(KEY_CONCURRENCY),
        retry_limit: doc.effective_i64(KEY_RETRY_LIMIT),
        gh_proxy: Some(gh_proxy.as_str()),
        throughput_floor_kib: doc.effective_i64(KEY_THROUGHPUT_FLOOR),
        stall_timeout_secs: doc.effective_i64(KEY_CHUNK_STALL),
        response_timeout_secs: doc.effective_i64(KEY_RESPONSE_TIMEOUT),
        proxy_mode: doc.effective_str(KEY_PROXY_MODE),
        proxy_url: doc.effective_str(KEY_PROXY_URL),
    })
    .with_source_selection(
        doc.effective_str(KEY_REGION),
        doc.effective_str(KEY_MIRROR_PREFERENCE),
    )
}

/// 生效值那一行说明，**只在改过设置时**给：`None` = 全是出厂值，没什么可说的。
///
/// 为什么是 stderr 而不是 `logs/launcher.log`：那个日志的目录是「先到者登记」的进程级全局
/// （`launcher_log::set_dir`），在下载装配点调它等于让一个读路径改全局状态——本仓被这种形状咬过
/// （`host/settings/download.rs` 的模块头记着：`apply_tuning` 的同步点是**显式**的，不是藏在读
/// 函数里）。CLI 的 stderr 是这一行的天然去处：`--json` 的机器输出在 stdout，不会被它污染。
#[must_use]
pub(crate) fn announce(tuning: &DownloadTuning) -> Option<String> {
    if *tuning == DownloadTuning::default() {
        None
    } else {
        Some(format!("下载调优：{}", tuning.summary()))
    }
}

/// 建该数据根的下载引擎：参数来自 [`tuning`]，引擎本体由 core 按缓存根共享。
///
/// 改过设置时往 stderr 打**一行**生效值（P21 的验收判据：改完设置得能看见引擎实际拿到了什么）。
/// 出厂值那一档不打：否则每个 `ull install` 都多一行噪声，而它什么也没告诉用户。
#[must_use]
pub(crate) fn engine(ctx: &Ctx) -> Arc<DownloadEngine> {
    let tuning = tuning(ctx);
    if let Some(line) = announce(&tuning) {
        eprintln!("{line}");
    }
    let engine = shared::engine_with_tuning(ctx.layout().cache(), tuning);
    // 代理那一档没生效时（`manual` 的地址解析不了 ⇒ 引擎回落直连）必须说出来：静默回落等于把
    // 「设置项说假话」换成「设置项不说实话」，用户只会觉得「填了代理还是连不上」。
    if let Some(problem) = engine.proxy_problem() {
        eprintln!("下载代理未生效，本次按直连：{problem}");
    }
    engine
}

#[cfg(test)]
mod tests;
