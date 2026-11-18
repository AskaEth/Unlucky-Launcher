//! 清单签名信封：`manifest.json.sig` 的解析与验签。
//!
//! # 为什么是**分离**的签名文件，而不是清单里的一个 `signatures` 字段
//!
//! 签名只能覆盖「签的时候那份字节」。把签名塞进清单本身，验签就得先把那个字段从 JSON 里摘出来
//! 再规范化（空白、键序、数字写法全都得钉死），任何一处规范化差异都会让「同一份内容」有两种字节，
//! 于是「改字节不改语义」的老把戏就回来了。分离文件把这件事变简单：**签名覆盖清单文件的原始字节**，
//! 一个 bit 都不能变。顺序也就固定了：先验签，再解析。
//!
//! ```json
//! {"format": 1,
//!  "signatures": [
//!    {"alg": "ed25519", "key_id": "ull-release-2026a", "sig": "<128 位十六进制>",
//!     "manifest_sha256": "<64 位十六进制>"}
//!  ]}
//! ```
//!
//! `manifest_sha256` 是冗余但**有意**的：它让「清单变了而签名没跟着换」这种最常见的发布事故
//! 给出**具体**原因（签名声明的是哪一份、手上是哪一份），而不是一句「验签失败」。它不参与判据
//! ——判据永远是 Ed25519 的那条方程。
//!
//! # 多签名
//!
//! 数组里可以有任意多条，**任意一条**由钉死的钥验过就放行。这给了「新旧钥并存」的窗口期：
//! 轮换期一份清单同时带旧钥与新钥签名，装着旧二进制的用户与装着新二进制的用户都过得去。
//!
//! # 判据顺序（每条都能单独变红）
//!
//! 1. 信封存在且能解析（`format` 必须是 1）；
//! 2. 本构建钉了至少一把公钥（[`super::trust::keyring`] 非空）——空表一律拒，绝不「没钥就放行」；
//! 3. 至少一条签名的 `key_id` 命中钉住的表，且它的 `alg` 是 `ed25519`；
//! 4. 那条签名对**清单原始字节**验过；
//! 5. 若签名带 `manifest_sha256`，与清单字节的 SHA-256 相符（只为给出可读原因，不替代第 4 条）。

use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::ed25519::{verify, PUBLIC_KEY_LEN, SIGNATURE_LEN};
use super::error::{UpdateError, UpdateErrorKind};
use super::trust;

/// 信封格式版本（改字段形状要抬它，老客户端会以「格式不支持」明确拒绝，而不是静默不认签名）。
pub(crate) const ENVELOPE_FORMAT: u32 = 1;

/// 验收通过的签名摘要（进日志/CLI 输出：谁签的、用哪把钥验过的）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedSignature {
    /// 验过的 `key_id`。
    pub key_id: String,
    /// 算法标识（当前只有 `ed25519`）。
    pub alg: String,
}

/// 清单的签名信封。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Envelope {
    format: u32,
    signatures: Vec<EnvelopeSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EnvelopeSignature {
    alg: String,
    key_id: String,
    sig: String,
    manifest_sha256: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEnvelope {
    format: u32,
    #[serde(default)]
    signatures: Vec<RawEnvelopeSignature>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEnvelopeSignature {
    alg: String,
    key_id: String,
    sig: String,
    manifest_sha256: Option<String>,
}

/// 验一份清单的签名。
///
/// # Errors
///
/// * [`UpdateErrorKind::SignatureMissing`]：源里没有配套的签名文件；
/// * [`UpdateErrorKind::SignatureMalformed`]：信封不是合法 JSON / 字段名拼错 / `format` 不认识；
/// * [`UpdateErrorKind::KeyringEmpty`]：本构建一把公钥都没钉（正式表还没录钥）；
/// * [`UpdateErrorKind::SignatureUnknownKey`]：所有签名的 `key_id` 都不在钉住的表里；
/// * [`UpdateErrorKind::SignatureInvalid`]：命中了钥但对清单字节验不过（清单被改过、签名不是给这份的）。
pub(crate) fn verify_manifest(
    manifest_bytes: &[u8],
    envelope_text: Option<&str>,
) -> Result<VerifiedSignature, UpdateError> {
    let Some(text) = envelope_text else {
        return Err(UpdateError::new(
            UpdateErrorKind::SignatureMissing,
            "清单没有配套签名（清单旁边缺 `<清单名>.sig`）：本启动器只安装「清单签名对得上」的更新，\
             没有签名一律拒装——这不是你的问题，是发布侧漏了签名",
        ));
    };
    let envelope = parse_envelope(text)?;
    let keys = trust::keyring();
    if keys.is_empty() {
        return Err(UpdateError::new(
            UpdateErrorKind::KeyringEmpty,
            "本构建没有钉任何清单签名公钥（正式表为空），因此任何清单都无法验签：\
             请勿绕过，这是「还没录钥」的构建，不是清单有问题",
        ));
    }
    let actual_sha256 = hex_sha256(manifest_bytes);
    let mut unknown = Vec::new();
    let mut attempted = Vec::new();
    for signature in &envelope.signatures {
        if signature.alg != "ed25519" {
            unknown.push(format!("{}({})", signature.key_id, signature.alg));
            continue;
        }
        let Some(pinned) = keys.iter().find(|key| key.key_id == signature.key_id) else {
            unknown.push(format!("{}（未钉住）", signature.key_id));
            continue;
        };
        let Some(public_key) = trust::decode_hex(pinned.public_key_hex) else {
            return Err(UpdateError::invalid_manifest(format!(
                "钉住的公钥 `{}` 不是合法十六进制（这是构建缺陷，不是清单的问题）",
                pinned.key_id
            )));
        };
        if public_key.len() != PUBLIC_KEY_LEN {
            return Err(UpdateError::invalid_manifest(format!(
                "钉住的公钥 `{}` 长 {} 字节，Ed25519 公钥必须是 {PUBLIC_KEY_LEN} 字节（构建缺陷）",
                pinned.key_id,
                public_key.len()
            )));
        }
        let mut key_bytes = [0_u8; PUBLIC_KEY_LEN];
        key_bytes.copy_from_slice(&public_key);
        let Some(signature_bytes) = trust::decode_hex(&signature.sig) else {
            attempted.push(format!("{}（`sig` 不是十六进制）", signature.key_id));
            continue;
        };
        if signature_bytes.len() != SIGNATURE_LEN {
            attempted.push(format!(
                "{}（`sig` 长 {} 字节，应为 {SIGNATURE_LEN}）",
                signature.key_id,
                signature_bytes.len()
            ));
            continue;
        }
        let mut sig_bytes = [0_u8; SIGNATURE_LEN];
        sig_bytes.copy_from_slice(&signature_bytes);
        if !verify(&key_bytes, manifest_bytes, &sig_bytes) {
            attempted.push(signature.key_id.clone());
            continue;
        }
        // 第 5 条：只为「把话说清楚」，不替代上面那条方程。
        if let Some(declared) = &signature.manifest_sha256 {
            if declared.to_ascii_lowercase() != actual_sha256 {
                return Err(UpdateError::new(
                    UpdateErrorKind::SignatureInvalid,
                    format!(
                        "清单签名声明的是另一份清单（签名里的 sha256 = {}，手上这份 = {}）：\
                         签名对上了、内容不是它签的那份",
                        short(declared),
                        short(&actual_sha256)
                    ),
                ));
            }
        }
        return Ok(VerifiedSignature {
            key_id: signature.key_id.clone(),
            alg: signature.alg.clone(),
        });
    }
    if attempted.is_empty() {
        return Err(UpdateError::new(
            UpdateErrorKind::SignatureUnknownKey,
            format!(
                "清单由本构建不认识的钥签名（{}）：本构建钉住的 key_id = [{}]。\
                 要么清单是别人签的，要么发布侧用了还没进信任表的新钥",
                unknown.join("、"),
                keys.iter()
                    .map(|key| key.key_id)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }
    Err(UpdateError::new(
        UpdateErrorKind::SignatureInvalid,
        format!(
            "清单签名验不过（key_id = {}，清单 sha256 = {}）：这份清单不是那把钥签的，\
             或签名之后清单被改过。绝不安装",
            attempted.join("、"),
            short(&actual_sha256)
        ),
    ))
}

/// 解析信封（`deny_unknown_fields`：字段名拼错宁可在 CI/发布侧当场炸，也别在用户机器上变成「验签失败」）。
fn parse_envelope(text: &str) -> Result<Envelope, UpdateError> {
    let raw: RawEnvelope = serde_json::from_str(text).map_err(|error| {
        UpdateError::new(
            UpdateErrorKind::SignatureMalformed,
            format!("清单签名文件不是合法 JSON（{error}）"),
        )
    })?;
    if raw.format != ENVELOPE_FORMAT {
        return Err(UpdateError::new(
            UpdateErrorKind::SignatureMalformed,
            format!(
                "清单签名格式 {} 不认识（本构建只认 {ENVELOPE_FORMAT}）：请升级启动器",
                raw.format
            ),
        ));
    }
    if raw.signatures.is_empty() {
        return Err(UpdateError::new(
            UpdateErrorKind::SignatureMalformed,
            "清单签名文件里 `signatures` 是空的：签了个空",
        ));
    }
    Ok(Envelope {
        format: raw.format,
        signatures: raw
            .signatures
            .into_iter()
            .map(|item| EnvelopeSignature {
                alg: item.alg,
                key_id: item.key_id,
                sig: item.sig,
                manifest_sha256: item.manifest_sha256,
            })
            .collect(),
    })
}

/// 清单字节的 SHA-256（小写十六进制）。
pub(crate) fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// 日志/文案里只露前 8 位。
fn short(hex: &str) -> String {
    let trimmed: String = hex.trim().chars().take(8).collect();
    format!("{trimmed}…")
}
