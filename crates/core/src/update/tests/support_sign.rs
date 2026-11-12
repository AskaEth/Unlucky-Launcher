//! 清单签名的**测试夹具**：用测试钥签出 `manifest.json.sig` 的信封（M190）。
//!
//! 为什么单独一个文件：`support.rs` 是 update 测试的通用夹具（临时目录、假 HTTP、清单片段），
//! 已经贴着 400 行红线；签名这一摊只被 `signature_cases` / `github_cases` 用到，搬出来两清。
//!
//! 签名走 `ed25519-dalek` 的 `SigningKey`（与仓里验签用的 `verify_strict` 同一棵树），而**不是**
//! 自写实现——M190 r1 复审把自写曲线算术整体删掉了，这里也不留第二条算法。测试钥的种子是公开的
//! （见下），它只对 `cfg(test)` / `update-test-source` / `update-test-key` 三档构建有意义。

use super::support::TempDir;

/// 测试签名钥的 32 字节种子。
///
/// **它是公开的，故意不保密**：它只对 `cfg(test)` / `update-test-source` / `update-test-key` 三档
/// 构建有意义（`trust::TEST_ONLY` 才认这把钥），正式构建的信任表里没有它，所以拿它签不出任何正式
/// 版本能装的清单。正式私钥只能来自仓库 Secret（见 `packaging/docs/signing.md`）。
pub(crate) const TEST_SIGNING_SEED: &[u8; 32] = b"ull-m190-test-key-seed-000000001";

/// 测试钥的 `key_id`（与 `trust::TEST_ONLY` 里那条一致）。
pub(crate) const TEST_KEY_ID: &str = "test-only-key";

/// 字节转小写十六进制。
pub(crate) fn hex_of(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 用测试钥签一份清单，返回 `manifest.json.sig` 的信封原文。
pub(crate) fn signature_envelope(manifest_text: &str) -> String {
    signature_envelope_with(TEST_SIGNING_SEED, TEST_KEY_ID, manifest_text)
}

/// 用指定种子/`key_id` 签一份清单（负向用例要造「不认识的钥」「改过内容的签名」）。
pub(crate) fn signature_envelope_with(
    seed: &[u8; 32],
    key_id: &str,
    manifest_text: &str,
) -> String {
    signature_envelope_of(&[signature_entry(seed, key_id, manifest_text)])
}

/// 多签名信封（轮换窗口的夹具：一份清单同时带几把钥的签名）。
pub(crate) fn signature_envelope_of(entries: &[String]) -> String {
    format!(r#"{{"format":1,"signatures":[{}]}}"#, entries.join(","))
}

/// 一条签名条目（可以被 [`signature_envelope_of`] 拼起来，也可以单独改坏再拼）。
pub(crate) fn signature_entry(seed: &[u8; 32], key_id: &str, manifest_text: &str) -> String {
    signature_entry_from_bytes(
        key_id,
        &sign_bytes(seed, manifest_text.as_bytes()),
        manifest_text,
    )
}

/// 用**现成的签名字节**拼一条条目（「只改签名字节」的用例靠它）。
pub(crate) fn signature_entry_from_bytes(
    key_id: &str,
    signature: &[u8; 64],
    manifest_text: &str,
) -> String {
    let sha = crate::update::signature::hex_sha256(manifest_text.as_bytes());
    format!(
        r#"{{"alg":"ed25519","key_id":"{key_id}","sig":"{}","manifest_sha256":"{sha}"}}"#,
        hex_of(signature)
    )
}

/// 用 32 字节种子签一段字节（dalek 的 `SigningKey`；测试专用，正式构建里没有签名代码）。
pub(crate) fn sign_bytes(seed: &[u8; 32], message: &[u8]) -> [u8; 64] {
    use ed25519_dalek::{Signer, SigningKey};
    let key = SigningKey::from_bytes(seed);
    key.sign(message).to_bytes()
}

/// 种子对应的公钥（用来让「钉住的公钥」与「签名的钥」对得上——漂了就当场红）。
pub(crate) fn public_key_hex(seed: &[u8; 32]) -> String {
    use ed25519_dalek::SigningKey;
    hex_of(SigningKey::from_bytes(seed).verifying_key().as_bytes())
}

impl TempDir {
    /// 写一份清单**连它的签名**（`<name>` + `<name>.sig`），返回清单路径。
    ///
    /// M190 起「有清单没签名」是被拒的一档，所以夹具一律成对落盘：只写清单的用例都是为了
    /// 证明「缺签名会被拒」，它们自己显式 `write` 清单即可。
    pub(crate) fn write_signed(&self, name: &str, manifest_text: &str) -> std::path::PathBuf {
        let path = self.write(name, manifest_text.as_bytes());
        self.write(
            &format!("{name}.sig"),
            signature_envelope(manifest_text).as_bytes(),
        );
        path
    }
}
