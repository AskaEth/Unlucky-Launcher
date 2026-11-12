//! M159 的遗留更新偏好 → 主配置（`update.auto_check`）的迁移（M161）。
//!
//! 从 `commands/config.rs` 拆出：那边贴着 400 行硬线，而这块是**独立的一件事**
//! （遗留文件的桥接），与 get/set 的排版逻辑无关。

use serde_json::{json, Value};
use unlucky_core::setting::update_prefs::{self, Migration};

use crate::context::Ctx;

/// M159 的遗留更新偏好 → 主配置的迁移结果（只在**有话说**时返回）。
pub(super) struct PrefsNote {
    /// 人读一行（`config get/set` 的人类可读输出里追加）
    pub(super) human: String,
    /// 同一件事的机器可读形状（`--json` 信封里追加）
    pub(super) data: Value,
}

/// 把 `<数据根>/cache/update/prefs.json` 迁进 `launcher.json`（M161）。
///
/// 为什么 `config` 这个**读命令**也会写盘：那份遗留文件对 CLI 本来就不可见——用户在手头没 GUI
/// 的机器上 `config get update.auto_check`，旧实现永远回默认 `true`，而他的开关可能是关的。
/// 迁移是**幂等**的（源指纹闸门 + 旧文件改名 `.migrated`），有收据，且在写不进去时什么都不动
/// （见 [`unlucky_core::setting::update_prefs`]）。没有旧文件时这里只是一次 `stat`。
pub(super) fn migrate_update_prefs(ctx: &Ctx) -> Option<PrefsNote> {
    let layout = ctx.layout();
    let paths = update_prefs::Targets {
        launcher: &ctx.launcher_config(),
        legacy: &layout.cache().join("update").join("prefs.json"),
        receipt: &layout.config().join(update_prefs::RECEIPT_FILE),
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0);
    let migration = update_prefs::migrate(paths, now);
    let (human, detail) = match &migration {
        Migration::Nothing => return None,
        Migration::Migrated { auto_check, .. } => (
            format!(
                "已把旧更新偏好迁入主配置：update.auto_check = {auto_check}（收据 {}）",
                paths.receipt.display()
            ),
            format!("update.auto_check = {auto_check} 已迁移"),
        ),
        Migration::NewValueWins { auto_check, .. } => (
            format!(
                "旧更新偏好文件已退役（主配置里已有 update.auto_check = {auto_check}，新值胜出；收据 {}）",
                paths.receipt.display()
            ),
            "旧文件已退役，主配置的值未改动".to_string(),
        ),
        Migration::Unreadable { path, reason } => (
            format!("旧更新偏好读不出来（{}）：{reason}", path.display()),
            "旧文件坏，未迁移".to_string(),
        ),
        Migration::Refused { reason } => (
            format!("旧更新偏好未迁移：{reason}"),
            "主配置不可写，未迁移".to_string(),
        ),
    };
    Some(PrefsNote {
        human,
        data: json!({
            "id": migration.as_str(),
            "receipt": migration.receipt().map(|path| path.display().to_string()),
            "detail": detail,
        }),
    })
}
