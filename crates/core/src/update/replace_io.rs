//! 替换过程真正碰盘的三个动作（改名 / 复制 / 删除）+ 重试与跨卷判据。
//!
//! # 为什么抽成 trait
//!
//! 只为一件事：**故障注入**。两条最关键的坏路径在真盘上没法确定性复现——
//!
//! * 「跨卷 rename 被拒」（`EXDEV` / `ERROR_NOT_SAME_DEVICE`）：得先有第二块卷；
//! * 「复制/改名到一半失败」：得让盘真的满，或者让杀软恰好在那 100ms 里咬住文件。
//!
//! 真实现 [`RealFs`] 就是 `std::fs` 的三个函数（薄到没有分支）；测试用注入器把上面两种
//! 失败变成 `install_with` 上的确定性输入。抽的只有「动作」，**判据与顺序仍在 `install` 里**，
//! 所以注入器能改的只是「系统调用成不成功」，改不了「失败时目标是否仍是旧版本」这条规矩。
//!
//! # 跨卷为什么不能退化成「直接覆盖」
//!
//! Windows 上正在运行的 exe 不能覆盖、只能改名；`rename` **不能跨卷**（`std` 不带
//! `MOVEFILE_COPY_ALLOWED`，也不会偷偷降级成复制）。所以唯一既原子又能跨卷的姿势是：
//! 先把字节搬到**目标同目录**的临时名（同卷内改名 = 原子），再由一次同卷改名把临时名换成目标名。
//! 目标路径上的新字节永远来自一次原子命名，任何时刻都不存在「拷了一半的目标文件」。

use std::fs;
use std::io;
use std::path::Path;
use std::time::Duration;

/// 替换策略：改名失败时重试几次、每次间隔多久。
///
/// 默认 5 次 × 80ms：足够扛过杀软扫描/索引器短暂占用，又不至于让用户在「更新」按钮上
/// 干等半分钟。真正长期的占用（另一个启动器实例在跑）重试多少次都没用，直接报错更诚实。
///
/// 跨卷（`CrossesDevices`）**不重试**：换多少次都不会变成同卷，那是复制降级该接的信号
/// （见 `install::materialize`）。
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

/// 替换用到的三个文件动作（真实现 = `std::fs`）。
pub(crate) trait ReplaceFs {
    /// 同卷改名。
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// 复制（跨卷降级用）；返回实际写出的字节数。
    fn copy(&self, from: &Path, to: &Path) -> io::Result<u64>;
    /// 删文件（清理中转名）。
    fn remove(&self, path: &Path) -> io::Result<()>;
}

/// 真实现。
pub(crate) struct RealFs;

impl ReplaceFs for RealFs {
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    fn copy(&self, from: &Path, to: &Path) -> io::Result<u64> {
        fs::copy(from, to)
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }
}

/// 改名，失败时按策略重试（**只有占用类错误**值得重试）。
pub(crate) fn rename_with_retry(
    fs: &dyn ReplaceFs,
    from: &Path,
    to: &Path,
    policy: ReplacePolicy,
) -> io::Result<()> {
    let attempts = policy.attempts.max(1);
    let mut last = None;
    for attempt in 0..attempts {
        match fs.rename(from, to) {
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
/// `CrossesDevices` **不在**这里：它不是占用，是「这块地搬不过去」，重试零收益。
pub(crate) fn is_transient(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::PermissionDenied
            | io::ErrorKind::Other
            | io::ErrorKind::Interrupted
            | io::ErrorKind::WouldBlock
    )
}

/// 是否「源与目标不同卷」：Unix `EXDEV` / Windows `ERROR_NOT_SAME_DEVICE`，`std` 都映射到
/// [`io::ErrorKind::CrossesDevices`]。
pub(crate) fn is_cross_device(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::CrossesDevices
}
