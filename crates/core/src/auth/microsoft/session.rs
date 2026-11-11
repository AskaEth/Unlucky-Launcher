//! 微软会话与刷新语义：**HMCL 三层判定的类型化版本**（`analyse/04` §5.3 / §3.5 / §8.4）。
//!
//! 三层判定原样平移：
//! 1. `has_profile_name()`：没有玩家名就没法启动（`--username` 不能为空）；
//! 2. `not_after`：本地时间戳过期即视为未登录（省一次网络）；
//! 3. 进程内 `authenticated` 标记归宿主（GUI/CLI）持有，不在本类型里。
//!
//! 关键语义（照 HMCL，别自己发明）：
//! - 存下来的 `refresh_token` 是 **Live（MSA）** 的，`access_token` 是 **Minecraft** 的；
//! - 刷新 = 整条链重跑（Live refresh → XBL → XSTS → login_with_xbox → entitlements → profile），
//!   不存在「只刷 MC token」的捷径；
//! - `profile_name` 属于**可变的私有缓存**（2022 年后允许改名），元数据里只留不可变的 profileID。

use serde::{Deserialize, Serialize};

use crate::auth::error::{AuthError, AuthResult};
use crate::auth::secret::Secret;
use crate::launch::request::AuthPlaceholders;

/// 微软账户的 launch 用户类型（`AuthInfo.USER_TYPE_MSA`）。
pub const USER_TYPE_MSA: &str = "msa";

/// 一次成功登录后的会话（= 该账户的凭据载荷）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicrosoftSession {
    /// Minecraft 档案 UUID（32 位无连字符）。**不可变**。
    pub profile_id: String,
    /// 玩家名（可空串：磁盘恢复允许「有档案但名字未知」，登录时补齐）。
    #[serde(default)]
    pub profile_name: String,
    /// MC token 类型（`Bearer`）。
    pub token_type: String,
    /// Minecraft access token。
    pub access_token: Secret,
    /// Live refresh token（轮换：每次刷新都会被新值整体覆盖）。
    pub refresh_token: Secret,
    /// MC token 过期时刻（Unix 毫秒；0 = 未知 → 强制刷新）。
    pub not_after_ms: u64,
    /// Xbox user id（`XUID`，诊断/排障用，非凭据）。
    pub user_id: Option<String>,
}

/// 过期判定的结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshDecision {
    /// 本地认为可用（有玩家名且未过期）：直接复用，不打网络。
    Reuse,
    /// 需要整链重跑。
    Refresh(RefreshReason),
}

/// 为什么要刷新。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshReason {
    /// 玩家名缺失（改名或历史档案）——启动参数拿不到 `--username`。
    MissingProfileName,
    /// 本地时间戳已过期。
    Expired,
    /// 名字缺失**且**过期（两条理由都成立）。
    MissingProfileNameAndExpired,
}

impl RefreshReason {
    /// 人读一行。
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::MissingProfileName => "会话缺少玩家名，需要刷新",
            Self::Expired => "Minecraft 令牌已过期，需要刷新",
            Self::MissingProfileNameAndExpired => "会话缺少玩家名且令牌已过期，需要刷新",
        }
    }
}

impl MicrosoftSession {
    /// 构造（`expires_in` 是秒，`now_ms` 是当前 Unix 毫秒）。
    #[must_use]
    pub fn new(
        profile_id: impl Into<String>,
        profile_name: impl Into<String>,
        access_token: Secret,
        refresh_token: Secret,
        expires_in_secs: u64,
        now_ms: u64,
        user_id: Option<String>,
    ) -> Self {
        Self {
            profile_id: profile_id.into(),
            profile_name: profile_name.into(),
            token_type: "Bearer".to_string(),
            access_token,
            refresh_token,
            not_after_ms: now_ms.saturating_add(expires_in_secs.saturating_mul(1000)),
            user_id,
        }
    }

    /// 有没有玩家名（空串/全空白都算没有）。
    #[must_use]
    pub fn has_profile_name(&self) -> bool {
        !self.profile_name.trim().is_empty()
    }

    /// 本地是否过期（`not_after_ms == 0` 视为过期，承 HMCL「坏数据强制刷新」）。
    #[must_use]
    pub fn is_expired(&self, now_ms: u64) -> bool {
        self.not_after_ms == 0 || now_ms >= self.not_after_ms
    }

    /// 刷新判定（纯函数，可单测；不打网络）。
    #[must_use]
    pub fn refresh_decision(&self, now_ms: u64) -> RefreshDecision {
        match (self.has_profile_name(), self.is_expired(now_ms)) {
            (true, false) => RefreshDecision::Reuse,
            (false, false) => RefreshDecision::Refresh(RefreshReason::MissingProfileName),
            (true, true) => RefreshDecision::Refresh(RefreshReason::Expired),
            (false, true) => RefreshDecision::Refresh(RefreshReason::MissingProfileNameAndExpired),
        }
    }

    /// 该会话所有凭据值（武装脱敏器用）。
    #[must_use]
    pub fn secrets(&self) -> Vec<&Secret> {
        vec![&self.access_token, &self.refresh_token]
    }

    /// 转成 launch 的占位符集合（`user_properties` 为 `{}`，承 HMCL）。
    ///
    /// # Errors
    ///
    /// 会话缺少玩家名/UUID 时返回 [`AuthError::CredentialExpired`]——此时必须刷新，
    /// 不能拿空名字去拼命令行（`--username` 空串会导致游戏直接退出）。
    pub fn to_placeholders(&self) -> AuthResult<AuthPlaceholders> {
        if !self.has_profile_name() {
            return Err(AuthError::CredentialExpired);
        }
        if self.profile_id.is_empty() {
            return Err(AuthError::Malformed("会话缺少 profileID".to_string()));
        }
        Ok(AuthPlaceholders {
            player_name: self.profile_name.clone(),
            uuid: self.profile_id.clone(),
            access_token: self.access_token.expose().to_string(),
            user_type: USER_TYPE_MSA.to_string(),
            user_properties: "{}".to_string(),
        })
    }

    /// 刷新后守住两条不变量（`analyse/04` §5.3 的 `WrongAccountException` / `hasProfileName`）。
    ///
    /// 返回 `false` 表示「刷出来的不是同一个档案」，调用方必须当错误处理。
    #[must_use]
    pub fn same_profile(&self, other: &Self) -> bool {
        self.profile_id == other.profile_id
    }
}

#[cfg(test)]
mod tests {
    use super::{MicrosoftSession, RefreshDecision, RefreshReason};
    use crate::auth::secret::Secret;

    fn session(name: &str, not_after_ms: u64) -> MicrosoftSession {
        MicrosoftSession {
            profile_id: "0f8fad5bd9cb469fa16570867728950e".to_string(),
            profile_name: name.to_string(),
            token_type: "Bearer".to_string(),
            access_token: Secret::new("mc-token-value"),
            refresh_token: Secret::new("live-refresh-value"),
            not_after_ms,
            user_id: None,
        }
    }

    #[test]
    fn decision_matrix() {
        let now = 1_000_000;
        assert_eq!(
            session("Steve", now + 1).refresh_decision(now),
            RefreshDecision::Reuse
        );
        assert_eq!(
            session("", now + 1).refresh_decision(now),
            RefreshDecision::Refresh(RefreshReason::MissingProfileName)
        );
        assert_eq!(
            session("Steve", now - 1).refresh_decision(now),
            RefreshDecision::Refresh(RefreshReason::Expired)
        );
        assert_eq!(
            session("   ", now).refresh_decision(now),
            RefreshDecision::Refresh(RefreshReason::MissingProfileNameAndExpired)
        );
    }

    #[test]
    fn zero_not_after_forces_refresh() {
        assert_eq!(
            session("Steve", 0).refresh_decision(5),
            RefreshDecision::Refresh(RefreshReason::Expired)
        );
    }

    #[test]
    fn new_computes_not_after_from_expires_in() {
        let built = MicrosoftSession::new(
            "uuid",
            "Steve",
            Secret::new("a"),
            Secret::new("r"),
            86_400,
            1_000,
            None,
        );
        assert_eq!(built.not_after_ms, 1_000 + 86_400_000);
        assert_eq!(built.token_type, "Bearer");
    }

    #[test]
    fn placeholders_need_a_name() {
        assert!(session("", 10).to_placeholders().is_err());
        let placeholders = session("Steve", 10).to_placeholders().expect("占位符");
        assert_eq!(placeholders.player_name, "Steve");
        assert_eq!(placeholders.user_type, "msa");
        assert_eq!(placeholders.user_properties, "{}");
        // token 进了占位符（启动需要），但它自己的 Debug 是脱敏的。
        assert_eq!(placeholders.access_token, "mc-token-value");
        assert!(!format!("{:?}", session("Steve", 10)).contains("mc-token-value"));
    }
}
