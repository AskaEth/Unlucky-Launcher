//! 清单签名：**先验签再解析**（M190）。
//!
//! 这一组用例是 mission 守卫里「更新侧」的那几条，每条都能单独变红：
//!
//! | 用例 | 红掉意味着什么 |
//! |---|---|
//! | `tampered_field_is_rejected` | 改清单里任意一个字段仍能装 → 签名判据被绕过 |
//! | `tampered_signature_only_is_rejected` | 只改签名文件、内容不动仍能装 → 「只改签名不换内容」没被拒 |
//! | `missing_signature_is_rejected` | 没有 `.sig` 也照装 → 发布侧漏签名即可绕过整套信任根 |
//! | `unknown_key_is_rejected` | 别人用自己的钥签的清单被认 → 信任锚不是钉死的那把 |
//! | `key_id_spoofing_does_not_help` | 按 `key_id` 名字信任而不是按公钥字节验 → 伪个名字就能签 |
//! | `unsigned_install_never_touches_the_target` | 验签失败却已经动过目标 exe → 判据跑在安装之后（顺序错） |
//! | `no_runtime_keyring_override` | 环境变量/旁边的 keyring 文件能换信任锚 → 钉在二进制里的公钥白钉 |
//!
//! 还有一条「两条判据互不替代」：`sha256_mismatch_is_reported_separately` 证明签名过了、包体摘要
//! 不符时仍是 `DigestMismatch`——签名管「清单是不是我们发的」，sha256 管「包体和清单对不对得上」。

use std::fs;

use crate::update::{
    trust, verified_manifest, Channel, DirSource, Job, OsFamily, Target, UpdateChecker,
    UpdateErrorKind, Version,
};

use super::support::{artifact, manifest, release, TempDir};
use super::support_sign::{
    public_key_hex, sign_bytes, signature_entry, signature_entry_from_bytes, signature_envelope,
    signature_envelope_of, signature_envelope_with, TEST_KEY_ID, TEST_SIGNING_SEED,
};

const PAYLOAD: &[u8] = b"signed release payload";

/// 另一把「攻击者的钥」：种子换一个词，公钥就完全不同。
const ATTACKER_SEED: &[u8; 32] = b"ull-m190-attacker-seed-000000001";

fn target() -> Target {
    Target::new(OsFamily::Windows, crate::update::Arch::X64)
}

fn stable_json(version: &str) -> String {
    manifest(
        "stable",
        &[release(
            version,
            "",
            &[artifact("win-x64", "portable-exe", "asset.bin", PAYLOAD)],
        )],
    )
}

fn current() -> Version {
    Version::parse("0.2.0").expect("版本号合法")
}

#[test]
fn pinned_test_key_matches_the_test_seed() {
    // 信任表里那把测试钥必须就是测试种子推出来的那把。漂了的话「钉住的公钥」与「签名的钥」不是同一个，
    // 下面所有正路用例会一起变红——先在这里单独钉死。
    let derived = public_key_hex(TEST_SIGNING_SEED);
    let pinned = trust::keyring()
        .iter()
        .find(|key| key.key_id == TEST_KEY_ID)
        .expect("测试构建的信任表里必须有测试钥");
    assert_eq!(
        pinned.public_key_hex, derived,
        "钉住的测试公钥与测试种子不匹配"
    );
}

#[test]
fn signed_manifest_is_accepted_and_picks_the_candidate() {
    let dir = TempDir::new("sig-ok");
    dir.write_signed("stable.json", &stable_json("0.3.0"));
    let outcome = UpdateChecker::new(DirSource::new(dir.path()), target())
        .check(&current(), Channel::Stable)
        .expect("签名对得上就不该报错");
    assert_eq!(
        outcome.candidate().map(|c| c.release.version.to_string()),
        Some("0.3.0".to_string())
    );
}

#[test]
fn tampered_field_is_rejected() {
    // 守卫①：清单里改一个字段（版本号多一个数字），签名不跟着换 ⇒ 必须拒。
    let original = stable_json("0.3.0");
    let tampered = stable_json("0.3.1");
    assert_ne!(original, tampered, "两次生成的清单必须真的不同");

    let dir = TempDir::new("sig-tamper-field");
    dir.write("stable.json", tampered.as_bytes());
    // 签名还是**原始**那份的签名。
    dir.write("stable.json.sig", signature_envelope(&original).as_bytes());
    let error = verified_manifest(&DirSource::new(dir.path()), Channel::Stable)
        .expect_err("改过字段的清单必须验不过");
    assert_eq!(error.kind, UpdateErrorKind::SignatureInvalid);
    assert!(
        error.message.contains("sha256"),
        "要说清「签名声明的是哪一份、手上是哪一份」（实际：{}）",
        error.message
    );
}

#[test]
fn tampered_signature_only_is_rejected() {
    // 守卫①的另一半：清单字节一个不动，只把签名的最后一个字节改掉。
    let json = stable_json("0.3.0");
    let mut signature = sign_bytes(TEST_SIGNING_SEED, json.as_bytes());
    let good = signature_envelope(&json);
    signature[63] ^= 0x04;
    let broken =
        signature_envelope_of(&[signature_entry_from_bytes(TEST_KEY_ID, &signature, &json)]);
    assert_ne!(good, broken, "两份信封必须真的不同");

    let dir = TempDir::new("sig-tamper-sig");
    dir.write("stable.json", json.as_bytes());
    dir.write("stable.json.sig", broken.as_bytes());
    let error = verified_manifest(&DirSource::new(dir.path()), Channel::Stable)
        .expect_err("只改签名也必须被拒");
    assert_eq!(error.kind, UpdateErrorKind::SignatureInvalid);
}

#[test]
fn missing_signature_is_rejected() {
    let dir = TempDir::new("sig-missing");
    dir.write("stable.json", stable_json("0.3.0").as_bytes());
    let error = verified_manifest(&DirSource::new(dir.path()), Channel::Stable)
        .expect_err("没有签名文件必须拒");
    assert_eq!(error.kind, UpdateErrorKind::SignatureMissing);
    assert!(!error.is_retryable(), "漏签名不是抖动，重试没有意义");
    assert!(error.kind.is_signature(), "壳层据此换文案");
}

#[test]
fn empty_signature_array_is_malformed() {
    let dir = TempDir::new("sig-empty-array");
    dir.write("stable.json", stable_json("0.3.0").as_bytes());
    dir.write("stable.json.sig", br#"{"format":1,"signatures":[]}"#);
    let error = verified_manifest(&DirSource::new(dir.path()), Channel::Stable)
        .expect_err("空签名数组必须拒");
    assert_eq!(error.kind, UpdateErrorKind::SignatureMalformed);
}

#[test]
fn unknown_field_in_envelope_is_malformed() {
    // `deny_unknown_fields`：发布侧拼错字段名要在 CI/发布侧当场炸，而不是变成用户机器上的「验签失败」。
    let json = stable_json("0.3.0");
    let dir = TempDir::new("sig-unknown-field");
    dir.write("stable.json", json.as_bytes());
    dir.write(
        "stable.json.sig",
        br#"{"format":1,"signatures":[],"comment":"hi"}"#,
    );
    let error = verified_manifest(&DirSource::new(dir.path()), Channel::Stable)
        .expect_err("未知字段必须拒");
    assert_eq!(error.kind, UpdateErrorKind::SignatureMalformed);
}

#[test]
fn unknown_key_is_rejected() {
    // 攻击者用自己那把钥签了一份**完全合法**的清单：字节没问题，只是钥不在钉住的表里。
    let json = stable_json("9.9.9");
    let dir = TempDir::new("sig-unknown-key");
    dir.write("stable.json", json.as_bytes());
    dir.write(
        "stable.json.sig",
        signature_envelope_with(ATTACKER_SEED, "attacker-key", &json).as_bytes(),
    );
    let error = verified_manifest(&DirSource::new(dir.path()), Channel::Stable)
        .expect_err("不认识的钥必须拒");
    assert_eq!(error.kind, UpdateErrorKind::SignatureUnknownKey);
    assert!(
        error.message.contains("attacker-key"),
        "要说出是谁签的（实际：{}）",
        error.message
    );
}

#[test]
fn key_id_spoofing_does_not_help() {
    // 攻击者把自己那把钥的 key_id 伪造成钉住的那个：查表命中，但**公钥字节**对不上 ⇒ 仍拒。
    // 这条就是「公钥钉在二进制里，不是按名字信任」的直接证据。
    let json = stable_json("9.9.9");
    let dir = TempDir::new("sig-key-id-spoof");
    dir.write("stable.json", json.as_bytes());
    dir.write(
        "stable.json.sig",
        signature_envelope_with(ATTACKER_SEED, TEST_KEY_ID, &json).as_bytes(),
    );
    let error = verified_manifest(&DirSource::new(dir.path()), Channel::Stable)
        .expect_err("key_id 撞名不能替代公钥");
    assert_eq!(error.kind, UpdateErrorKind::SignatureInvalid);
}

#[test]
fn multi_signature_accepts_any_pinned_key() {
    // 轮换窗口：一份清单同时带「钉住的旧钥」与「本构建不认识的新钥」的签名，只要有一条能过就放行。
    let json = stable_json("0.3.0");
    let dir = TempDir::new("sig-multi");
    dir.write("stable.json", json.as_bytes());
    let merged = signature_envelope_of(&[
        signature_entry(ATTACKER_SEED, "someone-else", &json),
        signature_entry(TEST_SIGNING_SEED, TEST_KEY_ID, &json),
    ]);
    dir.write("stable.json.sig", merged.as_bytes());
    let manifest =
        verified_manifest(&DirSource::new(dir.path()), Channel::Stable).expect("有一条能过就放行");
    assert!(manifest.is_some());
}

#[test]
fn unsigned_install_never_touches_the_target() {
    // 顺序证据：验签在**取包之前**，因此目标 exe 与暂存目录一个字节都没动过。
    let dir = TempDir::new("sig-apply-order");
    dir.write("stable.json", stable_json("0.3.0").as_bytes());
    dir.write("asset.bin", PAYLOAD);
    let dest = dir.join("bin/unlucky-launcher.exe");
    fs::create_dir_all(dest.parent().expect("父目录")).expect("建目录");
    fs::write(&dest, b"old-bytes").expect("写旧 exe");

    let job = Job::new(
        Box::new(DirSource::new(dir.path())),
        target(),
        dir.join("staging"),
        dest.clone(),
    );
    let error = job
        .apply(&current(), Channel::Stable)
        .expect_err("没有签名不许 apply");
    assert_eq!(error.kind, UpdateErrorKind::SignatureMissing);
    assert_eq!(
        fs::read(&dest).expect("目标仍在"),
        b"old-bytes",
        "目标 exe 不许被动过"
    );
    let staging_empty = match fs::read_dir(dir.join("staging")) {
        Ok(mut entries) => entries.next().is_none(),
        Err(_) => true,
    };
    assert!(staging_empty, "暂存目录不该留下任何东西（验签在取包之前）");
}

#[test]
fn sha256_mismatch_is_reported_separately() {
    // 两条判据互不替代：签名对（内容就是发布侧签的那份），但包体摘要对不上 ⇒ DigestMismatch。
    let dir = TempDir::new("sig-vs-digest");
    dir.write_signed("stable.json", &stable_json("0.3.0"));
    // 等长但不同内容：先过长度判据，才走得到摘要判据（顺序在 verify.rs 里是有意的）。
    dir.write("asset.bin", &[b'X'; PAYLOAD.len()]);
    let job = Job::new(
        Box::new(DirSource::new(dir.path())),
        target(),
        dir.join("staging"),
        dir.join("bin/unlucky-launcher.exe"),
    );
    let error = job
        .apply(&current(), Channel::Stable)
        .expect_err("摘要不符必须拒");
    assert_eq!(error.kind, UpdateErrorKind::DigestMismatch);
    assert!(!error.kind.is_signature(), "这是摘要判据，不是签名判据");
}

#[test]
fn no_runtime_keyring_override() {
    // 守卫②：信任锚是编译期常量。运行期旁边放一份「信任表」、或设环境变量，都不该有任何作用
    // （现在压根没有这种口子；将来谁加了，这条会红）。
    let json = stable_json("9.9.9");
    let dir = TempDir::new("sig-no-override");
    dir.write("stable.json", json.as_bytes());
    dir.write(
        "stable.json.sig",
        signature_envelope_with(ATTACKER_SEED, "attacker-key", &json).as_bytes(),
    );
    let attacker_public = public_key_hex(ATTACKER_SEED);
    dir.write(
        "keyring.json",
        format!(r#"{{"keys":[{{"key_id":"attacker-key","public_key_hex":"{attacker_public}"}}]}}"#)
            .as_bytes(),
    );
    dir.write("attacker.pub", attacker_public.as_bytes());
    let keyring_path = dir.join("keyring.json");
    std::env::set_var("ULL_UPDATE_KEYRING", &keyring_path);
    std::env::set_var("ULL_UPDATE_KEYRING_FILE", &keyring_path);
    let outcome = verified_manifest(&DirSource::new(dir.path()), Channel::Stable);
    std::env::remove_var("ULL_UPDATE_KEYRING");
    std::env::remove_var("ULL_UPDATE_KEYRING_FILE");
    let error = outcome.expect_err("旁边的信任表文件不许生效");
    assert_eq!(
        error.kind,
        UpdateErrorKind::SignatureUnknownKey,
        "改不了信任锚：攻击者的钥仍在表外（实际：{}）",
        error.message
    );
}
