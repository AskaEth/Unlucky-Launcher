//! 离线 UUID 与 **JDK 对拍**（这就是「HMCL 兼容」的定义）。
//!
//! 期望值不是我们算出来再抄回来的，而是用 JDK 25 跑
//! `UUID.nameUUIDFromBytes(("OfflinePlayer:" + name).getBytes(UTF_8))`
//! 生成的（驱动源码见下方注释里的等价 Java）。HMCL 的
//! `OfflineAccountFactory.getUUIDFromUserName` 就是这一行，因此对拍 JDK 即对拍 HMCL。
//!
//! ```java
//! UUID id = UUID.nameUUIDFromBytes(("OfflinePlayer:" + name).getBytes(StandardCharsets.UTF_8));
//! System.out.println(id.toString().replace("-", ""));
//! ```

use unlucky_core::auth::offline::{offline_login, offline_meta, offline_uuid, offline_uuid_dashed};
use unlucky_core::auth::AccountKind;

/// `(名字, JDK 输出 32 位无连字符)`。
const JDK_VECTORS: [(&str, &str); 7] = [
    ("Notch", "b50ad385829d3141a2167e7d7539ba7f"),
    ("jeb_", "a762f5604fce3236812ab80efff0b62b"),
    ("Player", "a01e3843e5213998958af459800e4d11"),
    ("Steve", "5627dd98e6be3c21b8a8e92344183641"),
    // 非 ASCII：UTF-8 字节参与 MD5（中文名离线服常见）。
    ("小明", "e1fa999fd9e93419bc012f51157a00e5"),
    ("name with space", "46c02d33ff53313b813aa95508518305"),
    ("AskaEth", "5c5918fb61303d6eb8d8eb2d1da6e439"),
];

#[test]
fn matches_jdk_name_uuid_from_bytes() {
    for (name, expected) in JDK_VECTORS {
        assert_eq!(
            offline_uuid(name),
            expected,
            "离线 UUID 与 JDK/HMCL 不一致：{name}"
        );
    }
}

#[test]
fn dashed_form_is_the_same_uuid() {
    for (name, expected) in JDK_VECTORS {
        let dashed = offline_uuid_dashed(name);
        assert_eq!(dashed.replace('-', ""), expected, "{name} 的连字符形式");
        assert_eq!(dashed.len(), 36);
        assert_eq!(dashed.matches('-').count(), 4);
    }
}

#[test]
fn version_is_3_and_variant_is_ietf() {
    for (name, raw) in JDK_VECTORS {
        let version_nibble = &raw[12..13];
        assert_eq!(version_nibble, "3", "{name} 必须是 v3");
        let variant = u8::from_str_radix(&raw[16..17], 16).expect("变体位");
        assert!((0x8..=0xb).contains(&variant), "{name} 变体必须是 10xx");
    }
}

#[test]
fn uuid_depends_on_exact_bytes_not_case() {
    // 大小写不同 → 不同身份（服务端就是按字节算的，别做「友好」归一化）。
    assert_ne!(offline_uuid("Steve"), offline_uuid("steve"));
    // 前缀也参与哈希：直接哈希名字会得到另一个 UUID。
    assert_ne!(offline_uuid("Notch"), offline_uuid("OfflinePlayer:Notch"));
}

#[test]
fn offline_login_matches_hmcl_behaviour() {
    let meta = offline_meta("小明", 1).expect("建元数据");
    assert_eq!(meta.kind, AccountKind::Offline);
    assert_eq!(meta.profile_id, "e1fa999fd9e93419bc012f51157a00e5");
    assert_eq!(meta.profile_name, "小明");

    let placeholders = offline_login(&meta).expect("占位符");
    // uuid 必须是**无连字符** 32 位（HMCL 的 UUIDs.toCompactString）。
    assert_eq!(placeholders.uuid, meta.profile_id);
    assert_eq!(placeholders.uuid.len(), 32);
    assert!(!placeholders.uuid.contains('-'));
    // accessToken = 随机 32 位十六进制（每次不同、非空：游戏不接受空 token）。
    assert_eq!(placeholders.access_token.len(), 32);
    assert!(placeholders
        .access_token
        .chars()
        .all(|ch| ch.is_ascii_hexdigit()));
    assert_ne!(
        placeholders.access_token,
        offline_login(&meta).expect("再来一次").access_token
    );
    // userType=msa、userProperties={}（承 OfflineAccount.logInWithoutSkin）。
    assert_eq!(placeholders.user_type, "msa");
    assert_eq!(placeholders.user_properties, "{}");
}

#[test]
fn names_are_validated_before_hashing() {
    assert!(offline_meta("   ", 1).is_err());
    assert!(offline_meta("0123456789abcdefg", 1).is_err());
    // 16 字符刚好合法。
    assert!(offline_meta("0123456789abcdef", 1).is_ok());
    // 首尾空白被去掉后哈希（避免「名字带空格」算出两套 UUID）。
    assert_eq!(
        offline_meta("  Steve  ", 1).expect("合法").profile_id,
        offline_uuid("Steve")
    );
}
