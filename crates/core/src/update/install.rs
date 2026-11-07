//! 原子替换：先把旧文件改名挪开，再把新文件搬进来；任一步失败都能回滚。
//!
//! # 为什么不用「直接覆盖」
//!
//! Windows 上**正在运行的 exe** 不能被覆盖、也不能被删除（映像被占用，改字节这条路根本
//! 不存在）；`std::fs::rename` 走 `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`，目标只要被任何
//! 「不共享删除」的句柄占着就报共享冲突（`os error 32`）。剩下唯一可用的动作是**改名**：
//! 把旧文件挪到同目录的备份名，让新文件顶替原来的路径。而改名本身也可能被拒绝（杀软扫描
//! 句柄、编辑器打开、另一个进程正在写），所以失败必须能回到旧版本——这正是本模块存在的理由。
//!
//! 三步写法，Unix 上也照这个顺序跑（Unix 本可直接 `rename` 覆盖，但两套语义分叉之后，
//! 回滚路径只有 Windows 被真正跑过，最容易坏的那一半就没人测了）：
//!
//! ```text
//! 1. dest 存在 → rename(dest → <同目录备份>)     旧版本先安全落地
//! 2. rename(暂存包 → dest)                       新版本就位
//! 3a. 成功 → 返回 InstallReceipt（备份留着，等新版真的能起来再删）
//! 3b. 失败 → rename(备份 → dest) 回滚，并把「回滚是否成功」如实报给调用方
//! ```
//!
//! # 本模块**不**负责的事
//!
//! * **不判断现在能不能替换**。正在跑的 exe 只能由「退出自身 → 由另一个进程搬运」这条
//!   路换掉（HMCL 的 `--apply-to` 模式，见 analyse/07 §4.3）。这里只保证搬一半失败能回滚。
//! * **不装安装器形态**（msi/dmg/deb/rpm）：那些该交给系统包管理器，
//!   [`crate::update::PackageFormat::needs_installer`] 是给上层的判据。
//! * **不重启、不删备份**。备份何时删由上层决定（新版启动成功后再
//!   [`InstallReceipt::remove_backup`]）。
//!
//! # 单测覆盖到哪
//!
//! 占用场景是**真的造出来**的：测试里用 `share_mode(FILE_SHARE_READ)` 打开目标文件
//! （**不带** `FILE_SHARE_DELETE`，与 Windows 加载器打开正在运行的映像时一致），
//! 让 `rename` 撞共享冲突，于是在 Windows 上能看到「重试 → 仍失败 → 旧版本完好」。
//! 那条用例是 `#[cfg(windows)]`：Unix 允许改名打开中的文件，本来就该成功。
//!
//! 但它不是「运行中的 exe」——那需要真的跑一个子进程并替换它，属真机验收，不在单测范围。
//! 另外注意：`std::fs::File::open` **默认共享删除**，用它造的「占用」不生效，
//! 写这类用例时必须显式指定 `share_mode`。

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use super::error::UpdateError;
use super::verify::StagedPackage;

/// 同进程内备份文件名去重序号。
static SEQ: AtomicU64 = AtomicU64::new(0);

/// 替换策略：改名失败时重试几次、每次间隔多久。
///
/// 默认 5 次 × 80ms：足够扛过杀软扫描/索引器短暂占用，又不至于让用户在「更新」按钮上
/// 干等半分钟。真正长期的占用（另一个启动器实例在跑）重试多少次都没用，直接报错更诚实。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplacePolicy {
    /// 总尝试次数（含首次），`0` 按 `1` 处理。
    pub attempts: u32,
    /// 两次尝试之间的等待。
    pub delay: Duration,
}

impl Default for ReplacePolicy {
    fn default() -> Self {
        Self {
            attempts: 5,
            delay: Duration::from_millis(80),
        }
    }
}

impl ReplacePolicy {
    /// 构造。
    #[must_use]
    pub const fn new(attempts: u32, delay: Duration) -> Self {
        Self { attempts, delay }
    }
}

/// 替换成功后的收据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallReceipt {
    /// 新版本所在的路径。
    pub dest: PathBuf,
    /// 旧版本被挪到哪（原本没有文件时为 `None`）。
    pub backup: Option<PathBuf>,
    /// 新版本字节数。
    pub bytes: u64,
}

impl InstallReceipt {
    /// 删掉旧版本备份（新版启动成功之后调用）。
    ///
    /// # Errors
    ///
    /// 删不掉（还占着、没权限）返回带路径的 [`UpdateError`]——这时**别**当成更新失败，
    /// 只是留了个垃圾文件，下次启动还能再删。
    pub fn remove_backup(&self) -> Result<(), UpdateError> {
        let Some(backup) = &self.backup else {
            return Ok(());
        };
        fs::remove_file(backup).map_err(|error| UpdateError::io_at(backup, &error))
    }

    /// 手动回滚：把备份搬回 `dest`（新版起不来时用）。成功后 `backup` 字段清空。
    ///
    /// # Errors
    ///
    /// 没有备份时是空操作 `Ok(())`；搬不动返回 [`UpdateError`]（此时 `backup` 保持不变，
    /// 旧版本还在原位置，可以再试）。
    pub fn rollback(&mut self, policy: ReplacePolicy) -> Result<(), UpdateError> {
        let Some(backup) = self.backup.as_ref() else {
            return Ok(());
        };
        rename_with_retry(backup, &self.dest, policy)
            .map_err(|error| UpdateError::io_at(&self.dest, &error))?;
        self.backup = None;
        Ok(())
    }
}

/// 替换失败：原始错误 + 回滚结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallFailure {
    /// 原始失败。
    pub error: UpdateError,
    /// 旧版本是否已回到原路径（`true` = 目标没被破坏；本来没有旧文件时也算 `true`）。
    pub rolled_back: bool,
    /// 回滚失败时旧版本留在哪（`None` = 已回到原位或压根没有旧文件）。
    pub backup_left: Option<PathBuf>,
}

impl InstallFailure {
    /// 目标是否可能处于「既不是旧版也不是新版」的坏状态（需要人工介入）。
    #[must_use]
    pub const fn needs_manual_repair(&self) -> bool {
        !self.rolled_back
    }
}

impl fmt::Display for InstallFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.error)?;
        if self.rolled_back {
            f.write_str("（已回滚，旧版本仍在原位）")
        } else {
            let where_ = self.backup_left.as_ref().map_or_else(String::new, |path| {
                format!("，旧文件留在 {}", path.display())
            });
            write!(f, "（回滚失败，需要人工处理{where_}）")
        }
    }
}

impl std::error::Error for InstallFailure {}

/// 把校验过的暂存包装到 `dest`；失败则回滚，旧版本保持可用。
///
/// # Errors
///
/// 目录建不出来、目标被占用、改名失败都返回 [`InstallFailure`]，
/// 其中的 `rolled_back` 如实反映旧版本有没有回到原位。
pub fn install_staged(
    staged: &StagedPackage,
    dest: &Path,
    policy: ReplacePolicy,
) -> Result<InstallReceipt, InstallFailure> {
    if let Some(parent) = dest
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|error| failed(UpdateError::io_at(parent, &error), true, None))?;
    }

    let mut backup = None;
    if fs::symlink_metadata(dest).is_ok() {
        let path = backup_path(dest);
        rename_with_retry(dest, &path, policy)
            .map_err(|error| failed(UpdateError::io_at(dest, &error), true, None))?;
        backup = Some(path);
    }

    match rename_with_retry(staged.path(), dest, policy) {
        Ok(()) => Ok(InstallReceipt {
            dest: dest.to_path_buf(),
            backup,
            bytes: staged.size(),
        }),
        Err(error) => {
            let error = UpdateError::io_at(dest, &error);
            let Some(backup_path) = backup else {
                // 目标原本不存在：没有旧版本要恢复，目标也没被写坏，
                // 所以按「旧版本仍在原位」上报（新安装失败 ≠ 现有安装被破坏）。
                return Err(failed(error, true, None));
            };
            match rename_with_retry(&backup_path, dest, policy) {
                Ok(()) => Err(failed(error, true, None)),
                Err(_) => Err(failed(error, false, Some(backup_path))),
            }
        }
    }
}

fn failed(error: UpdateError, rolled_back: bool, backup_left: Option<PathBuf>) -> InstallFailure {
    InstallFailure {
        error,
        rolled_back,
        backup_left,
    }
}

/// 旧版本的备份路径：`<dest 同目录>/.<文件名>.ullold-<pid>-<seq>`。
///
/// 点号开头在 Unix 上是隐藏文件，也和 `download` 的 `.ullpart-*` 中间文件一个家族；
/// 必须同目录，因为 `rename` **不能跨卷**（Unix 返回 `EXDEV`，Windows 返回
/// `ERROR_NOT_SAME_DEVICE`——`std` 的 `fs::rename` 不带 `MOVEFILE_COPY_ALLOWED`，
/// 不会偷偷降级成复制）。
#[must_use]
pub fn backup_path(dest: &Path) -> PathBuf {
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let name = dest.file_name().map_or_else(
        || String::from("ull"),
        |name| name.to_string_lossy().into_owned(),
    );
    let backup = format!(".{name}.ullold-{}-{seq}", std::process::id());
    match dest.parent() {
        Some(parent) => parent.join(backup),
        None => PathBuf::from(backup),
    }
}

/// 改名，失败时按策略重试（占用类错误才值得重试）。
fn rename_with_retry(from: &Path, to: &Path, policy: ReplacePolicy) -> io::Result<()> {
    let attempts = policy.attempts.max(1);
    let mut last = None;
    for attempt in 0..attempts {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(error) => {
                let retryable = is_transient(&error) && attempt + 1 < attempts;
                if !retryable {
                    return Err(error);
                }
                std::thread::sleep(policy.delay);
                last = Some(error);
            }
        }
    }
    Err(last.unwrap_or_else(|| io::Error::other("rename 未执行")))
}

/// 是否属于「等一会儿可能就好了」的失败。
///
/// Windows 的共享冲突（`os error 32`）落在 `ErrorKind::Other` 里（`std` 没有专门的 kind），
/// 所以这里必须把 `Other` 也算上；`PermissionDenied` 覆盖 `ACCESS_DENIED`（杀软持有的句柄）。
fn is_transient(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::PermissionDenied
            | io::ErrorKind::Other
            | io::ErrorKind::Interrupted
            | io::ErrorKind::WouldBlock
    )
}
