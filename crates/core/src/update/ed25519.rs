//! Ed25519 **严格验签**（`ed25519-dalek::verify_strict`），只验不签、只用公开数据。
//!
//! # 为什么是官方实现，而不是本仓自写
//!
//! M190 第一版为了「scope 里不含 `crates/core/Cargo.toml`」把曲线算术写在了仓里
//! （5x51 素域 + 扩展坐标 + 平方-乘）。r1 复审的裁定是**不允许**：这是**信任锚**，
//! 非规范编码、小阶点这类微妙错误的后果是「验签通过但本不该通过」，自写的向量覆盖再全也证明不了
//! 「严格性判据选对了」。当时的时机也最好：正式信任表（`trust::PRODUCTION`）还是空表，换实现
//! **零兼容负担**。于是这里只剩一层薄封装。
//!
//! # 判据分工（**实测口径**，`ed25519-dalek 2.2.0`；别照抄「常识」）
//!
//! | 步骤 | 它实际做什么 | 它**不**做什么 |
//! |---|---|---|
//! | `VerifyingKey::from_bytes` | 解压成曲线上的合法点；解不出来才 `Err` | **不**校验编码规范性：`y >= p` 这种第二种写法照样 `Ok`（此时 `is_weak() == true`）。M190 r2 复审实测并纠正了先前「要求规范编码」的说法 |
//! | `VerifyingKey::verify`（**非 strict**，本仓不用） | 验方程 `[S]B = R + [k]A`，并检查 `S` 是**规范标量**（`S >= L` 会拒） | **不**检查小阶点：小阶公钥 / 小阶 `R` 下「自洽的伪造四元组」能验过 |
//! | `VerifyingKey::verify_strict`（**本仓用的这个**） | 上面全部 + 拒**小阶公钥**与**小阶 `R`** | —— |
//!
//! 换成大白话：在这批拒绝类用例里，strict 与**非** strict 唯一的分歧是**小阶点**（不是可锻性——
//! `verify` 自己就查 `S` 规范性）。所以守卫里真正承重的是
//! `tests/ed25519_cases.rs::small_order_public_key_is_rejected` 与
//! `small_order_r_is_rejected_only_by_strict` 这两条；`malleable_s_plus_l_is_rejected` 等其余几条在
//! 非 strict 下**照样绿**，它们钉的是「判据没被删成恒真」，不要误当成 strict 的守卫。
//!
//! # 签名侧不在二进制里
//!
//! 本仓没有任何私钥代码：清单由 `packaging/sign_manifest.py`（纯 Python 参考实现，零第三方依赖）
//! 在发布侧签。测试里的签名夹具走 dalek 的 `SigningKey`，只在 `cfg(test)` 里编译。

use ed25519_dalek::{Signature, VerifyingKey};

/// 公钥字节数。
pub(crate) const PUBLIC_KEY_LEN: usize = 32;
/// 签名字节数。
pub(crate) const SIGNATURE_LEN: usize = 64;

/// 验一条 Ed25519 签名（**严格**语义，见模块文档）。
///
/// `message` 是任意长原文——这里恒为清单文件的**原始字节**（见 [`super::signature`]）。
#[must_use]
pub(crate) fn verify(
    public_key: &[u8; PUBLIC_KEY_LEN],
    message: &[u8],
    signature: &[u8; SIGNATURE_LEN],
) -> bool {
    let Ok(key) = VerifyingKey::from_bytes(public_key) else {
        // 不是曲线上的合法点：直接拒。注意 `from_bytes` **不**管编码规范性（见模块文档的表）。
        return false;
    };
    key.verify_strict(message, &Signature::from_bytes(signature))
        .is_ok()
}
