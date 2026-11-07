//! device-code 流：发起（拿 `user_code` + `verification_uri`）→ 轮询换 token →（刷新同构）。
//!
//! 逐条承 `analyse/04` §4.3 / §4.4 / §4.5：
//! - **硬超时**：只等 `min(expires_in, 900s)`，防止后台任务永久挂起；
//! - **`slow_down` 是流程内状态**，不是失败：轮询间隔 +5 秒继续；
//! - **每次轮询前先 sleep**（别密集打服务端）；
//! - **refresh 同时返回新 refresh_token**，调用方必须整体覆盖（轮换）；
//! - 收紧 HMCL 的观察项 1：**只要 `error` 非空就失败**，不再「描述为空就静默放行」。
//!
//! 打印 `user_code`/URL 不在这里：core 不做 UI，宿主拿 [`DeviceCodePrompt`] 自己决定
//! （CLI 打到 stderr，GUI 弹对话框 + 二维码）。

use std::time::{Duration, Instant};

use reqwest::Client;

use crate::auth::error::{AuthError, AuthResult};
use crate::auth::secret::Secret;

use super::endpoints::MicrosoftEndpoints;
use crate::auth::http::{post_form, Reply};

/// 申请的 scope（`XboxLive.signin` + 长期刷新）。
pub const SCOPE: &str = "XboxLive.signin offline_access";
/// 设备码流程的硬上限：无论服务端给多长，最多等这么久。
pub const DEVICE_CODE_MAX_WAIT: Duration = Duration::from_secs(900);
/// 默认轮询间隔下限（服务端 `interval` 更小时按下限走）。
pub const DEFAULT_MIN_INTERVAL: Duration = Duration::from_secs(1);
/// `slow_down` 的退避步长（承 HMCL：+5 秒）。
pub const DEFAULT_SLOW_DOWN_STEP: Duration = Duration::from_secs(5);
/// device_code grant 类型。
pub const DEVICE_CODE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// 轮询策略（生产用 [`PollPolicy::default`]，测试用小间隔跑同一套状态机）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PollPolicy {
    /// 轮询间隔下限。
    pub min_interval: Duration,
    /// `slow_down` 的退避步长。
    pub slow_down_step: Duration,
    /// 整个流程最多等多久（还会与 `expires_in` 取小）。
    pub max_wait: Duration,
}

impl Default for PollPolicy {
    fn default() -> Self {
        Self {
            min_interval: DEFAULT_MIN_INTERVAL,
            slow_down_step: DEFAULT_SLOW_DOWN_STEP,
            max_wait: DEVICE_CODE_MAX_WAIT,
        }
    }
}

/// 要展示给用户的信息（URL + 用户码）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCodePrompt {
    /// 用户在验证页输入的短码（如 `ABCD1234`）。**不是凭据**，可打印。
    pub user_code: String,
    /// 验证地址（`https://www.microsoft.com/link` 等）。
    pub verification_uri: String,
    /// 服务端给的有效期。
    pub expires_in: Duration,
    /// 服务端建议的轮询间隔。
    pub interval: Duration,
}

impl DeviceCodePrompt {
    /// 人读两行（CLI 打到 stderr，GUI 直接展示）。
    #[must_use]
    pub fn lines(&self) -> (String, String) {
        (
            format!("请在任意设备打开 {} 并输入代码：", self.verification_uri),
            format!(
                "用户码 {}（{} 分钟内有效）",
                self.user_code,
                self.expires_in.as_secs() / 60
            ),
        )
    }
}

/// 设备码申请的中间态（`device_code` 是凭据形状，不进日志）。
#[derive(Debug, Clone)]
pub(crate) struct DeviceCodeGrant {
    /// 展示信息。
    pub prompt: DeviceCodePrompt,
    /// 轮询用的长码（脱敏 Debug，永不打印）。
    pub device_code: Secret,
}

/// OAuth 换到的 token 对。
#[derive(Debug, Clone)]
pub(crate) struct OAuthTokens {
    /// Live access token（只在链内用，不落盘）。
    pub access_token: Secret,
    /// Live refresh token（落盘、轮换）。
    pub refresh_token: Secret,
}

/// 申请设备码。
pub(crate) async fn request_device_code(
    client: &Client,
    endpoints: &MicrosoftEndpoints,
    client_id: &str,
) -> AuthResult<DeviceCodeGrant> {
    let reply = post_form(
        client,
        &endpoints.device_code,
        vec![
            ("client_id", client_id.to_string()),
            ("scope", SCOPE.to_string()),
        ],
    )
    .await?;
    if !reply.is_success() {
        return Err(oauth_error(&reply, &endpoints.device_code));
    }
    let user_code = reply
        .text("user_code")
        .ok_or_else(|| AuthError::Malformed("设备码响应缺 user_code".to_string()))?;
    let device_code = reply
        .text("device_code")
        .ok_or_else(|| AuthError::Malformed("设备码响应缺 device_code".to_string()))?;
    let verification_uri = reply
        .text("verification_uri")
        .or_else(|| reply.text("verification_url"))
        .ok_or_else(|| AuthError::Malformed("设备码响应缺 verification_uri".to_string()))?;
    let expires_in = reply.number("expires_in").unwrap_or(900).max(1);
    // 服务端间隔原样收下，下限交给 [`PollPolicy::min_interval`]（生产 1 秒，测试可压到毫秒）。
    let interval = reply.number("interval").unwrap_or(5);
    Ok(DeviceCodeGrant {
        prompt: DeviceCodePrompt {
            user_code: user_code.to_string(),
            verification_uri: verification_uri.to_string(),
            expires_in: Duration::from_secs(expires_in),
            interval: Duration::from_secs(interval),
        },
        device_code: Secret::new(device_code),
    })
}

/// 轮询直到用户完成授权（或超时/被拒/取消）。
pub(crate) async fn poll_device_token(
    client: &Client,
    endpoints: &MicrosoftEndpoints,
    client_id: &str,
    grant: &DeviceCodeGrant,
    policy: PollPolicy,
) -> AuthResult<OAuthTokens> {
    let limit = grant.prompt.expires_in.min(policy.max_wait);
    let mut interval = grant.prompt.interval.max(policy.min_interval);
    let started = Instant::now();
    loop {
        if started.elapsed() >= limit {
            return Err(AuthError::Timeout {
                what: format!("等待设备码授权（上限 {} 秒）", limit.as_secs()),
            });
        }
        tokio::time::sleep(interval).await;
        let reply = post_form(
            client,
            &endpoints.token,
            vec![
                ("client_id", client_id.to_string()),
                ("grant_type", DEVICE_CODE_GRANT.to_string()),
                ("code", grant.device_code.expose().to_string()),
            ],
        )
        .await?;

        if let Some(error) = reply.text("error") {
            match error {
                "authorization_pending" => continue,
                // 服务端压力反馈：不是失败，退避后继续（承 HMCL）。
                "slow_down" => {
                    interval += policy.slow_down_step;
                    continue;
                }
                "expired_token" => {
                    return Err(AuthError::Timeout {
                        what: "设备码已过期（需重新发起登录）".to_string(),
                    })
                }
                "access_denied" | "authorization_declined" => return Err(AuthError::Canceled),
                _ => return Err(oauth_error(&reply, &endpoints.token)),
            }
        }
        return tokens_from(&reply, &endpoints.token);
    }
}

/// 用 refresh_token 换新的 token 对（**刷新也会返回新 refresh_token**）。
pub(crate) async fn refresh_tokens(
    client: &Client,
    endpoints: &MicrosoftEndpoints,
    client_id: &str,
    refresh_token: &Secret,
) -> AuthResult<OAuthTokens> {
    let reply = post_form(
        client,
        &endpoints.token,
        vec![
            ("client_id", client_id.to_string()),
            ("grant_type", "refresh_token".to_string()),
            ("refresh_token", refresh_token.expose().to_string()),
        ],
    )
    .await?;
    if let Some(error) = reply.text("error") {
        // `invalid_grant` + `AADSTS70000` = 授权被撤销/过期 → 唯一该重新登录的信号。
        if error == "invalid_grant" {
            let description = reply.text("error_description").unwrap_or("");
            if description.contains("AADSTS70000") {
                return Err(AuthError::CredentialExpired);
            }
        }
        return Err(oauth_error(&reply, &endpoints.token));
    }
    if !reply.is_success() {
        return Err(oauth_error(&reply, &endpoints.token));
    }
    tokens_from(&reply, &endpoints.token)
}

pub(crate) fn tokens_from(reply: &Reply, url: &str) -> AuthResult<OAuthTokens> {
    let access_token = reply
        .text("access_token")
        .ok_or_else(|| AuthError::Malformed(format!("{url} 响应缺 access_token")))?;
    let refresh_token = reply
        .text("refresh_token")
        .ok_or_else(|| AuthError::Malformed(format!("{url} 响应缺 refresh_token")))?;
    Ok(OAuthTokens {
        access_token: Secret::new(access_token),
        refresh_token: Secret::new(refresh_token),
    })
}

/// OAuth 错误归一：**`error` 非空即失败**（HMCL 只在两个字段都非空时处理，我们收紧）。
pub(crate) fn oauth_error(reply: &Reply, url: &str) -> AuthError {
    let Some(code) = reply.text("error") else {
        return AuthError::network(
            url.split('?').next().unwrap_or(url),
            format!("HTTP {}（响应不是 OAuth 错误体）", reply.status),
        );
    };
    let description = reply
        .text("error_description")
        .unwrap_or("服务端未给描述")
        .to_string();
    if code == "invalid_grant" && description.contains("AADSTS70000") {
        return AuthError::CredentialExpired;
    }
    AuthError::Denied {
        code: code.to_string(),
        description,
    }
}

#[cfg(test)]
mod tests {
    use super::{oauth_error, PollPolicy, DEFAULT_MIN_INTERVAL, DEVICE_CODE_MAX_WAIT};
    use crate::auth::error::AuthError;
    use crate::auth::http::Reply;
    use serde_json::json;

    #[test]
    fn default_policy_matches_hmcl_bounds() {
        let policy = PollPolicy::default();
        assert_eq!(policy.min_interval, DEFAULT_MIN_INTERVAL);
        assert_eq!(policy.max_wait, DEVICE_CODE_MAX_WAIT);
        assert_eq!(policy.max_wait.as_secs(), 900);
    }

    #[test]
    fn error_without_description_still_fails() {
        let reply = reply(400, Some(json!({ "error": "invalid_request" })));
        match oauth_error(&reply, "https://example.invalid/token") {
            AuthError::Denied { code, .. } => assert_eq!(code, "invalid_request"),
            other => panic!("应是 Denied，实际 {other:?}"),
        }
    }

    #[test]
    fn aadsts70000_becomes_credential_expired() {
        let reply = reply(
            400,
            Some(json!({
                "error": "invalid_grant",
                "error_description": "AADSTS70000: The grant is expired or revoked"
            })),
        );
        assert!(oauth_error(&reply, "https://example.invalid/token").is_credential_expired());
    }

    #[test]
    fn non_oauth_body_is_network_error() {
        let reply = reply(500, None);
        assert_eq!(
            oauth_error(&reply, "https://example.invalid/token").code(),
            "network"
        );
    }

    fn reply(status: u16, body: Option<serde_json::Value>) -> Reply {
        Reply {
            status,
            body,
            content_type: Some("application/json".to_string()),
            raw: None,
        }
    }
}
