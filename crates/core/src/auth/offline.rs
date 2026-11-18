//! 离线轨：**HMCL / Minecraft 服务端同款离线 UUID** + 启动占位字段。
//!
//! UUID 算法（`analyse/04` §7.1）：`UUID.nameUUIDFromBytes("OfflinePlayer:" + name)`
//! = 对 UTF-8 字节做 MD5，再按 RFC 4122 改写成 **v3** 版本位与 **IETF 变体**位。
//! 必须逐字节一致：离线服务器的 `ops.json` / 白名单 / 领地数据全按这个 UUID 存，
//! 算错就对不上（皮肤、权限、存档全乱）。因此有与 JDK `UUID.nameUUIDFromBytes`
//! 对拍的已知向量测试（见 `crates/core/tests/auth/offline_uuid.rs`）。
//!
//! 启动占位（`analyse/04` §7.2，承 `OfflineAccount.logInWithoutSkin`）：
//! - `accessToken` = **每次登录随机生成的 32 位十六进制**（游戏客户端不接受空 token）；
//! - `userType` = `msa`（HMCL 现版本如此；历史注释提过 `legacy`，但那是旧结论）；
//! - `userProperties` = `{}`；
//! - `uuid` 用**无连字符** 32 位形式（`--uuid` 参数要求）。

use md5::{Digest, Md5};

use crate::auth::account::{AccountKind, AccountMeta};
use crate::auth::error::{AuthError, AuthResult};
use crate::auth::secret::{random_hex32, Secret};
use crate::launch::request::AuthPlaceholders;

/// 离线 UUID 的固定前缀（改一个字节就会与所有服务端对不上）。
pub const OFFLINE_PREFIX: &str = "OfflinePlayer:";
/// 离线账户的 launch 用户类型（`AuthInfo.USER_TYPE_MSA`）。
pub const OFFLINE_USER_TYPE: &str = "msa";
/// 玩家名长度上限（MC 客户端 `--username` 的实际上限）。
pub const MAX_NAME_CHARS: usize = 16;

/// 算离线 UUID（**32 位无连字符小写十六进制**）。
#[must_use]
pub fn offline_uuid(name: &str) -> String {
    let digest = Md5::digest(format!("{OFFLINE_PREFIX}{name}").as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest);

    // RFC 4122 §4.3：版本位 3（MD5 based）、变体位 IETF（10xx）。
    bytes[6] = (bytes[6] & 0x0F) | 0x30;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;

    let mut hex = String::with_capacity(32);
    for byte in bytes {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// 带连字符的离线 UUID（展示/诊断用；启动参数要的是无连字符版）。
#[must_use]
pub fn offline_uuid_dashed(name: &str) -> String {
    let raw = offline_uuid(name);
    format!(
        "{}-{}-{}-{}-{}",
        &raw[0..8],
        &raw[8..12],
        &raw[12..16],
        &raw[16..20],
        &raw[20..32]
    )
}

/// 校验并规范化离线玩家名。
///
/// 规则：非空、去掉首尾空白后 ≤16 字符、不含控制字符。
/// **允许非 ASCII**（中文名在离线服里很常见，UUID 按 UTF-8 字节算即可）。
///
/// # Errors
///
/// 空名/过长/含控制字符时返回 [`AuthError::Malformed`]。
pub fn normalize_name(name: &str) -> AuthResult<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AuthError::Malformed("玩家名不能为空".to_string()));
    }
    if trimmed.chars().count() > MAX_NAME_CHARS {
        return Err(AuthError::Malformed(format!(
            "玩家名最长 {MAX_NAME_CHARS} 个字符，收到 {} 个",
            trimmed.chars().count()
        )));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(AuthError::Malformed("玩家名不能含控制字符".to_string()));
    }
    Ok(trimmed.to_string())
}

/// 建一条离线账户元数据（名字已规范化）。
///
/// # Errors
///
/// 名字不合法时返回 [`AuthError::Malformed`]。
pub fn offline_meta(name: &str, created_at_ms: u64) -> AuthResult<AccountMeta> {
    let name = normalize_name(name)?;
    Ok(AccountMeta::new(
        AccountKind::Offline,
        offline_uuid(&name),
        name,
        created_at_ms,
    ))
}

/// 生成一次「登录」的启动占位（不联网、不落盘）。
///
/// # Errors
///
/// 元数据不是离线轨或名字为空时返回 [`AuthError::Malformed`] / [`AuthError::CredentialExpired`]。
pub fn offline_login(meta: &AccountMeta) -> AuthResult<AuthPlaceholders> {
    if meta.kind != AccountKind::Offline {
        return Err(AuthError::Malformed(format!(
            "账户 {} 不是离线轨（{}）",
            meta.id,
            meta.kind.as_str()
        )));
    }
    if meta.profile_name.trim().is_empty() {
        return Err(AuthError::CredentialExpired);
    }
    Ok(AuthPlaceholders {
        player_name: meta.profile_name.clone(),
        uuid: meta.profile_id.clone(),
        // 随机占位：每次登录都变（与 HMCL 一致），不落盘、不进日志。
        access_token: Secret::new(random_hex32()).expose().to_string(),
        user_type: OFFLINE_USER_TYPE.to_string(),
        user_properties: "{}".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_name, offline_login, offline_meta, offline_uuid, offline_uuid_dashed,
        OFFLINE_USER_TYPE,
    };
    use crate::auth::account::AccountKind;

    #[test]
    fn uuid_matches_jdk_name_uuid_from_bytes() {
        // 向量由 JDK 25 的 UUID.nameUUIDFromBytes 生成（见 tests/auth/offline_uuid.rs 的说明）。
        assert_eq!(
            offline_uuid("Notch"),
            "b50ad385829d3141a2167e7d7539ba7f".to_string()
        );
        assert_eq!(
            offline_uuid_dashed("Notch"),
            "b50ad385-829d-3141-a216-7e7d7539ba7f"
        );
    }

    #[test]
    fn uuid_has_v3_and_ietf_bits() {
        let raw = offline_uuid("AnyPlayer");
        let version = u8::from_str_radix(&raw[12..13], 16).expect("版本位");
        assert_eq!(version, 3, "必须标记为 v3（MD5）");
        let variant = u8::from_str_radix(&raw[16..17], 16).expect("变体位");
        assert!(
            (0x8..=0xb).contains(&variant),
            "变体位必须是 IETF（10xx），实际 {variant:x}"
        );
    }

    #[test]
    fn uuid_is_stable_and_case_sensitive() {
        assert_eq!(offline_uuid("Steve"), offline_uuid("Steve"));
        assert_ne!(offline_uuid("Steve"), offline_uuid("steve"));
    }

    #[test]
    fn name_validation() {
        assert_eq!(normalize_name("  Steve  ").expect("合法"), "Steve");
        assert_eq!(normalize_name("小明").expect("中文合法"), "小明");
        assert!(normalize_name("   ").is_err());
        assert!(normalize_name("0123456789abcdefg").is_err());
        assert!(normalize_name("bad\u{7}name").is_err());
    }

    #[test]
    fn offline_meta_and_login_shape() {
        let meta = offline_meta("小明", 1_000).expect("建元数据");
        assert_eq!(meta.kind, AccountKind::Offline);
        assert_eq!(meta.profile_id, offline_uuid("小明"));
        let placeholders = offline_login(&meta).expect("占位符");
        assert_eq!(placeholders.player_name, "小明");
        assert_eq!(placeholders.uuid, offline_uuid("小明"));
        assert_eq!(placeholders.user_type, OFFLINE_USER_TYPE);
        assert_eq!(placeholders.user_properties, "{}");
        assert_eq!(placeholders.access_token.len(), 32);
        // 每次登录的占位 token 都不同（HMCL 行为）。
        let again = offline_login(&meta).expect("占位符");
        assert_ne!(placeholders.access_token, again.access_token);
    }

    #[test]
    fn offline_login_rejects_other_tracks() {
        let mut meta = offline_meta("Steve", 1).expect("建元数据");
        meta.kind = AccountKind::Microsoft;
        assert!(offline_login(&meta).is_err());
    }
}
