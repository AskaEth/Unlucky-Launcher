//! 导入管线的**准备段**：读包 → 组计划 → 抄进报告 → 写基线（一个字节都还没进目标目录）。
//!
//! 从 [`super::pipeline`] 拆出来的是"碰盘之前"的那一段，行为逐字未变：编排文件（`pipeline.rs`）
//! 要装下取消的七个检查点与回滚收尾，把这一段移到同级模块是为了让它仍在一屏之内。
//!
//! 这里的函数**都不查取消令牌**：整段落在"解析后一次检查"之前/之后的两侧，而 `open` 是
//! 不可分割的（zip 目录 + 四检 + 限额一次扫完）——见 [`super::cancel`] 的逐段表。

use super::baseline::PackBaseline;
use super::error::ImportError;
use super::plan::ImportPlan;
use super::report::ImportReport;
use super::stage::Staging;
use super::ImportRequest;
use crate::addon::pack::{PackArchive, PackError, PackRegistry};

/// 开包 + 探测 + 组计划（任何失败都是 `Failed`）。
///
/// 预检（[`super::scan`]）走的是**同一段代码**：两边对"这包是什么、要装什么"必须完全一致，
/// 否则"预检说没冲突、导入说有冲突"这种事就会发生。
pub(super) fn probe_and_plan(
    request: &ImportRequest<'_>,
) -> Result<(PackArchive, ImportPlan), ImportError> {
    // 路径穿越双检 + 符号链接 + 条数/体积限额都在 `open` 里一次做完（M110 的 `archive`）。
    let archive = PackArchive::open(request.pack, request.limits)?;
    let (format, manifest) = PackRegistry::builtin().read(&archive)?;
    let plan = ImportPlan::build(&archive, format.id(), manifest)?;
    Ok((archive, plan))
}

/// 把计划里的静态信息抄进报告。
pub(super) fn account_plan(report: &mut ImportReport, plan: &ImportPlan) {
    report.format = Some(plan.format.clone());
    report.pack_name = Some(plan.manifest.name.clone());
    report.pack_version = plan.manifest.version.clone();
    report.game_version = Some(plan.mc_version.clone());
    report.loader = plan.loader.clone();
    report.version_id = Some(plan.version_id.clone());
    report.max_memory_mb = plan.max_memory_mb;
    report.rejected_components.clone_from(&plan.rejected);
    report.warnings.extend(plan.notes.iter().cloned());
}

/// 基线作为**最后一个暂存文件**写进去：它跟同一个事务一起提交 / 回滚。
pub(super) fn write_baseline(
    staging: &Staging,
    plan: &ImportPlan,
    next: &PackBaseline,
) -> Result<(), ImportError> {
    let mut baseline = next.clone();
    baseline.pack_name.clone_from(&plan.manifest.name);
    baseline.format.clone_from(&plan.format);
    let bytes = baseline.to_bytes().map_err(PackError::io)?;
    staging.write(&baseline_relative(), &bytes)?;
    Ok(())
}

/// 基线相对实例根的路径（`<根>/.ull/pack-baseline.json`；与读侧同源，见
/// [`PackBaseline::relative_path`]）。
#[must_use]
pub fn baseline_relative() -> String {
    PackBaseline::relative_path()
        .to_string_lossy()
        .replace('\\', "/")
}
