//! 流式摘要：边下边算，不额外读一遍盘。
//!
//! 承 analyse/03 §5.6 的「先校验后落盘」：`MessageDigest` 在写流的同时喂数据，
//! 流结束时比对，不符就丢掉中间文件。

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use sha1::Digest;

use super::types::HashAlgo;

/// 小写十六进制编码（不引 `hex` 依赖）。
#[must_use]
pub fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}

/// 增量摘要器。
#[derive(Debug)]
pub enum Hasher {
    /// SHA-1。
    Sha1(sha1::Sha1),
    /// SHA-256。
    Sha256(sha2::Sha256),
}

impl Hasher {
    /// 按算法新建。
    #[must_use]
    pub fn new(algo: HashAlgo) -> Self {
        match algo {
            HashAlgo::Sha1 => Self::Sha1(sha1::Sha1::new()),
            HashAlgo::Sha256 => Self::Sha256(sha2::Sha256::new()),
        }
    }

    /// 算法。
    #[must_use]
    pub const fn algo(&self) -> HashAlgo {
        match self {
            Self::Sha1(_) => HashAlgo::Sha1,
            Self::Sha256(_) => HashAlgo::Sha256,
        }
    }

    /// 喂一段数据。
    pub fn update(&mut self, bytes: &[u8]) {
        match self {
            Self::Sha1(hasher) => Digest::update(hasher, bytes),
            Self::Sha256(hasher) => Digest::update(hasher, bytes),
        }
    }

    /// 收尾成小写十六进制。
    #[must_use]
    pub fn finish(self) -> String {
        match self {
            Self::Sha1(hasher) => to_hex(&hasher.finalize()),
            Self::Sha256(hasher) => to_hex(&hasher.finalize()),
        }
    }
}

/// 一次性算内存里的摘要。
#[must_use]
pub fn digest_bytes(algo: HashAlgo, bytes: &[u8]) -> String {
    let mut hasher = Hasher::new(algo);
    hasher.update(bytes);
    hasher.finish()
}

/// 算整个文件的摘要（`cache` 的抽样校验与 `register` 路径用）。
pub fn digest_file(path: &Path, algo: HashAlgo) -> io::Result<String> {
    let mut hasher = Hasher::new(algo);
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut file = File::open(path)?;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finish())
}

/// 把文件前 `limit` 字节喂进已有摘要器（续传时让「边下边算」接得上）。
pub fn update_hasher_from_file(hasher: &mut Hasher, path: &Path, limit: u64) -> io::Result<()> {
    if limit == 0 {
        return Ok(());
    }
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut reader = File::open(path)?.take(limit);
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(())
}

/// 对已有文件的前 `prefix_len` 字节补算摘要（诊断/抽样校验用）。
pub fn digest_prefix(path: &Path, algo: HashAlgo, prefix_len: u64) -> io::Result<String> {
    let mut hasher = Hasher::new(algo);
    update_hasher_from_file(&mut hasher, path, prefix_len)?;
    Ok(hasher.finish())
}

/// 摘要比较（大小写不敏感，避免上游给了大写 hex 就白跑一趟）。
#[must_use]
pub fn matches(expected_hex: &str, actual_hex: &str) -> bool {
    expected_hex.len() == actual_hex.len() && expected_hex.eq_ignore_ascii_case(actual_hex)
}
