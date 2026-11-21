//! 数据目录迁移：计划 → 执行 → 失败回滚 → 收据 + `redirect.json` 指针。
//!
//! 设计出处：`DevDocs/PlanDoc/04-data-and-config.md` §1 细节——
//! 「迁移 = 移动 + 原地留 redirect.json 指针；迁移失败自动回滚（HMCL `MigrationReceipt` 指纹防重放思想）」。
//!
//! 安全取舍（都是显式拒绝，不猜用户意图）：
//! 1. 目标目录已有数据 → 拒绝（不合并，避免半合并状态）；
//! 2. 任一步失败 → 逆序回滚已搬的分区；回滚也失败 → [`PlatformError::RollbackFailed`] 要求人工介入；
//! 3. `cache/` 是纯缓存，默认一起搬但可以关掉。

use std::io;
use std::path::{Path, PathBuf};

use super::data_dir::ResolvedData;
use super::error::{io_at, PlatformError, PlatformResult};
use super::layout::{LauncherLayout, Partition};
use super::receipt::{self, MigrationReceipt};
use super::redirect::{self, RedirectPointer, REDIRECT_SCHEMA_VERSION};

/// 迁移选项。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirMigrationOptions {
    /// 是否连 `cache/` 一起搬（关掉则目标侧重建缓存）。
    pub migrate_cache: bool,
}

impl Default for DirMigrationOptions {
    fn default() -> Self {
        Self {
            migrate_cache: true,
        }
    }
}

/// 一步分区搬运。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveStep {
    /// 分区。
    pub partition: Partition,
    /// 源目录。
    pub source: PathBuf,
    /// 目标目录。
    pub target: PathBuf,
}

/// 已校验过的迁移计划。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirMigrationPlan {
    /// 源布局。
    pub from: ResolvedData,
    /// 目标布局。
    pub to: ResolvedData,
    /// 要搬的分区（按搬动顺序：config → data → logs → cache）。
    pub steps: Vec<MoveStep>,
}

/// 迁移结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirMigrationOutcome {
    /// 已搬的分区（按搬动顺序）。
    pub moved: Vec<Partition>,
    /// 写入目标的收据。
    pub receipt: MigrationReceipt,
    /// 写入旧目录的指针。
    pub pointer: RedirectPointer,
    /// 收据落盘路径。
    pub receipt_path: PathBuf,
    /// 指针落盘路径。
    pub pointer_path: PathBuf,
}

/// 搬动顺序：配置文件最重要、缓存最后（且可跳过）。
const ORDER: [Partition; 4] = [
    Partition::Config,
    Partition::Data,
    Partition::Logs,
    Partition::Cache,
];

/// 生成计划（不改动磁盘，只校验）。目标目录已存在且非空 → 拒绝。
///
/// # Errors
/// 源目标相同、源目录不存在、目标非空、没有可搬分区时返回 [`PlatformError::MigrationRefused`]。
pub fn plan(
    from: &ResolvedData,
    to: &ResolvedData,
    options: &DirMigrationOptions,
) -> PlatformResult<DirMigrationPlan> {
    if from.same_target(to) {
        return Err(PlatformError::MigrationRefused {
            reason: format!("源与目标相同: {}", from.launcher_home.display()),
        });
    }
    if !from.launcher_home.is_dir() {
        return Err(PlatformError::MigrationRefused {
            reason: format!("源数据目录不存在: {}", from.launcher_home.display()),
        });
    }
    if is_non_empty_dir(&to.launcher_home) {
        return Err(PlatformError::MigrationRefused {
            reason: format!("目标数据目录已有数据: {}", to.launcher_home.display()),
        });
    }

    let from_layout = LauncherLayout::new(from.launcher_home.clone());
    let to_layout = LauncherLayout::new(to.launcher_home.clone());
    let mut steps = Vec::new();
    for partition in ORDER {
        if partition.is_disposable() && !options.migrate_cache {
            continue;
        }
        let source = from_layout.partition(partition);
        if !source.is_dir() {
            continue;
        }
        steps.push(MoveStep {
            partition,
            source,
            target: to_layout.partition(partition),
        });
    }
    if steps.is_empty() {
        return Err(PlatformError::MigrationRefused {
            reason: format!(
                "源数据目录里没有可搬的分区: {}",
                from.launcher_home.display()
            ),
        });
    }
    Ok(DirMigrationPlan {
        from: from.clone(),
        to: to.clone(),
        steps,
    })
}

/// 执行计划：逐分区搬 → 失败逆序回滚 → 成功后写收据 + 旧目录指针。
///
/// # Errors
/// 写/搬失败且回滚成功 → 返回原始错误（磁盘已还原）；回滚也失败 → [`PlatformError::RollbackFailed`]。
pub fn execute(plan: &DirMigrationPlan, now_ms: u64) -> PlatformResult<DirMigrationOutcome> {
    let mut applied: Vec<&MoveStep> = Vec::new();
    for step in &plan.steps {
        match move_tree(&step.source, &step.target) {
            Ok(()) => applied.push(step),
            Err(err) => return Err(rollback(&applied, err)),
        }
    }

    let from_layout = LauncherLayout::new(plan.from.launcher_home.clone());
    let to_layout = LauncherLayout::new(plan.to.launcher_home.clone());
    std::fs::create_dir_all(from_layout.home()).map_err(io_at(from_layout.home()))?;
    std::fs::create_dir_all(to_layout.config()).map_err(io_at(to_layout.config()))?;

    let receipt = MigrationReceipt::data_dir_move(
        plan.from.launcher_home.clone(),
        plan.to.launcher_home.clone(),
        now_ms,
    );
    let receipt_path = to_layout.migration_receipt();
    receipt::write(&receipt_path, &receipt)?;

    let pointer = RedirectPointer {
        schema_version: REDIRECT_SCHEMA_VERSION,
        mode: plan.to.mode,
        launcher_home: plan.to.launcher_home.clone(),
        game_home: plan.to.game_home.clone(),
        migrated_at_ms: now_ms,
    };
    let pointer_path = redirect::write(from_layout.home(), &pointer)?;

    Ok(DirMigrationOutcome {
        moved: plan.steps.iter().map(|step| step.partition).collect(),
        receipt,
        pointer,
        receipt_path,
        pointer_path,
    })
}

/// 逆序回滚已搬分区；回滚本身失败时升级为 [`PlatformError::RollbackFailed`]。
fn rollback(applied: &[&MoveStep], cause: PlatformError) -> PlatformError {
    for step in applied.iter().rev() {
        if let Err(rollback_err) = move_tree(&step.target, &step.source) {
            return PlatformError::RollbackFailed {
                reason: format!(
                    "分区 {} 搬运失败（{cause}），且回滚 {} 失败（{rollback_err}）",
                    step.partition.as_str(),
                    step.partition.as_str()
                ),
            };
        }
    }
    cause
}

/// 搬一棵目录树：优先 `rename`（同盘瞬时）；跨盘失败回落「复制 + 校验 + 删源」。
///
/// 目标已存在则拒绝（绝不合并）。源不存在视为无事可做。
fn move_tree(source: &Path, target: &Path) -> PlatformResult<()> {
    if !source.exists() {
        return Ok(());
    }
    if target.exists() {
        return Err(PlatformError::MigrationRefused {
            reason: format!("目标已存在，拒绝覆盖: {}", target.display()),
        });
    }
    let parent = target.parent().unwrap_or_else(|| Path::new(""));
    if !parent.as_os_str().is_empty() {
        std::fs::create_dir_all(parent).map_err(io_at(parent))?;
    }

    match std::fs::rename(source, target) {
        Ok(()) => Ok(()),
        Err(_) => {
            // 跨盘（便携盘 → 内置盘）时 rename 会失败，改用复制 + 删源。
            copy_tree(source, target).map_err(|err| {
                std::fs::remove_dir_all(target).ok();
                PlatformError::Io {
                    path: target.to_path_buf(),
                    source: err,
                }
            })?;
            std::fs::remove_dir_all(source).map_err(io_at(source))
        }
    }
}

/// 递归复制（不跟随符号链接，避免把外部大目录拖进来）。
fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let destination = to.join(entry.file_name());
        if file_type.is_dir() {
            copy_tree(&entry.path(), &destination)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), &destination)?;
        }
    }
    Ok(())
}

/// 目录存在且非空。
fn is_non_empty_dir(path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }
    std::fs::read_dir(path).is_ok_and(|mut entries| entries.next().is_some())
}
