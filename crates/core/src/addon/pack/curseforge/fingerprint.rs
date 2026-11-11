//! CurseForge 文件指纹（`MurmurHash2-32` + 空白字节过滤），导出侧的「能否被远程替代」判据之一。
//!
//! # 算法（A 级规范缺失，这里是 B 级实现 + 双向验证）
//!
//! ```text
//! data        = 原文件去掉所有 0x09 / 0x0A / 0x0D / 0x20 字节
//! fingerprint = MurmurHash2_32(data, seed = 1, len = data 的长度)
//! ```
//!
//! 三条容易写错的点，逐条钉住：
//!
//! 1. **长度用过滤后的长度**参与初始态（`h = seed ^ len`），不是原文件长度；
//! 2. **seed 恒为 1**（MurmurHash2 的默认 seed 是 `0x9747b28c`，那是别的调用方的约定）；
//! 3. 过滤是**逐字节**的，只在 0x09/0x0A/0x0D/0x20 上生效——不是"去空白"（`0x0B` 竖tab、
//!    `0x0C` 换页都保留）。
//!
//! # 来源与验证
//!
//! CurseForge 官方只公开 `POST /v1/fingerprints/432` 端点，**没有公开算法**（A 级规范不存在）。
//! 这里的实现对齐 B 级参考 `HMCL CurseForgeRemoteAddonRepository.calculateFingerprint`
//! 与 `MurmurHash2.hash32(byte[], int, int)`（SMHasher `MurmurHash2.cpp` 的忠实转写）。
//!
//! **双向验证**（`tests` 模块）：一组固定输入的金标准值由一份**独立的 Java 实现**（HMCL 两份
//! 源码的逐行移植，不在本仓库内）算出后写死在这里；Rust 侧同时跑「一次性字节序」与「流式文件」
//! 两条路径，两条都要与金标准相等。所以这一层是"两套实现 + 两个入口"的四向对齐，而不是自证。
//! 仍未验的部分：**真实 CF 文件**的官方指纹（要网络 + `x-api-key`，本任务环境离线）。
//!
//! # 哨兵值 `811513880`
//!
//! 这个值会误命中（HMCL issue #4597 的临时规避）：无论文件是什么，只要算出这个数就不能当成
//! "已找到"。判据写成 [`is_sentinel`]，由调用方（导出管线）在查表前短路。

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

/// 误命中的哨兵指纹（HMCL issue #4597）。命中时按「查不到」处理。
pub const SENTINEL: u32 = 811_513_880;

/// 被过滤掉的四个字节：水平制表、换行、回车、空格。
pub const FILTERED: [u8; 4] = [0x09, 0x0a, 0x0d, 0x20];

/// 指纹查表时是否该按「没找到」处理。
#[must_use]
pub const fn is_sentinel(fingerprint: u32) -> bool {
    fingerprint == SENTINEL
}

/// 这个字节参与计算吗（`0x09` / `0x0A` / `0x0D` / `0x20` 不参与）。
#[must_use]
pub const fn keeps(byte: u8) -> bool {
    !matches!(byte, 0x09 | 0x0a | 0x0d | 0x20)
}

/// 对**内存里的一份内容**算指纹（过滤 + 一次性哈希）。
///
/// 小文件与测试用；大文件请用 [`fingerprint_file`]（两遍流式读，不把整份读进内存）。
#[must_use]
pub fn fingerprint_bytes(bytes: &[u8]) -> u32 {
    let filtered: Vec<u8> = bytes.iter().copied().filter(|byte| keeps(*byte)).collect();
    murmur2(&filtered)
}

/// 对一个**文件**算指纹：两遍流式读（先数过滤后长度，再喂哈希）。
///
/// MurmurHash2 的初始态要事先知道总长度，所以必须两遍；这里不把文件读进内存（mod jar 可以
/// 有几百 MB，整包导出要对每个文件算一次）。
pub fn fingerprint_file(path: &Path) -> io::Result<u32> {
    let mut filtered_length = 0_u64;
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut file = File::open(path)?;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        filtered_length += buffer[..read].iter().filter(|byte| keeps(**byte)).count() as u64;
    }

    // 长度参数是 u32（MurmurHash2-32 的初始态），超了要报错而不是截断。
    let length = u32::try_from(filtered_length).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("文件过滤后仍有 {filtered_length} 字节，超出指纹长度参数（u32）上限"),
        )
    })?;
    let mut hasher = Stream::new(length, 1);
    file = File::open(path)?;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finish())
}

/// MurmurHash2 32 位，seed 固定 1（CurseForge 的约定）。
///
/// 实现是 SMHasher `MurmurHash2.cpp` 的形状：按 4 字节小端块混合，尾部 1–3 字节按小端补齐后
/// 再乘一次；收尾两次移位混合。**不在这里做字节过滤**（调用方先过滤，见 [`fingerprint_bytes`]）。
#[must_use]
pub fn murmur2(data: &[u8]) -> u32 {
    const M: u32 = 0x5bd1_e995;
    const R: u32 = 24;

    let mut h = 1_u32 ^ (data.len() as u32);
    let blocks = data.len() / 4;
    for index in 0..blocks {
        let mut k = u32::from_le_bytes([
            data[index * 4],
            data[index * 4 + 1],
            data[index * 4 + 2],
            data[index * 4 + 3],
        ]);
        k = k.wrapping_mul(M);
        k ^= k >> R;
        k = k.wrapping_mul(M);
        h = h.wrapping_mul(M);
        h ^= k;
    }
    let tail = &data[blocks * 4..];
    if !tail.is_empty() {
        let mut packed = 0_u32;
        for (shift, byte) in tail.iter().enumerate() {
            packed |= u32::from(*byte) << (8 * shift);
        }
        h ^= packed;
        h = h.wrapping_mul(M);
    }
    h ^= h >> 13;
    h = h.wrapping_mul(M);
    h ^= h >> 15;
    h
}

/// 流式 MurmurHash2：长度先知道，字节可以分任意多段喂进来。
struct Stream {
    hash: u32,
    tail: u32,
    tail_len: u32,
}

impl Stream {
    const fn new(length: u32, seed: u32) -> Self {
        Self {
            hash: seed ^ length,
            tail: 0,
            tail_len: 0,
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        const M: u32 = 0x5bd1_e995;
        const R: u32 = 24;
        for byte in bytes {
            if !keeps(*byte) {
                continue;
            }
            self.tail |= u32::from(*byte) << (8 * self.tail_len);
            self.tail_len += 1;
            if self.tail_len == 4 {
                let mut k = self.tail;
                k = k.wrapping_mul(M);
                k ^= k >> R;
                k = k.wrapping_mul(M);
                self.hash = self.hash.wrapping_mul(M);
                self.hash ^= k;
                self.tail = 0;
                self.tail_len = 0;
            }
        }
    }

    fn finish(mut self) -> u32 {
        const M: u32 = 0x5bd1_e995;
        if self.tail_len != 0 {
            self.hash ^= self.tail;
            self.hash = self.hash.wrapping_mul(M);
        }
        self.hash ^= self.hash >> 13;
        self.hash = self.hash.wrapping_mul(M);
        self.hash ^= self.hash >> 15;
        self.hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 金标准值由一份独立的 Java 实现（HMCL `MurmurHash2.hash32` + `calculateFingerprint` 的
    /// 逐行移植）对同一批输入算出。改了算法这两列会同时红——这就是"双向验证"的用处。
    const GOLDEN: &[(&str, &[u8], u32)] = &[
        ("空文件", b"", 1_540_447_798),
        ("只有空白", b" \t\r\n \t\r\n", 1_540_447_798),
        (
            "夹具 jar 存根",
            b"PK\x03\x04 hand-made jar stub",
            489_715_614,
        ),
        ("混入空白", b"\x00\t\n\r AB\tC\n\r \x7f\xff", 3_734_724_774),
        (
            "ascii 36",
            b"abcdefghijklmnopqrstuvwxyz0123456789",
            3_026_742_571,
        ),
    ];

    fn pattern(length: usize) -> Vec<u8> {
        (0..length).map(|index| (index & 0xff) as u8).collect()
    }

    #[test]
    fn matches_the_independent_implementation() {
        for (label, bytes, expected) in GOLDEN {
            assert_eq!(fingerprint_bytes(bytes), *expected, "{label}（一次性路径）");
        }
        assert_eq!(
            fingerprint_bytes(&pattern(512)),
            3_451_065_282,
            "pattern-512"
        );
        assert_eq!(fingerprint_bytes(&pattern(5)), 813_985_283, "pattern-5");
        assert_eq!(fingerprint_bytes(&pattern(7)), 2_174_351_703, "pattern-7");
    }

    #[test]
    fn filtered_length_enters_the_initial_state() {
        // 原文件长度 8、过滤后长度 0：与空文件同值。用原长度当 len 会得到另一个数（这里不是）。
        assert_eq!(fingerprint_bytes(b" \t\r\n \t\r\n"), fingerprint_bytes(b""),);
        assert_ne!(fingerprint_bytes(b"aaaaaaaa"), fingerprint_bytes(b"aaaa"),);
    }

    #[test]
    fn only_the_four_documented_bytes_are_dropped() {
        assert!(!keeps(0x09) && !keeps(0x0a) && !keeps(0x0d) && !keeps(0x20));
        // 竖 tab、换页、NUL 都保留：过滤是四个字节，不是"去空白"。
        assert!(keeps(0x0b) && keeps(0x0c) && keeps(0x00));
    }

    #[test]
    fn streaming_and_one_shot_agree_at_every_split() {
        let data = pattern(64);
        let expected = fingerprint_bytes(&data);
        for split in 0..=data.len() {
            let mut hasher =
                Stream::new(data.iter().filter(|byte| keeps(**byte)).count() as u32, 1);
            hasher.update(&data[..split]);
            hasher.update(&data[split..]);
            assert_eq!(hasher.finish(), expected, "split={split}");
        }
    }

    #[test]
    fn sentinel_is_treated_as_missing() {
        assert!(is_sentinel(SENTINEL));
        assert!(!is_sentinel(0));
        assert!(!is_sentinel(SENTINEL + 1));
    }
}
