//! 计划的 JSON 视图与源选择样本（`--dry-run` 的输出形状）。
//!
//! 单独一份是为了让 [`InstallPlan`](super::plan::InstallPlan) 的"计算"与"呈现"分开：
//! 形状变了只动这里，计划算法不受影响；也让 `plan.rs` 不至于越过 400 行硬约束。

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use super::plan::{FileRole, InstallPlan, PlanFile};
use crate::download::DownloadEngine;

impl InstallPlan {
    /// 计划摘要（`--dry-run` 与日志共用；不含逐文件明细）。
    #[must_use]
    pub fn summary_json(&self) -> Value {
        let (total, unknown) = self.total_bytes();
        let by_role: BTreeMap<String, Value> = self
            .by_role()
            .into_iter()
            .map(|(role, (count, bytes))| {
                (
                    role.as_str().to_string(),
                    json!({"files": count, "bytes": bytes, "label": role.label()}),
                )
            })
            .collect();
        json!({
            "version_id": self.version_id,
            "jar_id": self.jar_id,
            "game_root": self.game_root.display().to_string(),
            "file_count": self.file_count(),
            "total_bytes": total,
            "unknown_size_files": unknown,
            "by_role": by_role,
            "warnings": self.warnings,
        })
    }

    /// 逐文件明细（`--dry-run` 的"列清单"用它；真实安装不输出，避免上千行噪声）。
    #[must_use]
    pub fn files_json(&self) -> Vec<Value> {
        self.files.iter().map(PlanFile::to_json).collect()
    }

    /// 每个角色取第一个文件，[`DownloadEngine::plan`] 给出候选源顺序（"源选择"）。
    ///
    /// 只取每类的**首个**文件做样本：3000 个资源对象的候选源是一样的，
    /// 逐文件列源选择既没用也没法读。
    #[must_use]
    pub fn source_samples(&self, engine: &DownloadEngine, now_ms: u64) -> Vec<Value> {
        let mut seen: BTreeSet<FileRole> = BTreeSet::new();
        let mut samples = Vec::new();
        for file in &self.files {
            if !seen.insert(file.role) {
                continue;
            }
            let candidates: Vec<Value> = engine
                .plan(&file.request(), now_ms)
                .into_iter()
                .map(|candidate| json!({"source": candidate.source, "url": candidate.url}))
                .collect();
            samples.push(json!({
                "role": file.role.as_str(),
                "label": file.role.label(),
                "sample": file.name,
                "candidates": candidates,
            }));
        }
        samples
    }
}
