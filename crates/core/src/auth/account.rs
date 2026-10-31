//! 账户模型：**条目 id 与 profileID 解耦**的三轨统一抽象。
//!
//! 承 `analyse/04` §2.3 / §3.2 / §12.1(1) 的三条硬教训：
//! 1. 条目 id（`account:<32hex>`）**不得**等于 profileID / 登录名 / 服务器 URL——
//!    这些值可能被多个条目共享（同一角色导入两次、服务器换域名）；
//! 2. **元数据 / 凭据双写分离**：`AccountMeta` 只装「能公开」的字段（列表渲染不需要解密）；
//! 3. 反序列化**允许不完整对象**（`profile_name` 可空 = 「有档案但名字待刷新」），
//!    补齐推迟到登录时——不能因为名字未知就整条丢弃。

use serde::{Deserialize, Serialize};

use crate::auth::authlib::AuthlibSession;
use crate::auth::error::{AuthError, AuthResult};
use crate::auth::microsoft::MicrosoftSession;
use crate::auth::secret::random_hex32;

/// 条目 id 前缀（防被误当 UUID；承 HMCL `AccountID.PREFIX`）。
pub const ACCOUNT_ID_PREFIX: &str = "account:";

/// 账户条目 id：`account:<32 位十六进制>`。
///
/// 与 profileID 无关：改服务器地址、改玩家名、同角色多条目都不影响它。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AccountId(String);

impl AccountId {
    /// 生成一个新条目 id。
    #[must_use]
    pub fn generate() -> Self {
        Self(format!("{ACCOUNT_ID_PREFIX}{}", random_hex32()))
    }

    /// 从磁盘字符串解析（宽容：坏数据返回 `None`，不 panic）。
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        let body = raw.strip_prefix(ACCOUNT_ID_PREFIX)?;
        if body.len() != 32 || !body.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return None;
        }
        Some(Self(raw.to_ascii_lowercase()))
    }

    /// 字符串视图。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AccountId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// 三条轨道。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccountKind {
    /// 离线（本地生成的 UUID，不联网）。
    Offline,
    /// 微软（device-code）。
    Microsoft,
    /// authlib-injector 第三方认证服务器（标准 Yggdrasil 协议）。
    AuthlibInjector,
}

impl AccountKind {
    /// 落盘用的稳定名（承 HMCL 的 `offline` / `microsoft` / `authlibInjector`）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::Microsoft => "microsoft",
            Self::AuthlibInjector => "authlibInjector",
        }
    }

    /// 解析（未知值返回 `None`，让上层跳过该条记录）。
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "offline" => Some(Self::Offline),
            "microsoft" => Some(Self::Microsoft),
            // 兼容两种写法：HMCL 用 camelCase，别的地方有人写 kebab。
            "authlibInjector" | "authlib-injector" => Some(Self::AuthlibInjector),
            _ => None,
        }
    }

    /// 人读标签（CLI/JSON 展示用）。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Offline => "离线",
            Self::Microsoft => "微软",
            Self::AuthlibInjector => "authlib-injector",
        }
    }
}

/// 账户元数据（**可公开**，不含任何凭据）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountMeta {
    /// 条目 id。
    pub id: AccountId,
    /// 轨道。
    pub kind: AccountKind,
    /// Minecraft 档案 UUID（32 位无连字符）。**不可变**。
    pub profile_id: String,
    /// 展示名/玩家名；空串 = 「有档案但名字待刷新」（合法中间态）。
    #[serde(default)]
    pub profile_name: String,
    /// 登录名（authlib 轨的邮箱/用户名；其余轨为 `None`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login_name: Option<String>,
    /// authlib 认证服务器 API 根（其余轨为 `None`；凭据之外唯一需要持久化的服务器信息）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
    /// 创建时刻（Unix 毫秒；仅用于排序/展示）。
    pub created_at_ms: u64,
}

impl AccountMeta {
    /// 构造一条元数据（`profile_name` 可后补）。
    #[must_use]
    pub fn new(
        kind: AccountKind,
        profile_id: impl Into<String>,
        profile_name: impl Into<String>,
        created_at_ms: u64,
    ) -> Self {
        Self {
            id: AccountId::generate(),
            kind,
            profile_id: profile_id.into(),
            profile_name: profile_name.into(),
            login_name: None,
            server_url: None,
            created_at_ms,
        }
    }

    /// 展示名：优先玩家名，其次登录名，最后条目 id（永不 panic、永不空串）。
    #[must_use]
    pub fn display_name(&self) -> &str {
        if !self.profile_name.trim().is_empty() {
            return &self.profile_name;
        }
        if let Some(login_name) = &self.login_name {
            if !login_name.trim().is_empty() {
                return login_name;
            }
        }
        self.id.as_str()
    }

    /// 该条目是否匹配一个查找键（名字 / 登录名 / 条目 id）。
    ///
    /// 大小写不敏感：玩家名在 MC 里大小写不敏感，CLI 用户不该为此折腾。
    #[must_use]
    pub fn matches(&self, key: &str) -> bool {
        let key = key.trim();
        if key.is_empty() {
            return false;
        }
        self.profile_name.eq_ignore_ascii_case(key)
            || self
                .login_name
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case(key))
            || self.id.as_str().eq_ignore_ascii_case(key)
    }

    /// 该条目的凭据是否与轨道匹配（防止磁盘上元数据被换成另一轨）。
    ///
    /// # Errors
    ///
    /// 轨道与凭据种类不一致时返回 [`AuthError::Malformed`]。
    pub fn check_credentials(&self, credentials: &Credentials) -> AuthResult<()> {
        if credentials.kind() == self.kind {
            return Ok(());
        }
        Err(AuthError::Malformed(format!(
            "账户 {} 的元数据轨道是 {}，凭据却是 {}",
            self.id,
            self.kind.as_str(),
            credentials.kind().as_str()
        )))
    }

    /// 从 JSON 值宽容解析（单条坏记录只返回 `Err`，由调用方跳过并记 warning）。
    ///
    /// # Errors
    ///
    /// 缺字段/类型不符/轨道未知时返回 [`AuthError::Malformed`]。
    pub fn from_value(value: &serde_json::Value) -> AuthResult<Self> {
        serde_json::from_value(value.clone())
            .map_err(|error| AuthError::Malformed(format!("账户元数据不合法：{error}")))
    }
}

/// 凭据载荷（**私有**，只落 `credentials.dat`，永不进列表/日志）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Credentials {
    /// 离线账户没有凭据（accessToken 每次登录随机生成，不落盘）。
    Offline,
    /// 微软会话（access + refresh + `notAfter`）。
    Microsoft(MicrosoftSession),
    /// authlib 会话（accessToken + clientToken）。
    AuthlibInjector(AuthlibSession),
}

impl Credentials {
    /// 所属轨道。
    #[must_use]
    pub const fn kind(&self) -> AccountKind {
        match self {
            Self::Offline => AccountKind::Offline,
            Self::Microsoft(_) => AccountKind::Microsoft,
            Self::AuthlibInjector(_) => AccountKind::AuthlibInjector,
        }
    }

    /// 该凭据里的所有明文敏感值（武装脱敏器用；离线轨为空）。
    #[must_use]
    pub fn secrets(&self) -> Vec<&str> {
        match self {
            Self::Offline => Vec::new(),
            Self::Microsoft(session) => session
                .secrets()
                .into_iter()
                .map(|secret| secret.expose())
                .collect(),
            Self::AuthlibInjector(session) => session
                .secrets()
                .into_iter()
                .map(|secret| secret.expose())
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AccountId, AccountKind, AccountMeta, Credentials};
    use crate::auth::microsoft::MicrosoftSession;
    use crate::auth::secret::Secret;

    #[test]
    fn account_id_shape_and_parse() {
        let id = AccountId::generate();
        assert!(id.as_str().starts_with("account:"));
        assert_eq!(id.as_str().len(), "account:".len() + 32);
        assert_eq!(AccountId::parse(id.as_str()), Some(id.clone()));
        assert_eq!(AccountId::parse("account:xyz"), None);
        assert_eq!(AccountId::parse("0192-blah"), None);
    }

    #[test]
    fn kind_roundtrip() {
        for kind in [
            AccountKind::Offline,
            AccountKind::Microsoft,
            AccountKind::AuthlibInjector,
        ] {
            assert_eq!(AccountKind::parse(kind.as_str()), Some(kind));
            assert!(!kind.label().is_empty());
        }
        assert_eq!(AccountKind::parse("nope"), None);
    }

    #[test]
    fn meta_matches_name_login_and_id() {
        let mut meta = AccountMeta::new(AccountKind::Offline, "uuid32", "Steve", 1);
        meta.login_name = Some("steve@example.com".to_string());
        assert!(meta.matches("Steve"));
        assert!(meta.matches("steve"));
        assert!(meta.matches("steve@example.com"));
        assert!(meta.matches(meta.id.as_str()));
        assert!(!meta.matches("Alex"));
        assert!(!meta.matches("  "));
    }

    #[test]
    fn display_name_falls_back() {
        let mut meta = AccountMeta::new(AccountKind::Microsoft, "uuid", "", 1);
        assert_eq!(meta.display_name(), meta.id.as_str());
        meta.login_name = Some("me@example.com".to_string());
        assert_eq!(meta.display_name(), "me@example.com");
        meta.profile_name = "Steve".to_string();
        assert_eq!(meta.display_name(), "Steve");
    }

    #[test]
    fn credential_kind_must_match_meta() {
        let meta = AccountMeta::new(AccountKind::Microsoft, "uuid", "Steve", 1);
        let session = MicrosoftSession::new(
            "uuid",
            "Steve",
            Secret::new("a"),
            Secret::new("r"),
            60,
            0,
            None,
        );
        assert!(meta
            .check_credentials(&Credentials::Microsoft(session))
            .is_ok());
        assert!(meta.check_credentials(&Credentials::Offline).is_err());
    }

    #[test]
    fn secrets_only_expose_real_credentials() {
        assert!(Credentials::Offline.secrets().is_empty());
        let session = MicrosoftSession::new(
            "uuid",
            "Steve",
            Secret::new("mc-token"),
            Secret::new("live-refresh"),
            60,
            0,
            None,
        );
        assert_eq!(
            Credentials::Microsoft(session).secrets(),
            vec!["mc-token", "live-refresh"]
        );
    }

    #[test]
    fn malformed_record_is_an_error_not_a_panic() {
        let value = serde_json::json!({ "id": "account:zz", "kind": "offline" });
        assert!(AccountMeta::from_value(&value).is_err());
    }
}
