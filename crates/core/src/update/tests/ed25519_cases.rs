//! Ed25519 验签判据：**钉住 `ed25519-dalek::verify_strict` 的行为**（RFC 8032 §7.1 向量 + 六条拒绝类）。
//!
//! M190 r1 复审把自写曲线算术整体换成了官方实现（理由见 `crate::update::ed25519` 的模块文档）。
//! 这批用例的定位因此从「测自己的实现」变成「**钉 dalek 的严格语义**」：
//!
//! | 用例 | 它挡住什么 | 把 `verify_strict` 换成 `verify` 后会怎样（r2 实测） |
//! |---|---|---|
//! | `small_order_public_key_is_rejected` | 小阶公钥下「自洽的伪造」（谁都能凑） | **红** —— 这是**唯一**承重的守卫 |
//! | `small_order_r_is_rejected_only_by_strict` | 小阶 `R`（R = 单位元）的同一族伪造 | **红**（其余几条不会） |
//! | `rfc8032_vectors_verify` | 实现换成任何一个错的版本（向量来自 RFC 附录） | 仍绿（`verify` 也验方程） |
//! | `malleable_s_plus_l_is_rejected` | `S >= L` 的可锻变体 | **仍绿**：`verify` 自己就查 `S` 规范性，别把它当 strict 的守卫 |
//! | `non_canonical_r_encoding_is_rejected` | 非规范编码的点（`y >= p` 的第二种写法） | 仍绿（dalek 靠自己的下游判据拒，见该用例注释） |
//! | `empty_signature_is_rejected` / `message_bit_flip` / `signature_bit_flip` | 判据本身失效（恒真） | 仍绿 |
//! | `signing_key_round_trips` | 测试夹具（dalek `SigningKey`）与验签侧同一套语义 | 仍绿 |
//!
//! 口径来源：M190 r2 复审在 `ed25519-dalek 2.2.0` 上逐条实测（并把先前的四处错误说法打回）。
//! **结论**：strict 与**非** strict 在这批输入上唯一的分歧是**小阶点**——所以想确认「strict 还开着」，
//! 看前两条；其余几条只在「判据被删成恒真」时才会红。

use ed25519_dalek::{Signature, Verifier, VerifyingKey};

use crate::update::ed25519::{verify, PUBLIC_KEY_LEN, SIGNATURE_LEN};

use super::support_sign::{hex_of, public_key_hex, sign_bytes};

/// 十六进制转字节（长度不对/字符非法当场 panic：这些向量是写死的常量）。
fn unhex(text: &str) -> Vec<u8> {
    let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        cleaned.len().is_multiple_of(2),
        "十六进制串长度必须是偶数：{text}"
    );
    let bytes = cleaned.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let (pairs, _) = bytes.as_chunks::<2>();
    for pair in pairs {
        let hi = (pair[0] as char).to_digit(16).expect("十六进制字符");
        let lo = (pair[1] as char).to_digit(16).expect("十六进制字符");
        out.push(u8::try_from(hi * 16 + lo).expect("字节"));
    }
    out
}

fn array32(bytes: &[u8]) -> [u8; 32] {
    let mut out = [0_u8; 32];
    out.copy_from_slice(bytes);
    out
}

fn array64(bytes: &[u8]) -> [u8; 64] {
    let mut out = [0_u8; 64];
    out.copy_from_slice(bytes);
    out
}

/// L（群阶）的小端字节，用于构造「S + L」这种可锻变体。
const L_BYTES: [u8; 32] = [
    0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58, 0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde, 0x14,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10,
];

/// RFC 8032 §7.1 的四条向量：(public, message, signature)。
fn rfc_vectors() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        (
            "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
            "",
            "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
        ),
        (
            "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c",
            "72",
            "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00",
        ),
        (
            "fc51cd8e6218a1a38da47ed00230f0580816ed13ba3303ac5deb911548908025",
            "af82",
            "6291d657deec24024827e69c3abe01a30ce548a284743a445e3680d7db5ac3ac18ff9b538d16f290ae67f760984dc6594a7c15e9716ed28dc027beceea1ec40a",
        ),
        (
            "ec172b93ad5e563bf4932c70e1245034c35467ef2efd4d64ebf819683467e2bf",
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f",
            "dc2a4459e7369633a52b1bf277839a00201009a3efbf3ecb69bea2186c26b58909351fc9ac90b3ecfdfbc7c66431e0303dca179c138ac17ad9bef1177331a704",
        ),
    ]
}

#[test]
fn rfc8032_vectors_verify() {
    for (index, (public, message, signature)) in rfc_vectors().into_iter().enumerate() {
        assert!(
            verify(
                &array32(&unhex(public)),
                &unhex(message),
                &array64(&unhex(signature))
            ),
            "RFC 8032 第 {} 条向量必须通过",
            index + 1
        );
    }
}

#[test]
fn message_bit_flip_is_rejected() {
    let (public, message, signature) = rfc_vectors()[1];
    let mut message = unhex(message);
    message[0] ^= 0x01;
    assert!(
        !verify(
            &array32(&unhex(public)),
            &message,
            &array64(&unhex(signature))
        ),
        "改一个消息字节必须被拒"
    );
}

#[test]
fn signature_bit_flip_is_rejected() {
    let (public, message, signature) = rfc_vectors()[2];
    let mut r_flipped = array64(&unhex(signature));
    r_flipped[31] ^= 0x01;
    assert!(
        !verify(&array32(&unhex(public)), &unhex(message), &r_flipped),
        "改 R 必须被拒"
    );
    let mut s_flipped = array64(&unhex(signature));
    s_flipped[63] ^= 0x04;
    assert!(
        !verify(&array32(&unhex(public)), &unhex(message), &s_flipped),
        "改 S 必须被拒"
    );
}

#[test]
fn malleable_s_plus_l_is_rejected() {
    // 可锻性：把 S 换成 S + L，曲线方程照样成立（S 与 S+L 模 L 同余）。RFC 8032 的最低要求允许它。
    // ⚠ 这条**不是** strict 的守卫：dalek 的非 strict `verify` 自己就检查 `S` 规范性，换掉 strict 它
    // 仍绿（r2 实测）。它挡的是「判据被删成恒真」。strict 的守卫见 `small_order_*` 两条。
    let (public, message, signature) = rfc_vectors()[0];
    let mut signature = array64(&unhex(signature));
    let mut carry = 0_u16;
    for index in 0..32 {
        let sum = u16::from(signature[32 + index]) + u16::from(L_BYTES[index]) + carry;
        signature[32 + index] = (sum & 0xFF) as u8;
        carry = sum >> 8;
    }
    assert_eq!(
        carry, 0,
        "S + L 不该溢出 256 位（否则这条用例本身没造出来）"
    );
    assert!(
        !verify(&array32(&unhex(public)), &unhex(message), &signature),
        "S >= L 必须被拒（可锻变体）"
    );
}

#[test]
fn small_order_public_key_is_rejected() {
    // 阶为 1 的点（y = 1, x = 0）与零字节公钥：用它们做公钥，`verify`（非 strict）会接受一批伪造。
    let message = b"manifest bytes";
    let identity_key = {
        let mut key = [0_u8; PUBLIC_KEY_LEN];
        key[0] = 1;
        key
    };
    let mut signature = [0_u8; SIGNATURE_LEN];
    signature[0] = 1; // R = 单位元
    assert!(
        !verify(&identity_key, message, &signature),
        "小阶公钥必须被拒"
    );
    assert!(
        !verify(&[0_u8; PUBLIC_KEY_LEN], message, &signature),
        "零公钥必须被拒"
    );
}

#[test]
fn non_canonical_r_encoding_is_rejected() {
    // R = p（= 2^255-19）的字节：它与 y = 0 同值但编码不同（非规范）。
    // dalek 的实际行为（r2 实测，与先前的说法不同）：
    //   * `CompressedEdwardsY::decompress` **不**校验编码规范性 —— 这串字节解出来是**阶为 4**的点；
    //   * 于是它被 `verify_strict` 的**小阶**判据拒，而不是被「编码非规范」拒；
    //   * 公钥侧同理：`VerifyingKey::from_bytes` 对非规范编码返回 **Ok**（`is_weak() == true`），
    //     所以要拒它只能靠下游判据 —— 下面把 dalek 的这个真实行为**断言下来**，将来 dalek 升级
    //     改了口径会红（否则这条断言对任何实现都绿，等于没测）。
    let (public, message, _) = rfc_vectors()[0];
    let p_bytes: [u8; 32] = [
        0xed, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0x7f,
    ];
    let mut signature = [0_u8; SIGNATURE_LEN];
    signature[..32].copy_from_slice(&p_bytes);
    assert!(
        !verify(&array32(&unhex(public)), &unhex(message), &signature),
        "非规范 R 编码必须被拒（dalek 的理由是小阶点）"
    );

    // dalek 的真实行为：非规范编码的公钥**能**通过 `from_bytes`，并被标成 weak。
    let weak = VerifyingKey::from_bytes(&p_bytes).expect("dalek 2.2 不校验编码规范性，应返回 Ok");
    assert!(
        weak.is_weak(),
        "这个非规范编码解出的是小阶点，dalek 应标记 is_weak"
    );
    // 我们的 `verify` 对同一串公钥仍然拒（下游方程/小阶判据兜住）。
    assert!(
        !verify(
            &p_bytes,
            &unhex(message),
            &array64(&unhex(rfc_vectors()[0].2))
        ),
        "非规范/小阶公钥不得验过"
    );
}

#[test]
fn small_order_r_is_rejected_only_by_strict() {
    // **第二条只由 strict 承重的判据**（r2 复审建议补的）：小阶 `R`。
    // 构造：R = 单位元（阶 1），S = k·a mod L，其中 k = SHA-512(R‖A‖M) mod L、a 是测试钥的标量。
    // 这样方程 `[S]B = R + [k]A` 自洽 ⇒ **非 strict 的 `verify` 会通过**；`verify_strict` 因
    // 「R 是小阶点」拒。下面两边都断言，所以将来谁把 strict 换成非 strict，这条会红。
    let public = array32(&unhex(
        "f3ea49a34ebda73d6500c9416f425e0358baf094b957baf3ea7daa6df63e876b",
    ));
    let message = b"manifest bytes";
    let forged = array64(&unhex(concat!(
        "0100000000000000000000000000000000000000000000000000000000000000",
        "698a94a4c82aa13c1a4da6248799849f2254ef333b5f5d69dcbd71b6320e5e0e"
    )));

    // 非 strict 侧：dalek 的 `verify` 会接受这条伪造（这就是 strict 存在的理由）。
    let key = VerifyingKey::from_bytes(&public).expect("合法公钥");
    assert!(
        key.verify(message, &Signature::from_bytes(&forged)).is_ok(),
        "非 strict 的 verify 应当接受这条伪造（构造前提；若 dalek 改了行为，这条先红）"
    );
    // strict 侧：我们导出的 `verify` 必须拒。
    assert!(
        !verify(&public, message, &forged),
        "小阶 R 的伪造必须被 verify_strict 拒"
    );
}

#[test]
fn empty_signature_is_rejected() {
    let (public, message, _) = rfc_vectors()[0];
    assert!(!verify(
        &array32(&unhex(public)),
        &unhex(message),
        &[0_u8; 64]
    ));
}

#[test]
fn signing_key_round_trips() {
    // 测试夹具（dalek `SigningKey`）与验签侧同一套语义：RFC 的 secret 反推出的公钥要对得上，
    // 签出来的能验过，改一个字节就验不过。
    let seed = array32(&unhex(
        "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
    ));
    assert_eq!(
        public_key_hex(&seed),
        rfc_vectors()[0].0,
        "种子展开必须与 RFC 一致"
    );
    assert_eq!(
        hex_of(&sign_bytes(&seed, b"")),
        rfc_vectors()[0].2,
        "空消息的签名必须逐字节等于 RFC 向量（测试夹具与向量同一套语义）"
    );
    let signature = sign_bytes(&seed, b"unlucky");
    assert!(verify(
        &array32(&unhex(rfc_vectors()[0].0)),
        b"unlucky",
        &signature
    ));
    let mut tampered = signature;
    tampered[0] ^= 0x80;
    assert!(!verify(
        &array32(&unhex(rfc_vectors()[0].0)),
        b"unlucky",
        &tampered
    ));
}
