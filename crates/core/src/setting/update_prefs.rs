//! 自更新偏好 `<数据根>/cache/update/prefs.json` → `<数据根>/config/launcher.json` 的迁移（M161）。
//!
//! # 为什么要有这一步
//!
//! M159 把「启动时静默检查」临时落在更新域自己的文件里（当时 `crates/core/src/setting/**`
//! 不在那条 mission 的 scope）。后果是**同一个开关有两个真相源**：GUI 读写 `prefs.json`，
//! 而 `ull config get update.auto_check` 读 `launcher.json`——用户在 GUI 里关掉自动检查，
//! CLI 永远回答默认值 `true`。M161 把两个键（[`KEY_AUTO_CHECK`] / [`KEY_CHANNEL`]）登记进
//! schema，本模块负责把**存量值**搬过去。
//!
//! # 四条硬约束（每条都有对应用例，见 `tests/setting/update_keys.rs`）
//!
//! 1. **幂等**：跑两次的第二遍什么都不做。判据不是「文件还在不在」（改名可能失败），而是
//!    [`crate::platform::receipt`] 里的源指纹——同一份源文件内容只迁一次；
//! 2. **不丢值**：旧文件里 `auto_check: false` 必须迁成 `false`。默认值是 `true`，所以
//!    「用默认值兜底」会把用户的关闭状态**静默打开**——这是本条里最贵的一种错；
//! 3. **新值胜出**：主配置里**已经有**这个键时绝不覆盖（用户可能在前一版 GUI 之后手改过
//!    `launcher.json`，或者刚从 CLI 设过）。旧文件照样退役，但不写字；
//! 4. **旧文件迁完不能再被读**：改名为 `prefs.json.migrated`（不删——留着是取证材料，
//!    但不再有任何读路径，`Prefs::load` 已经从宿主层删掉）。
//!
//! 写不进去（只读/不可读的 `launcher.json`）时**不退役旧文件**：宁可下一次再试，
//! 也不能让用户的值在两边都不存在。

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::json;

use crate::platform::receipt::{self, MigrationReceipt};
use crate::setting::document::SettingDocument;
use crate::setting::keys::launcher_schema;
use crate::setting::store::SettingStore;

/// 主配置里的「启动时静默检查」键。
pub const KEY_AUTO_CHECK: &str = "update.auto_check";
/// 主配置里的「更新渠道」键。
pub const KEY_CHANNEL: &str = "update.channel";
/// 旧域内文件里的同义键（`serde` 的字段名，别改）。
pub const LEGACY_AUTO_CHECK: &str = "auto_check";
/// `update.auto_check` 的出厂值（与 `keys::launcher_schema` 里那份**必须一致**，契约用例守着）。
pub const DEFAULT_AUTO_CHECK: bool = true;
/// `update.channel` 的出厂值。
pub const DEFAULT_CHANNEL: &str = "stable";
/// 收据文件名（落在 `<数据根>/config/` 下；与数据目录迁移的 `migration-receipt.json` 分开）。
pub const RECEIPT_FILE: &str = "update-prefs-migration.json";
/// 旧文件退役后的后缀（`prefs.json` → `prefs.json.migrated`）。
pub const RETIRED_SUFFIX: &str = "migrated";

/// 迁移需要知道的三个路径（都由调用方按数据根算：core 不猜布局）。
#[derive(Debug, Clone, Copy)]
pub struct Targets<'a> {
    /// `<数据根>/config/launcher.json`
    pub launcher: &'a Path,
    /// `<数据根>/cache/update/prefs.json`（旧域内文件）
    pub legacy: &'a Path,
    /// 收据落点（`<数据根>/config/update-prefs-migration.json`）
    pub receipt: &'a Path,
}

/// 迁移结局（**每一种都要能被调用方如实转述**，不许折叠成 bool）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Migration {
    /// 没有旧文件（绝大多数用户），或已经迁过（源指纹相同）。
    Nothing,
    /// 迁进去了：值来自旧文件。
    Migrated {
        /// 迁过去的取值
        auto_check: bool,
        /// 收据路径
        receipt: PathBuf,
        /// 退役后的旧文件路径
        retired: PathBuf,
    },
    /// 主配置里已经有这个键 ⇒ 新值胜出，一个字都没写；旧文件照样退役。
    NewValueWins {
        /// 主配置里那个生效值
        auto_check: bool,
        /// 收据路径
        receipt: PathBuf,
        /// 退役后的旧文件路径
        retired: PathBuf,
    },
    /// 旧文件读不出来（坏 JSON / 读失败）：**什么都不动**，旧文件原样留着等下一次。
    Unreadable {
        /// 旧文件路径
        path: PathBuf,
        /// 原因（人读）
        reason: String,
    },
    /// 主配置写不进去：不退役旧文件（用户的值不能两边都没有）。
    Refused {
        /// 原因（人读）
        reason: String,
    },
}

impl Migration {
    /// 有没有真的改动主配置或旧文件。
    #[must_use]
    pub const fn touched(&self) -> bool {
        matches!(self, Self::Migrated { .. } | Self::NewValueWins { .. })
    }

    /// 收据路径（没写收据时为 `None`）。
    #[must_use]
    pub fn receipt(&self) -> Option<&Path> {
        match self {
            Self::Migrated { receipt, .. } | Self::NewValueWins { receipt, .. } => Some(receipt),
            _ => None,
        }
    }

    /// 稳定 id（日志 / CLI 输出 / 测试用；**不要改名**）。
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Nothing => "nothing",
            Self::Migrated { .. } => "migrated",
            Self::NewValueWins { .. } => "new_value_wins",
            Self::Unreadable { .. } => "unreadable",
            Self::Refused { .. } => "refused",
        }
    }
}

/// 旧域内文件的结构（只有一个字段，但**保留 `serde(default)`**：
/// 少了字段的旧文件按出厂值处理，而不是整个解析失败）。
#[derive(Debug, Clone, Copy, Deserialize)]
struct LegacyPrefs {
    #[serde(default = "default_auto_check")]
    auto_check: bool,
}

const fn default_auto_check() -> bool {
    DEFAULT_AUTO_CHECK
}

/// 执行一次迁移（幂等）。
///
/// `now_ms` = 毫秒 since UNIX_EPOCH（注入便于断言收据内容）。
#[must_use]
pub fn migrate(targets: Targets<'_>, now_ms: u64) -> Migration {
    let Ok(bytes) = std::fs::read(targets.legacy) else {
        // 文件不在（或读不动=一样当没有）：绝大多数用户走这一支。
        return Migration::Nothing;
    };
    let fingerprint = receipt::fingerprint_bytes(&bytes);

    // ① 收据闸门：同一份源文件内容只迁一次。
    if matches!(
        receipt::read(targets.receipt),
        Ok(Some(existing)) if existing.source_fingerprint == fingerprint
    ) {
        retire(targets.legacy);
        return Migration::Nothing;
    }

    // ② 旧文件必须能解析：坏文件不猜（猜错就等于替用户关掉/打开自动检查）。
    let legacy: LegacyPrefs = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(error) => {
            return Migration::Unreadable {
                path: targets.legacy.to_path_buf(),
                reason: error.to_string(),
            }
        }
    };

    let mut store = SettingStore::load(targets.launcher, launcher_schema());
    if !store.access().can_save() {
        return Migration::Refused {
            reason: format!(
                "主配置不可写（{}）：{}",
                targets.launcher.display(),
                store.access().as_str()
            ),
        };
    }

    // ③ 新值胜出：主配置里**显式**有这个键就不覆盖（`effective` 会兜默认值，用 `get`）。
    let explicit = store
        .doc()
        .get(KEY_AUTO_CHECK)
        .and_then(serde_json::Value::as_bool);

    let outcome = if let Some(current) = explicit {
        Migration::NewValueWins {
            auto_check: current,
            receipt: targets.receipt.to_path_buf(),
            retired: retired_path(targets.legacy),
        }
    } else {
        // ④ 只在**非出厂值**时写盘：出厂值（`true`）不落盘是 `document` 的既定语义
        //    （「默认值不落盘」），写进去只是给文件添一行噪音。`false` 必须写。
        if legacy.auto_check != DEFAULT_AUTO_CHECK {
            if let Err(error) = store.set(KEY_AUTO_CHECK, json!(legacy.auto_check)) {
                return Migration::Refused {
                    reason: format!("写 {KEY_AUTO_CHECK} 被拒：{error}"),
                };
            }
            match store.save() {
                Ok(saved) if saved.wrote() || !store.is_dirty() => {}
                Ok(saved) => {
                    return Migration::Refused {
                        reason: format!("主配置没落盘（{}）", format_save(&saved)),
                    }
                }
                Err(error) => {
                    return Migration::Refused {
                        reason: format!("写主配置失败：{error}"),
                    }
                }
            }
        }
        Migration::Migrated {
            auto_check: legacy.auto_check,
            receipt: targets.receipt.to_path_buf(),
            retired: retired_path(targets.legacy),
        }
    };

    // ⑤ 收据（写不成也不回滚主配置：值已经在主配置里生效了，收据丢了下一次会重跑一遍，
    //    而重跑是幂等的——正如「新值胜出」那一支不会覆盖）。
    let record = MigrationReceipt::legacy_config(
        targets.legacy.to_path_buf(),
        targets.launcher.to_path_buf(),
        fingerprint,
        now_ms,
    );
    let _ = receipt::write(targets.receipt, &record);

    // ⑥ 旧文件退役（改名失败就留着：主配置已经是真相源，`prefs.json` 不再有读路径）。
    retire(targets.legacy);
    outcome
}

/// 退役后的路径（`prefs.json` → `prefs.json.migrated`）。
#[must_use]
pub fn retired_path(legacy: &Path) -> PathBuf {
    let name = legacy
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "prefs.json".to_string());
    legacy.with_file_name(format!("{name}.{RETIRED_SUFFIX}"))
}

/// 把旧文件改名成 `.migrated`（不存在 / 改不动都算成功：它不是真相源了）。
fn retire(legacy: &Path) {
    if legacy.exists() {
        let _ = std::fs::rename(legacy, retired_path(legacy));
    }
}

fn format_save(outcome: &crate::setting::store::SaveOutcome) -> String {
    match outcome {
        crate::setting::store::SaveOutcome::Saved(path) => format!("已写 {}", path.display()),
        crate::setting::store::SaveOutcome::Skipped(reason) => reason.as_str().to_string(),
    }
}

/// 「启动时静默检查」的生效值（**唯一**取值点：宿主层与 CLI 都从这里读）。
#[must_use]
pub fn auto_check(doc: &SettingDocument) -> bool {
    doc.effective_bool(KEY_AUTO_CHECK)
        .unwrap_or(DEFAULT_AUTO_CHECK)
}

/// 「更新渠道」的生效值（同上；空串/坏值都回出厂渠道）。
#[must_use]
pub fn channel(doc: &SettingDocument) -> &str {
    doc.effective_str(KEY_CHANNEL)
        .filter(|text| !text.trim().is_empty())
        .unwrap_or(DEFAULT_CHANNEL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retired_name_keeps_the_original_stem() {
        assert_eq!(
            retired_path(Path::new("/data/cache/update/prefs.json")),
            Path::new("/data/cache/update/prefs.json.migrated")
        );
    }

    #[test]
    fn defaults_agree_with_the_schema() {
        // 契约：本文件里的常量与 `keys::launcher_schema()` 的默认值必须一致
        // （两份字面量漂移 = 「迁过去的值」和「不迁时的值」不同）。
        let schema = launcher_schema();
        assert_eq!(
            schema.spec(KEY_AUTO_CHECK).expect("键已登记").default,
            json!(DEFAULT_AUTO_CHECK)
        );
        assert_eq!(
            schema.spec(KEY_CHANNEL).expect("键已登记").default,
            json!(DEFAULT_CHANNEL)
        );
    }
}
