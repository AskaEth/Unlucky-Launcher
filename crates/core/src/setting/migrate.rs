//! 迁移链：`schemaVersion N → N+1` 顺序迁移框架 + 收据闸门（防重放）。
//!
//! 出处：`DevDocs/PlanDoc/04-data-and-config.md` §3（迁移链 + 迁移收据）、
//! `DevDocs/analyse/07-settings-packaging-update.md` §1.8（收据存源文件 `sourceSHA256`）。
//!
//! 与 HMCL 的差别：HMCL 完全靠 `$schema` 字段做兼容、迁移只处理遗留格式；ULL 在这之上补一条
//! 「major 逐级迁移链」——每一步只跨一个 major，缺环/重复/逆序在 `push` 时就报错，
//! 避免出现「跳版迁移脚本」这种组合爆炸。

use std::path::Path;

use serde_json::{Map, Value};

use crate::platform::error::PlatformResult;
use crate::platform::receipt::{self, MigrationReceipt, ReplayDecision};

use super::error::{SettingError, SettingResult};
use super::schema::{SchemaVersion, SCHEMA_VERSION_KEY};

/// 一步迁移。实现者只管把 `root` 从 `from` 改造成 `to`。
pub trait SettingMigration: Send + Sync {
    /// 起始版本。
    fn from(&self) -> SchemaVersion;
    /// 目标版本（必须等于 `from.next_major()`）。
    fn to(&self) -> SchemaVersion;
    /// 一句话说明（进日志与迁移报告）。
    fn describe(&self) -> &'static str;
    /// 就地改 JSON 对象；`Err(String)` 会被包成 [`SettingError::Migration`]。
    ///
    /// # Errors
    /// 结构不符合预期（该有的键没有/类型不对）时返回原因。
    fn apply(&self, root: &mut Map<String, Value>) -> Result<(), String>;
}

/// 迁移报告（进日志 / 遥测 / 收据）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    /// 起始版本。
    pub from: SchemaVersion,
    /// 结束版本。
    pub to: SchemaVersion,
    /// 依次执行的步骤说明。
    pub applied: Vec<&'static str>,
}

impl MigrationReport {
    /// 是否真的做了迁移。
    #[must_use]
    pub fn is_noop(&self) -> bool {
        self.applied.is_empty()
    }
}

/// 有序迁移链。
#[derive(Default)]
pub struct MigrationChain {
    steps: Vec<Box<dyn SettingMigration>>,
}

impl std::fmt::Debug for MigrationChain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.steps.iter().map(|step| step.describe()))
            .finish()
    }
}

impl MigrationChain {
    /// 空链。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加一步；必须与上一环首尾相接，且只跨一个 major。
    ///
    /// # Errors
    /// 跨多个 major、起始版本重复、顺序错乱时返回 [`SettingError::ChainBroken`]。
    pub fn push(&mut self, step: Box<dyn SettingMigration>) -> SettingResult<()> {
        let from = step.from();
        let to = step.to();
        if to != from.next_major() {
            return Err(SettingError::ChainBroken {
                reason: format!("一步迁移只能跨一个 major：{from} → {to}"),
            });
        }
        if self.steps.iter().any(|existing| existing.from() == from) {
            return Err(SettingError::ChainBroken {
                reason: format!("{from} 已有迁移步骤"),
            });
        }
        if let Some(last) = self.steps.last() {
            if last.to() != from {
                return Err(SettingError::ChainBroken {
                    reason: format!("顺序错误：上一环到 {}，本环从 {from}", last.to()),
                });
            }
        }
        self.steps.push(step);
        Ok(())
    }

    /// 步骤数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// 是否没有步骤。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// 步骤说明清单（日志用）。
    #[must_use]
    pub fn describe(&self) -> Vec<&'static str> {
        self.steps.iter().map(|step| step.describe()).collect()
    }

    /// 链能达到的最新版本。
    #[must_use]
    pub fn latest(&self) -> Option<SchemaVersion> {
        self.steps.last().map(|step| step.to())
    }

    /// 从 `from` 起要执行的步骤（含 `from` 那一步）。
    ///
    /// # Errors
    /// 链里没有以 `from` 开头的步骤（版本不认识）时返回 [`SettingError::ChainBroken`]。
    pub fn plan_from(&self, from: SchemaVersion) -> SettingResult<Vec<&dyn SettingMigration>> {
        let start = self
            .steps
            .iter()
            .position(|step| step.from() == from)
            .ok_or_else(|| SettingError::ChainBroken {
                reason: format!("链里没有以 {from} 起始的迁移步骤"),
            })?;
        Ok(self.steps[start..]
            .iter()
            .map(|step| step.as_ref())
            .collect())
    }

    /// 顺序执行迁移，并把 `schemaVersion` 镜像更新成最终 major。
    ///
    /// # Errors
    /// 缺环 → [`SettingError::ChainBroken`]；某步失败 → [`SettingError::Migration`]。
    pub fn migrate(
        &self,
        from: SchemaVersion,
        root: &mut Map<String, Value>,
    ) -> SettingResult<MigrationReport> {
        let plan = self.plan_from(from)?;
        let mut current = from;
        let mut applied = Vec::with_capacity(plan.len());
        for step in plan {
            step.apply(root).map_err(|reason| SettingError::Migration {
                from: current,
                to: step.to(),
                reason,
            })?;
            current = step.to();
            applied.push(step.describe());
        }
        root.insert(SCHEMA_VERSION_KEY.to_owned(), Value::from(current.major));
        Ok(MigrationReport {
            from,
            to: current,
            applied,
        })
    }
}

/// 收据闸门：迁移前问一句「这次该不该跑」。
#[derive(Debug, Clone, Copy)]
pub struct MigrationGate<'a> {
    receipt_path: &'a Path,
    now_ms: u64,
}

impl<'a> MigrationGate<'a> {
    /// 绑定收据路径与当前时间（毫秒 since UNIX_EPOCH，注入便于测试）。
    #[must_use]
    pub const fn new(receipt_path: &'a Path, now_ms: u64) -> Self {
        Self {
            receipt_path,
            now_ms,
        }
    }

    /// 当前时间。
    #[must_use]
    pub const fn now_ms(&self) -> u64 {
        self.now_ms
    }

    /// 读收据并判断是否重放。
    ///
    /// # Errors
    /// 收据损坏或读失败时返回 [`crate::platform::error::PlatformError`]。
    pub fn evaluate(&self, source_fingerprint: &str) -> PlatformResult<ReplayDecision> {
        let existing = receipt::read(self.receipt_path)?;
        Ok(receipt::decide(existing.as_ref(), source_fingerprint))
    }

    /// 是否该执行迁移（收据在且源指纹相同 → `false`）。
    ///
    /// # Errors
    /// 同 [`Self::evaluate`]。
    pub fn should_run(&self, source_fingerprint: &str) -> PlatformResult<bool> {
        Ok(self.evaluate(source_fingerprint)?.should_replay())
    }

    /// 记录一次迁移收据。
    ///
    /// # Errors
    /// 写盘失败时返回 [`crate::platform::error::PlatformError`]。
    pub fn record(&self, receipt: &MigrationReceipt) -> PlatformResult<()> {
        receipt::write(self.receipt_path, receipt)
    }
}
