//! 落盘前的最后一段：**剔引擎残片 → 装加载器（M311 第六跳）→ 落盘前检查点**。
//!
//! 从 [`super::pipeline`] 拆出来与 `phase/settle.rs` 同一条理由：编排文件要装下取消的检查点
//! 与回滚收尾，再在这里写二十多行就过 400 行硬线。**行为逐字未变**（只是换了家），唯一的
//! 新增是第六跳本身。
//!
//! # 为什么装载器排在落盘之前（`pipeline.rs` 那边只留一句结论，这里是理由全本）
//!
//! 装载器落在**共享分区**（`versions/` `libraries/` `assets/`，D20：任何隔离档都不按实例复制），
//! 与实例内容目录无关。所以它可以先装，且必须先装：
//!
//! | 顺序 | 装不上时的事实 | 实例登记 |
//! |------|----------------|----------|
//! | 先装（本模块） | 暂存目录删掉，目标目录**一个字节都没写** | 不登记（管线直接走 `abandoned`） |
//! | 先落盘再装 | 只能整包回滚，用户等半天的 mods 白下 | 回滚后不登记 |
//!
//! 两条都不会留下"档案指向不存在的版本 JSON"，但只有前者说得出「目标目录未被写入」——
//! 于是「导入成功、点启动报 `VersionManifest`」这个假功能在结构上不可能发生。
//!
//! # 取消
//!
//! 这一段的每一次停下都在**碰目标目录之前**：`staging.abort()` 之后调用方按"取消/未写入"
//! 收尾即可（回滚口径由 [`super::pipeline`] 给，这里不碰报告以外的任何状态）。
//! 装载器装到一半被取消时 `versions/` 里可能留下已下好的库（`execute` 不写版本 JSON 就不算
//! "装完了"），那是共享分区的正常中间态，下一次重试会按 sha1 跳过。
//!
//! # 装载器装好了、后面才失败的后果（如实说）
//!
//! 装载器先装意味着：**它成功之后**若落盘或登记失败并回滚，`versions/<id>/` 里那一层会留下。
//! 那是**一份合法装好的加载器版本**（不是半成品：版本 JSON 只在全量就绪后才写），别的实例也能
//! 用它，重导时按 sha1 跳过；所以这里**不回滚它**——删掉一份装好的共享版本，代价比留着大得多。

use super::cancel::Stopped;
use super::loader;
use super::plan::ImportPlan;
use super::report::{keys, ImportIssue, ImportReport};
use super::stage::Staging;
use super::ImportRequest;

/// 跑这一段。`Err` 时调用方按 [`Stopped`] 分流（取消 / 失败）——**删暂存目录是调用方的事**
/// （`Staging::abort(self)` 拿走所有权，这里只借它）。
pub(super) async fn run(
    request: &ImportRequest<'_>,
    plan: &ImportPlan,
    staging: &Staging,
    report: &mut ImportReport,
) -> Result<(), Stopped> {
    // 引擎的续传残片（`.ullpart-*`）与 meta 不是整合包内容，提交前剔掉。
    match staging.purge_engine_artifacts() {
        Ok(0) => {}
        Ok(removed) => report.warnings.push(ImportIssue::note(
            keys::NOTE_ENGINE_ARTIFACTS_DROPPED,
            format!("提交前剔除了 {removed} 个下载引擎的中间产物（.ullpart-* / .ullmeta.json）"),
        )),
        Err(error) => return Err(Stopped::Failed(error)),
    }

    // 第六跳：清单声明了加载器就真的装（失败 ⇒ 报告说"未写入"、实例不登记）。
    loader::install_declared_loader(request, plan, report).await?;

    // 装载器装完之后的那次检查（此前写过的是共享分区，目标目录仍然一个字没动）。
    request.cancel.stopped()?;
    Ok(())
}
