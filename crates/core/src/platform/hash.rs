//! 迁移收据的源文件指纹（SHA-256），委托给 `sha2`。
//!
//! 出处：`DevDocs/analyse/07-settings-packaging-update.md` §1.8——收据里存源文件 `sourceSHA256`，
//! 这样「删掉新文件但旧文件没变」时不会重复迁移，旧文件变了才重放。
//!
//! 阶段A 在「不许动 Cargo.toml」的约束下自带过一版 FIPS 180-4 实现；阶段B 按 tower 批复
//! 改用 `sha2`（安全哈希不自维护），**公开签名不变**，单测从「自带实现的标准向量」
//! 变为「wrapper 与 `sha2` 一致 + 标准向量兜底」。

use std::io;
use std::path::Path;

use sha2::{Digest, Sha256};

/// 计算 32 字节摘要。
#[must_use]
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(data);
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_slice());
    out
}

/// 计算小写十六进制摘要。
#[must_use]
pub fn sha256_hex(data: &[u8]) -> String {
    to_hex(&sha256(data))
}

/// 读文件算十六进制摘要（配置文件很小，整读即可）。
///
/// # Errors
/// 文件读失败时返回原始 `io::Error`。
pub fn sha256_hex_file(path: &Path) -> io::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(sha256_hex(&bytes))
}

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}

#[cfg(test)]
mod tests {
    use sha2::Digest;

    use super::{sha256, sha256_hex, sha256_hex_file};

    #[test]
    fn matches_fips_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn agrees_with_sha2_at_block_boundaries() {
        // 55 / 56 / 63 / 64 / 65 是补齐逻辑最容易出错的长度，wrapper 必须与 sha2 完全一致。
        for len in [55usize, 56, 63, 64, 65, 128, 1000] {
            let data = vec![b'a'; len];
            let expected = sha2::Sha256::digest(&data);
            assert_eq!(&sha256(&data)[..], expected.as_slice(), "length {len}");
            assert_eq!(sha256_hex(&data).len(), 64);
        }
    }

    #[test]
    fn hashes_file_contents() {
        let dir = std::env::temp_dir().join(format!("ull-hash-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("a.txt");
        std::fs::write(&path, b"abc").expect("write");
        let hex = sha256_hex_file(&path).expect("hash file");
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(
            hex,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
