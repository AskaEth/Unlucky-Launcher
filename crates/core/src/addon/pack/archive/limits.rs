//! 读取限额：条目数 / 解压总体积 / 单条目，以及两处封顶实现。
//!
//! # 两阶段限额（打开 vs 读取）
//!
//! 打开阶段（[`check_limits`]）只能拿**中央目录声明的大小**做判断——它会撒谎。
//! 因此读取阶段还有第二道：单条目按实际读出字节封顶（[`read_capped`]），批量读取
//! （[`super::PackArchive::override_files`]）再按**累计实际字节**从 [`ReadBudget`] 里扣减。
//! 没有预算的批量读取等于把 `max_uncompressed_bytes` 降级成"每条单独算"——一个 2 万条目、
//! 每条声明 1 字节却解出几十 MiB 的包能把 CPU/时间耗在这里（r1 p3-4）。

use std::cell::Cell;
use std::io::Read;

use super::super::error::{PackError, PackResult};

/// 默认条目数上限（一个巨型整合包 ≈ 数千条，2 万条已经很宽）。
pub const DEFAULT_MAX_ENTRIES: usize = 20_000;
/// 默认解压总体积上限（512 MiB；`.mrpack` 的 `overrides` 通常几十 MiB）。
pub const DEFAULT_MAX_UNCOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;
/// 默认单条目上限（64 MiB；最大的 mod 也不过十几 MiB）。
pub const DEFAULT_MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;

/// 包读取限额。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackLimits {
    /// 条目数上限。
    pub max_entries: usize,
    /// 解压总体积上限。
    pub max_uncompressed_bytes: u64,
    /// 单条目上限。
    pub max_entry_bytes: u64,
}

impl PackLimits {
    /// 三档显式给值（测试与"可疑包"复查用）。
    #[must_use]
    pub const fn new(
        max_entries: usize,
        max_uncompressed_bytes: u64,
        max_entry_bytes: u64,
    ) -> Self {
        Self {
            max_entries,
            max_uncompressed_bytes,
            max_entry_bytes,
        }
    }
}

impl Default for PackLimits {
    fn default() -> Self {
        Self {
            max_entries: DEFAULT_MAX_ENTRIES,
            max_uncompressed_bytes: DEFAULT_MAX_UNCOMPRESSED_BYTES,
            max_entry_bytes: DEFAULT_MAX_ENTRY_BYTES,
        }
    }
}

/// 一次批量读取的**累计**预算（读取阶段，按实际读出的字节扣减）。
///
/// 作用域是"拿到它的那次扫描"：`PackArchive::override_files` 每次调用新建一份，
/// 所以要跨多次调用共享额度（例如整包只给一份 `max_uncompressed_bytes`），
/// 必须由调用方自建一个并交给 `override_files_with`（r2 p3-b）。
///
/// 为什么需要它：中央目录声明的大小可以撒谎，单个条目也被限制在
/// [`PackLimits::max_entry_bytes`]，但"很多条各自合规"累起来仍可能远超
/// `max_uncompressed_bytes`。批量读取（`override_files`）持有一个预算，边读边扣。
#[derive(Debug)]
pub struct ReadBudget {
    remaining: Cell<u64>,
}

impl ReadBudget {
    /// 按总预算新建。
    #[must_use]
    pub fn new(limit: u64) -> Self {
        Self {
            remaining: Cell::new(limit),
        }
    }

    /// 还剩多少字节。
    #[must_use]
    pub fn remaining(&self) -> u64 {
        self.remaining.get()
    }

    /// 扣减一次实际读出的字节数；超额报 `pack_limits`。
    ///
    /// 超额时预算**照样扣到 0**（这些字节已经从流里读出来了），后续调用会继续失败——
    /// 不给"失败一次、下次又能读一点"的机会。
    pub(super) fn spend(&self, bytes: u64, name: &str) -> PackResult<()> {
        let remaining = self.remaining.get();
        self.remaining.set(remaining.saturating_sub(bytes));
        if bytes > remaining {
            return Err(PackError::limits(format!(
                "{name} 之后累计解压超过上限（本次还需 {bytes} 字节，预算已用尽）"
            )));
        }
        Ok(())
    }
}

/// 累计与单条目限额检查（打开阶段）。
pub(super) fn check_limits(
    limits: &PackLimits,
    total: &mut u64,
    size: u64,
    name: &str,
    seen: usize,
) -> PackResult<()> {
    if size > limits.max_entry_bytes {
        return Err(PackError::limits(format!(
            "条目 {name} 声明 {} 字节，超过单条目上限 {}",
            size, limits.max_entry_bytes
        )));
    }
    *total = total.saturating_add(size);
    if *total > limits.max_uncompressed_bytes {
        return Err(PackError::limits(format!(
            "解压总体积超过上限 {} 字节（累计到 {total}）",
            limits.max_uncompressed_bytes
        )));
    }
    if seen >= limits.max_entries {
        return Err(PackError::limits(format!(
            "条目数超过上限 {}",
            limits.max_entries
        )));
    }
    Ok(())
}

/// 带封顶的读：中央目录里声明的大小可以撒谎，这里按**实际读出的字节**封顶。
///
/// 给了 `budget` 就同时从累计预算里扣（批量读取）；单条读取给 `None`，
/// 只受 [`PackLimits::max_entry_bytes`] 约束。
pub(super) fn read_capped(
    reader: &mut impl Read,
    cap: u64,
    budget: Option<&ReadBudget>,
    name: &str,
) -> PackResult<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| PackError::io(format!("读 {name} 失败：{error}")))?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > cap {
            return Err(PackError::limits(format!(
                "{name} 实际解压超过单条目上限 {cap}"
            )));
        }
        if let Some(budget) = budget {
            budget.spend(read as u64, name)?;
        }
        bytes.extend_from_slice(&buffer[..read]);
    }
    Ok(bytes)
}
