//! 授权码流（`research/08` §3 方案 A）：授权 URL 构造 + code 换 token + 整链编排。
//!
//! 与设备码流的分工：**只多一跳**——拿 MS access_token 的方式不同，往后的
//! XBL → XSTS → login_with_xbox → entitlements → profile 与刷新链完全复用
//! （[`super::chain::complete_chain`]），所以两条流的凭据存储与刷新语义天然一致。
//!
//! # 三个硬性细节（都是「抄错就失败」的那一类）
//!
//! 1. **租户 `consumers`**：授权页与 token 端点都走 `login.microsoftonline.com/consumers/…`，
//!    换成 `common`/AAD 租户 id 时 `XboxLive.signin` 直接被拒（`research/08` §2 约束 1）。
//! 2. **重定向 URI 严格一致**：`http://localhost:{port}/auth` —— 授权 URL 里的、
//!    token 请求体里的、本地服务器实际监听的必须是**同一个字符串**（含端口与路径），
//!    所以本模块只从 [`RedirectUri`] 取这一个来源。
//! 3. **`prompt=select_account`**：不带这个参数，用户会被静默塞进上一次的账号（坑 6）。
//!
//! scope 与设备码流共用 [`super::flow::SCOPE`]（`XboxLive.signin offline_access`）。

use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::Client;

use crate::auth::error::AuthResult;
use crate::auth::http::post_form;
use crate::auth::secret::Secret;

use super::callback::{CallbackServer, REDIRECT_PATH};
use super::chain::complete_chain;
use super::endpoints::MicrosoftEndpoints;
use super::flow::{oauth_error, tokens_from, OAuthTokens, SCOPE};
use super::now_ms;
use super::session::MicrosoftSession;

/// 授权码 grant 类型。
pub const AUTHORIZATION_CODE_GRANT: &str = "authorization_code";
/// 默认等回调的时间预算（用户要开浏览器、登录、点同意）。
pub const DEFAULT_CALLBACK_TIMEOUT: Duration = Duration::from_secs(300);
/// 默认的端口绑定尝试次数。
pub const DEFAULT_BIND_ATTEMPTS: u8 = 3;

/// 回填到授权 URL 与 token 请求里的重定向 URI（**唯一来源**）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedirectUri {
    port: u16,
    url: String,
}

impl RedirectUri {
    /// 按实际绑定的端口构造 `http://localhost:{port}/auth`。
    ///
    /// 用 `localhost` 而不是 `127.0.0.1`：微软对两者处理不一致（`research/08` 坑 5）。
    #[must_use]
    pub fn new(port: u16) -> Self {
        Self {
            port,
            url: format!("http://localhost:{port}{REDIRECT_PATH}"),
        }
    }

    /// 端口。
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// URI 字面量（授权 URL 与 token 请求共用，保证逐字节一致）。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.url
    }
}

impl fmt::Display for RedirectUri {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.url)
    }
}

/// 授权码流的运行参数（生产用 [`AuthCodePolicy::default`]，测试压短超时/固定端口）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthCodePolicy {
    /// 等浏览器回调的上限。
    pub callback_timeout: Duration,
    /// 端口绑定尝试次数（`>= 1`）。
    pub bind_attempts: u8,
    /// 固定端口（`None` = 由内核分配，生产路径）。
    pub port: Option<u16>,
    /// CSRF 用的 `state`（`None` = 每次随机生成）。
    pub state: Option<String>,
}

impl Default for AuthCodePolicy {
    fn default() -> Self {
        Self {
            callback_timeout: DEFAULT_CALLBACK_TIMEOUT,
            bind_attempts: DEFAULT_BIND_ATTEMPTS,
            port: None,
            state: None,
        }
    }
}

/// 要宿主做的事：打开浏览器（core 不碰 UI：谁来开由 CLI/GUI 决定）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthCodePrompt {
    /// 授权页 URL（宿主交给系统默认浏览器；也可展示给用户手动打开）。
    pub authorization_url: String,
    /// 本地回调地址（展示/排障用；与授权 URL 里的 `redirect_uri` 同源）。
    pub redirect_uri: String,
    /// 最多等多久。
    pub timeout: Duration,
}

/// 拼授权 URL（`response_type=code` + `prompt=select_account` + 严格编码的重定向 URI）。
#[must_use]
pub fn authorization_url(
    endpoints: &MicrosoftEndpoints,
    client_id: &str,
    redirect_uri: &RedirectUri,
    state: &str,
) -> String {
    format!(
        "{}?client_id={}&response_type=code&redirect_uri={}&scope={}&prompt=select_account&state={}",
        endpoints.authorize,
        encode(client_id),
        encode(redirect_uri.as_str()),
        encode(SCOPE),
        encode(state),
    )
}

/// 授权码换 token（`grant_type=authorization_code`）。
///
/// # Errors
///
/// OAuth 拒绝（[`crate::auth::AuthError::Denied`] /
/// [`crate::auth::AuthError::CredentialExpired`]）、网络失败、响应缺 token。
pub(crate) async fn exchange_code(
    client: &Client,
    endpoints: &MicrosoftEndpoints,
    client_id: &str,
    code: &Secret,
    redirect_uri: &RedirectUri,
) -> AuthResult<OAuthTokens> {
    let reply = post_form(
        client,
        &endpoints.token,
        vec![
            ("client_id", client_id.to_string()),
            ("grant_type", AUTHORIZATION_CODE_GRANT.to_string()),
            ("code", code.expose().to_string()),
            ("redirect_uri", redirect_uri.as_str().to_string()),
            ("scope", SCOPE.to_string()),
        ],
    )
    .await?;
    if reply.text("error").is_some() || !reply.is_success() {
        return Err(oauth_error(&reply, &endpoints.token));
    }
    tokens_from(&reply, &endpoints.token)
}

/// 授权码登录主体：绑端口 → 出授权 URL（交给 `present`）→ 等回调 → 换 token → 整链。
///
/// `present` 在**端口绑定之后、等待之前**被调用：宿主此时才知道端口，也只有这时
/// 打开浏览器才不会让用户对着还没起来的服务端登录。
pub(crate) async fn login<F>(
    client: &Client,
    endpoints: &MicrosoftEndpoints,
    client_id: &str,
    policy: AuthCodePolicy,
    present: F,
) -> AuthResult<MicrosoftSession>
where
    F: FnOnce(&AuthCodePrompt),
{
    let state = policy.state.clone().unwrap_or_else(nonce);
    let server = CallbackServer::bind(policy.port, policy.bind_attempts, state.clone()).await?;
    let redirect_uri = RedirectUri::new(server.port());
    present(&AuthCodePrompt {
        authorization_url: authorization_url(endpoints, client_id, &redirect_uri, &state),
        redirect_uri: redirect_uri.as_str().to_string(),
        timeout: policy.callback_timeout,
    });
    let code = server.wait_for_code(policy.callback_timeout).await?;
    let tokens = exchange_code(
        client,
        endpoints,
        client_id,
        &Secret::new(code),
        &redirect_uri,
    )
    .await?;
    complete_chain(client, endpoints, tokens, now_ms()).await
}

/// 每次登录一个 `state`（不用于加密，只用来把迟到的旧回调挡掉）。
///
/// 128 位（两个独立随机种子的 SipHash 输出拼接）：RFC 6749 §10.10 建议 `state` 至少
/// 128 位熵，挡住「猜到 state 就能伪造回调」这类攻击面。
fn nonce() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    let mut halves = [String::new(), String::new()];
    for half in &mut halves {
        // 每次 `RandomState::new()` 都是独立随机种子 → 两段各 64 位、彼此不可推。
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u128(nanos);
        hasher.write_u32(std::process::id());
        *half = format!("{:016x}", hasher.finish());
    }
    format!("{}{}", halves[0], halves[1])
}

/// 百分号编码（RFC 3986 unreserved 之外一律转义；空格用 `%20` 而不是 `+`）。
fn encode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(char::from(byte));
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{authorization_url, encode, RedirectUri, AUTHORIZATION_CODE_GRANT};
    use crate::auth::microsoft::MicrosoftEndpoints;

    #[test]
    fn redirect_uri_is_localhost_with_exact_port_and_path() {
        let uri = RedirectUri::new(51_234);
        assert_eq!(uri.as_str(), "http://localhost:51234/auth");
        assert_eq!(uri.port(), 51_234);
        assert_eq!(uri.to_string(), uri.as_str());
    }

    #[test]
    fn authorization_url_matches_research_08_vector() {
        let endpoints = MicrosoftEndpoints::consumers();
        let uri = RedirectUri::new(8_080);
        let url = authorization_url(&endpoints, "0000-client", &uri, "state-123");
        assert_eq!(
            url,
            "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize\
?client_id=0000-client&response_type=code\
&redirect_uri=http%3A%2F%2Flocalhost%3A8080%2Fauth\
&scope=XboxLive.signin%20offline_access&prompt=select_account&state=state-123"
        );
        // 三个硬性细节各断言一遍，改坏了立刻知道是哪一条。
        assert!(url.contains("/consumers/oauth2/v2.0/authorize"));
        assert!(url.contains("prompt=select_account"));
        assert!(url.contains("scope=XboxLive.signin%20offline_access"));
    }

    #[test]
    fn encode_escapes_everything_but_unreserved() {
        assert_eq!(encode("abc-._~09"), "abc-._~09");
        assert_eq!(encode("a b/c:"), "a%20b%2Fc%3A");
    }

    #[test]
    fn grant_type_is_the_standard_one() {
        assert_eq!(AUTHORIZATION_CODE_GRANT, "authorization_code");
    }

    #[test]
    fn generated_state_has_128_bits_of_entropy() {
        let first = super::nonce();
        let second = super::nonce();
        assert_eq!(first.len(), 32, "128 位 = 32 个 hex 字符：{first}");
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()), "{first}");
        assert_ne!(first, second, "每次登录的 state 必须不同");
    }
}
