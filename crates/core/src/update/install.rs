//! 原子替换：先把新包落到**目标同目录**的中转名，再把旧文件改名挪开、中转名顶上；
//! 任一步失败都能回滚，**目标路径上永远不会出现半截文件**。
//!
//! # 为什么不用「直接覆盖」
//!
//! Windows 上**正在运行的 exe** 不能被覆盖、也不能被删除（映像被占用，改字节这条路根本
//! 不存在）；`std::fs::rename` 走 `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`，目标只要被任何
//! 「不共享删除」的句柄占着就报共享冲突（`os error 32`）。剩下唯一可用的动作是**改名**：
//! 把旧文件挪到同目录的备份名，让新文件顶替原来的路径。而改名本身也可能被拒绝（杀软扫描
//! 句柄、编辑器打开、另一个进程正在写），所以失败必须能回到旧版本——这正是本模块存在的理由。
//!
//! # 为什么先落中转名（M171 跨卷修复）
//!
//! 暂存包在 `<数据根>/cache/update/staging`，目标 exe 在安装目录：**两者可能不在同一卷**
//! （便携版放 U 盘、数据根在 D 盘而程序在 C 盘、部分 macOS 配置）。`rename` 不能跨卷
//! （Unix `EXDEV` / Windows `ERROR_NOT_SAME_DEVICE`，`std` 不会偷偷降级成复制），
//! 于是「暂存包 → 目标」那一步必然失败——M169 复审判定这是已发布版本上的真实 bug。
//!
//! 修法不是「把 rename 换成 copy」（那会让目标路径上出现半截文件），而是把**原子替换的前提**
//! 变成不依赖跨卷：
//!
//! ```text
//! 0. 新包 → <目标同目录>/.<文件名>.ullnew-<pid>-<seq>    同卷改名；跨卷时复制到这儿并核对字节数
//! 1. dest 存在 → rename(dest → <同目录 .ullold-…>)      旧版本先安全落地
//! 2. rename(中转名 → dest)                              新版本就位（同卷，原子）
//! 3a. 成功 → 返回 InstallReceipt（备份留着，等新版真的能起来再删）
//! 3b. 失败 → rename(备份 → dest) 回滚，并把「回滚是否成功」如实报给调用方
//! ```
//!
//! 关键性质：**第 2 步之后目标路径要么是旧字节要么是新字节**，不可能是一半。跨卷那条路多出来的
//! 复制只发生在第 0 步，落的是中转名：复制到一半失败 → 删中转名、目标一个字节都没动
//! （多数情况下备份都还没建）。同一卷上走的是改名不是复制，**没有一个字节被多搬一次**。
//!
//! Unix 上也照这个顺序跑（Unix 本可直接 `rename` 覆盖，但两套语义分叉之后，回滚路径只有 Windows
//! 被真正跑过，最容易坏的那一半就没人测了）。
//!
//! # 旧版本备份留几份
//!
//! 替换成功的那一刻会把**同目录里更早的** `.<同名文件>.ullold-*` 删掉，只留本次这一份
//! （见 `prune_stale_artifacts`）：旧实现在每次 apply 都留一份、而全仓**没有任何调用方**调
//! [`InstallReceipt::remove_backup`]，等于每更新一次就在安装目录里堆一个 20+ MB 的 exe。
//! 「留多份」唯一能多给的东西是「回退到更早的版本」，而回滚只用最近一份，所以按份数上界收敛成 1。
//!
//! **一条刻意的例外**：本次安装前 `dest` 就不存在时（`keep = None`）一个 `.ullold-*` 都不删，
//! 只清残留的 `.ullnew-*`。理由见 `prune_stale_artifacts`：那个时刻的旧备份可能是崩溃现场里
//! **唯一幸存的旧 exe**（上一次装在「dest → 备份」之后、「中转名 → dest」之前被杀），
//! 删掉它等于抹掉用户手工恢复的最后一份材料。
//!
//! # 本模块**不**负责的事
//!
//! * **不判断现在能不能替换**：占用/杀软/权限都在 [`ReplacePolicy`] 的重试里撞，撞不动就回滚。
//!   **正在运行的 exe 照样能换**——这一点 M159 在 Windows 真机验过（`ull update apply` 在自己
//!   运行中完成了 `rename(dest→备份)` + `rename(暂存→dest)`，替换后的 exe 仍能启动）：
//!   Windows 加载器打开映像时带 `FILE_SHARE_DELETE`，所以**可以改名**，只是**不能覆盖/删除**
//!   （改字节这条路根本不存在）。这也是本模块「先改名、再搬入」而不是「直接覆盖」的原因。
//!   别照早先那版文档去设计「退出自身 → 由另一个进程搬运」的双进程方案（HMCL 的 `--apply-to`）：
//!   那条路在 Windows 上不是必需的。（Unix 上 `rename` 本就可以覆盖，两步写法是为了两边同一条
//!   回滚路径，见上。）
//! * **不装安装器形态**（msi/dmg/deb/rpm）：那些该交给系统包管理器，
//!   [`crate::update::PackageFormat::needs_installer`] 是给上层的判据。
//! * **不重启、不删备份**（最新一份何时删由上层决定：新版启动成功后
//!   [`InstallReceipt::remove_backup`]）。
//!
//! # 单测覆盖到哪
//!
//! 占用场景是**真的造出来**的：测试里用 `share_mode(FILE_SHARE_READ)` 打开目标文件
//! （**不带** `FILE_SHARE_DELETE`，与 Windows 加载器打开正在运行的映像时一致），
//! 让 `rename` 撞共享冲突，于是在 Windows 上能看到「重试 → 仍失败 → 旧版本完好」。
//! 那条用例是 `#[cfg(windows)]`：Unix 允许改名打开中的文件，本来就该成功。
//!
//! 跨卷场景是**真的跨卷**（`install_xdev_cases` 拿一次真实 rename 探出第二块卷），真盘上没法
//! 确定性复现的「跨卷被拒」「复制/改名到一半失败」则由 `install_io_cases` 的注入器打出来。
//! 它都不是「运行中的 exe」——那需要真的跑一个子进程并替换它，属真机验收，不在单测范围。
//! 另外注意：`std::fs::File::open` **默认共享删除**，用它造的「占用」不生效，
//! 写这类用例时必须显式指定 `share_mode`。

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::error::UpdateError;
use super::replace_io::{is_cross_device, rename_with_retry, RealFs, ReplaceFs};
use super::verify::StagedPackage;

pub use super::replace_io::ReplacePolicy;

/// 同进程内备份 / 中转文件名去重序号。
static SEQ: AtomicU64 = AtomicU64::new(0);

/// 旧版本备份的名字标签（`.<文件名>.ullold-<pid>-<seq>`）。
const OLD_TAG: &str = "ullold";

/// 新版本中转名的名字标签（`.<文件名>.ullnew-<pid>-<seq>`）。
const NEW_TAG: &str = "ullnew";

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
        rename_with_retry(&RealFs, backup, &self.dest, policy)
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
/// 目录建不出来、目标被占用、改名/跨卷复制失败都返回 [`InstallFailure`]，
/// 其中的 `rolled_back` 如实反映旧版本有没有回到原位。
pub fn install_staged(
    staged: &StagedPackage,
    dest: &Path,
    policy: ReplacePolicy,
) -> Result<InstallReceipt, InstallFailure> {
    install_with(&RealFs, staged, dest, policy)
}

/// [`install_staged`] 的注入版：文件动作用调用方给的 [`ReplaceFs`]。
///
/// 判据与顺序与本函数完全相同——测试只能改「系统调用成不成功」，改不了「失败时目标是否完好」。
pub(crate) fn install_with(
    fs: &dyn ReplaceFs,
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

    // 第 0 步：新包 → 目标同目录的中转名。**先做这一步**：它在目标没被碰过的情况下把
    // 「搬不动」暴露出来（跨卷、临时目录写不进），失败时连备份都不用建。
    let new = new_path(dest);
    if let Err(error) = materialize(fs, staged, &new, policy) {
        let _ = fs.remove(&new);
        return Err(failed(UpdateError::io_at(dest, &error), true, None));
    }

    // 第 1 步：dest → 同目录备份。
    let mut backup = None;
    if fs::symlink_metadata(dest).is_ok() {
        let path = backup_path(dest);
        if let Err(error) = rename_with_retry(fs, dest, &path, policy) {
            let _ = fs.remove(&new);
            return Err(failed(UpdateError::io_at(dest, &error), true, None));
        }
        backup = Some(path);
    }

    // 第 2 步：中转名 → dest（同卷，原子）。
    match rename_with_retry(fs, &new, dest, policy) {
        Ok(()) => {
            prune_stale_artifacts(backup.as_deref(), dest);
            Ok(InstallReceipt {
                dest: dest.to_path_buf(),
                backup,
                bytes: staged.size(),
            })
        }
        Err(error) => {
            let _ = fs.remove(&new);
            let error = UpdateError::io_at(dest, &error);
            let Some(backup_path) = backup else {
                // 目标原本不存在：没有旧版本要恢复，目标也没被写坏，
                // 所以按「旧版本仍在原位」上报（新安装失败 ≠ 现有安装被破坏）。
                return Err(failed(error, true, None));
            };
            match rename_with_retry(fs, &backup_path, dest, policy) {
                Ok(()) => Err(failed(error, true, None)),
                Err(_) => Err(failed(error, false, Some(backup_path))),
            }
        }
    }
}

/// 第 0 步：把暂存包落成 `new`（目标同目录）。同卷改名一步到位；跨卷才复制，并核对字节数。
fn materialize(
    fs: &dyn ReplaceFs,
    staged: &StagedPackage,
    new: &Path,
    policy: ReplacePolicy,
) -> std::io::Result<()> {
    match rename_with_retry(fs, staged.path(), new, policy) {
        Ok(()) => Ok(()),
        // 跨卷不重试：换多少次都不会变成同卷。复制是**唯一**的降级方向，而且只能往
        // 目标同目录的中转名里复制——绝不许直接往 dest 上复制。
        Err(error) if is_cross_device(&error) => copy_across_volumes(fs, staged, new),
        Err(error) => Err(error),
    }
}

/// 跨卷降级：复制到中转名，并核对写出的字节数（少了就是半截，直接判失败）。
///
/// 复制**不重试**：它不是占用问题，重来一次只会再搬几十 MB。失败后留下的半截文件由
/// `install_with` 删掉（`fs::copy` 自己不会收拾）。
fn copy_across_volumes(
    fs: &dyn ReplaceFs,
    staged: &StagedPackage,
    new: &Path,
) -> std::io::Result<()> {
    let copied = fs.copy(staged.path(), new)?;
    if copied != staged.size() {
        return Err(std::io::Error::other(format!(
            "跨卷复制只写出 {copied} 字节，暂存包是 {} 字节（拒绝把它顶到目标上）",
            staged.size()
        )));
    }
    Ok(())
}

fn failed(error: UpdateError, rolled_back: bool, backup_left: Option<PathBuf>) -> InstallFailure {
    InstallFailure {
        error,
        rolled_back,
        backup_left,
    }
}

fn tagged_path(dest: &Path, tag: &str) -> PathBuf {
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let name = dest.file_name().map_or_else(
        || String::from("ull"),
        |name| name.to_string_lossy().into_owned(),
    );
    let tagged = format!(".{name}.{tag}-{}-{seq}", std::process::id());
    match dest.parent() {
        Some(parent) => parent.join(tagged),
        None => PathBuf::from(tagged),
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
    tagged_path(dest, OLD_TAG)
}

/// 新版本的中转名：`<dest 同目录>/.<文件名>.ullnew-<pid>-<seq>`。
///
/// **必须与 dest 同目录**：这是「不经复制就把新字节原子地放到目标位置上」的全部前提。
fn new_path(dest: &Path) -> PathBuf {
    tagged_path(dest, NEW_TAG)
}

/// 清同目录里的替换残留：只留最近一份旧版本备份，顺带扫掉更早失败遗留的中转名。
///
/// * `keep = Some(本次新建的备份)`：删掉同目录里**同名目标**更早的 `.<名>.ullold-*`，只留这一份；
/// * `keep = None`（本次安装前 `dest` 本来不存在）：**一个 `.ullold-*` 都不动**，只清 `.ullnew-*`。
///
/// `keep = None` 这条例外是 M171 r2 P3-2 的结论：`.<名>.ullold-*` 在那个时刻极可能是**用户旧版本
/// 唯一幸存的副本**——上一次安装在「dest → 备份」之后、「中转名 → dest」之前被杀（掉电、被结束
/// 进程），于是 dest 空着、旧字节全在备份里。把它当垃圾删掉，用户就彻底失去手工恢复的材料；
/// 而修前（没有清理）根本不会删。宁可留一份几十 MB 的垃圾：等 dest 重新存在（下一次成功替换
/// ⇒ `keep = Some`），这些旧备份会自然收敛成一份。
///
/// `.ullnew-*` 则任何时候都是垃圾：本次那份在被调用前已经改名成 dest 了，还留着的只可能是
/// 更早某次失败遗留的中转文件（本次失败路径自己会删，删不掉才会留到这里）。
///
/// 删失败不当错误（只是垃圾文件，下次 apply 再清）；只在替换成功之后调用，所以正在用的
/// 那一份（`keep`）不会被动。
///
/// 前提是「同一个 exe 只有一个更新者在动」。两个进程同时替换同一路径本来就不成立：
/// 后一个回滚会把前一个刚装上的新版本换成旧字节。这条不因为保留多份而变好，反倒因为
/// 每份都是几十 MB 的 exe 而变坏。
fn prune_stale_artifacts(keep: Option<&Path>, dest: &Path) {
    let (Some(dir), Some(name)) = (dest.parent(), dest.file_name()) else {
        return;
    };
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let name = name.to_string_lossy();
    let old_prefix = format!(".{name}.{OLD_TAG}-");
    let new_prefix = format!(".{name}.{NEW_TAG}-");
    for entry in entries.flatten() {
        let path = entry.path();
        if Some(path.as_path()) == keep {
            continue;
        }
        let stale = path.file_name().is_some_and(|other| {
            let other = other.to_string_lossy();
            other.starts_with(&new_prefix) || (keep.is_some() && other.starts_with(&old_prefix))
        });
        if stale {
            let _ = fs::remove_file(&path);
        }
    }
}
