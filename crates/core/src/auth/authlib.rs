//! authlib-injector 轨（**账户侧**）：标准 Yggdrasil 协议的登录/刷新/校验/注销。
//!
//! 边界（mission 明示）：本模块只做「账户」；`-javaagent` 三件套（jar 下载、
//! `-Dauthlibinjector.side=client`、`yggdrasil.prefetched` 的 Base64）归后续 mission。
//! 依据 `analyse/04` §6.1–§6.2（端点布局与四动作报文）与 §6.3（服务发现约定）。
//!
//! 三条照抄要点：
//! - 端点 = `<apiRoot>authserver/{authenticate,refresh,validate,invalidate}`，`apiRoot`
//!   必须带尾斜杠（`https://littleskin.cn/api/yggdrasil/` 这种目录布局是规范要求）；
//! - `clientToken` 是**启动器侧生成的随机值**，服务端回传必须一致（防会话被替换/重放）；
//! - 「token 无效」用**返回值 `false`** 表达（`validate` 的 `ForbiddenOperationException`），
//!   「服务器抽风」才抛错——调用方据此决定「刷新」还是「重试」。

pub mod server_meta;

pub use server_meta::AuthlibServerMeta;

use reqwest::Client;
use serde_json::json;

use crate::auth::error::{AuthError, AuthResult};
use crate::auth::http::{post_json, Reply};
use crate::auth::secret::Secret;
use crate::download::config::EngineConfig;
use crate::download::http::build_client;
use crate::launch::request::AuthPlaceholders;

/// authlib 轨的 launch 用户类型（承 HMCL `YggdrasilSession.toAuthInfo`：也是 `msa`）。
pub const USER_TYPE_MSA: &str = "msa";

/// 把用户输入规范成 API 根：补 `https://`、补尾斜杠、去空白。
///
/// # Errors
///
/// 空串或只剩协议头时返回 [`AuthError::Malformed`]。
pub fn normalize_api_root(input: &str) -> AuthResult<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(AuthError::Malformed(
            "认证服务器地址不能为空（例：https://littleskin.cn/api/yggdrasil/）".to_string(),
        ));
    }
    let with_scheme = if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let host = with_scheme
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or("");
    if host.trim_matches('/').is_empty() {
        return Err(AuthError::Malformed(format!(
            "认证服务器地址不完整：`{input}`"
        )));
    }
    Ok(if with_scheme.ends_with('/') {
        with_scheme
    } else {
        format!("{with_scheme}/")
    })
}

/// Yggdrasil 四个端点（都从 API 根派生）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthlibEndpoints {
    /// `authserver/authenticate`。
    pub authenticate: String,
    /// `authserver/refresh`。
    pub refresh: String,
    /// `authserver/validate`。
    pub validate: String,
    /// `authserver/invalidate`。
    pub invalidate: String,
}

impl AuthlibEndpoints {
    /// 从 API 根派生（先 [`normalize_api_root`]）。
    ///
    /// # Errors
    ///
    /// 地址不合法时返回 [`AuthError::Malformed`]。
    pub fn from_api_root(api_root: &str) -> AuthResult<Self> {
        let root = normalize_api_root(api_root)?;
        let join = |path: &str| format!("{root}{path}");
        Ok(Self {
            authenticate: join("authserver/authenticate"),
            refresh: join("authserver/refresh"),
            validate: join("authserver/validate"),
            invalidate: join("authserver/invalidate"),
        })
    }
}

/// 一个 Yggdrasil 角色（`selectedProfile`）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AuthlibProfile {
    /// 32 位无连字符 UUID。
    pub id: String,
    /// 角色名。
    pub name: String,
}

/// authlib 会话 = 该账户的凭据载荷。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthlibSession {
    /// Yggdrasil accessToken。
    pub access_token: Secret,
    /// 启动器生成的 clientToken（刷新/校验都要带）。
    pub client_token: Secret,
    /// 当前选中的角色（多角色服务器里由 refresh 的 `selectedProfile` 决定）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_profile: Option<AuthlibProfile>,
}

impl AuthlibSession {
    /// 该会话的全部凭据值（武装脱敏器）。
    #[must_use]
    pub fn secrets(&self) -> Vec<&Secret> {
        vec![&self.access_token, &self.client_token]
    }

    /// 有没有角色（没角色的会话不能启动游戏）。
    #[must_use]
    pub fn has_profile(&self) -> bool {
        self.selected_profile
            .as_ref()
            .is_some_and(|profile| !profile.name.trim().is_empty() && !profile.id.is_empty())
    }

    /// 转成 launch 占位符。
    ///
    /// `user_properties` 用 `{}`：纹理由游戏进程经 authlib-injector 向服务器取
    /// （注入参数属后续 mission），启动参数不需要它。
    ///
    /// # Errors
    ///
    /// 会话没有角色时返回 [`AuthError::CredentialExpired`]（需要重新选角色/登录）。
    pub fn to_placeholders(&self) -> AuthResult<AuthPlaceholders> {
        let profile = self
            .selected_profile
            .as_ref()
            .filter(|profile| !profile.name.trim().is_empty())
            .ok_or(AuthError::CredentialExpired)?;
        Ok(AuthPlaceholders {
            player_name: profile.name.clone(),
            uuid: profile.id.clone(),
            access_token: self.access_token.expose().to_string(),
            user_type: USER_TYPE_MSA.to_string(),
            user_properties: "{}".to_string(),
        })
    }
}

/// authlib 认证客户端。
#[derive(Debug, Clone)]
pub struct AuthlibAuth {
    client: Client,
}

impl AuthlibAuth {
    /// 构造（HTTP 基座复用 `core::download`）。
    ///
    /// # Errors
    ///
    /// 客户端建不起来时返回 [`AuthError::Network`]。
    pub fn new() -> AuthResult<Self> {
        let client = build_client(&EngineConfig::default())
            .map_err(|error| AuthError::network("http-client", error.to_string()))?;
        Ok(Self { client })
    }

    /// 注入客户端（测试）。
    #[must_use]
    pub fn with_client(mut self, client: Client) -> Self {
        self.client = client;
        self
    }

    /// 密码登录（**新账户**）。密码只在本调用栈里出现，不落盘、不入日志。
    ///
    /// # Errors
    ///
    /// 密码错/服务器拒绝 → [`AuthError::Denied`]；网络与结构问题各有分流。
    pub async fn authenticate(
        &self,
        api_root: &str,
        login_name: &str,
        password: &Secret,
        client_token: &Secret,
    ) -> AuthResult<AuthlibSession> {
        let endpoints = AuthlibEndpoints::from_api_root(api_root)?;
        let body = json!({
            "agent": { "name": "Minecraft", "version": 1 },
            "username": login_name,
            "password": password.expose(),
            "clientToken": client_token.expose(),
            "requestUser": true,
        });
        let reply = post_json(&self.client, &endpoints.authenticate, &body, None).await?;
        if !reply.is_success() {
            return Err(rejected(&reply, &endpoints.authenticate, "登录"));
        }
        let session = session_from(&reply, client_token, &endpoints.authenticate)?;
        if !session.has_profile() {
            // 服务端没自动选角色（多角色服务器）：交给调用方走 refresh + selectedProfile。
            return Ok(session);
        }
        Ok(session)
    }

    /// 刷新（可顺带 `selectedProfile` 换角色）。
    ///
    /// # Errors
    ///
    /// token 失效 → [`AuthError::CredentialExpired`]；换角色失败 → [`AuthError::Malformed`]。
    pub async fn refresh(
        &self,
        api_root: &str,
        session: &AuthlibSession,
        select: Option<&AuthlibProfile>,
    ) -> AuthResult<AuthlibSession> {
        let endpoints = AuthlibEndpoints::from_api_root(api_root)?;
        let mut body = json!({
            "accessToken": session.access_token.expose(),
            "clientToken": session.client_token.expose(),
            "requestUser": true,
        });
        if let Some(profile) = select {
            body["selectedProfile"] = json!({ "id": profile.id, "name": profile.name });
        }
        let reply = post_json(&self.client, &endpoints.refresh, &body, None).await?;
        if !reply.is_success() {
            if is_forbidden(&reply) {
                return Err(AuthError::CredentialExpired);
            }
            return Err(rejected(&reply, &endpoints.refresh, "刷新"));
        }
        let refreshed = session_from(&reply, &session.client_token, &endpoints.refresh)?;
        if let Some(wanted) = select {
            let got = refreshed.selected_profile.as_ref();
            if got.map(|profile| profile.id.as_str()) != Some(wanted.id.as_str()) {
                return Err(AuthError::Malformed(format!(
                    "服务器没有切到角色 {}（refresh 未生效）",
                    wanted.name
                )));
            }
        }
        Ok(refreshed)
    }

    /// 校验 token 是否仍有效（`ForbiddenOperationException` = `Ok(false)`）。
    ///
    /// # Errors
    ///
    /// 服务器 5xx/网络失败时返回错误（调用方据此区分「要刷新」与「要重试」）。
    pub async fn validate(&self, api_root: &str, session: &AuthlibSession) -> AuthResult<bool> {
        let endpoints = AuthlibEndpoints::from_api_root(api_root)?;
        let body = json!({
            "accessToken": session.access_token.expose(),
            "clientToken": session.client_token.expose(),
        });
        let reply = post_json(&self.client, &endpoints.validate, &body, None).await?;
        if is_forbidden(&reply) {
            return Ok(false);
        }
        if !reply.is_success() {
            return Err(rejected(&reply, &endpoints.validate, "校验"));
        }
        // 成功时 body 为空或 `{}`，都算有效。
        Ok(true)
    }

    /// 主动注销（删账户时顺手清理服务端会话；失败按「尽力而为」返回错误，调用方可不理）。
    ///
    /// # Errors
    ///
    /// 网络/服务端失败时返回错误。
    pub async fn invalidate(&self, api_root: &str, session: &AuthlibSession) -> AuthResult<()> {
        let endpoints = AuthlibEndpoints::from_api_root(api_root)?;
        let body = json!({
            "accessToken": session.access_token.expose(),
            "clientToken": session.client_token.expose(),
        });
        let reply = post_json(&self.client, &endpoints.invalidate, &body, None).await?;
        if reply.is_success() || is_forbidden(&reply) {
            return Ok(());
        }
        Err(rejected(&reply, &endpoints.invalidate, "注销"))
    }
}

/// 响应 → 会话（`clientToken` 回传必须一致；缺失时保留原值）。
fn session_from(reply: &Reply, client_token: &Secret, url: &str) -> AuthResult<AuthlibSession> {
    let access_token = reply
        .text("accessToken")
        .ok_or_else(|| AuthError::Malformed(format!("{url} 响应缺 accessToken")))?;
    if let Some(returned) = reply.text("clientToken") {
        if returned != client_token.expose() {
            return Err(AuthError::Malformed(
                "服务器回传的 clientToken 与本地不一致（会话可能被替换）".to_string(),
            ));
        }
    }
    Ok(AuthlibSession {
        access_token: Secret::new(access_token),
        client_token: client_token.clone(),
        selected_profile: selected_profile(reply),
    })
}

fn selected_profile(reply: &Reply) -> Option<AuthlibProfile> {
    let profile = reply
        .body
        .as_ref()
        .and_then(|body| body.get("selectedProfile"))?;
    let id = profile.get("id")?.as_str()?;
    let name = profile.get("name")?.as_str()?;
    Some(AuthlibProfile {
        id: id.to_string(),
        name: name.to_string(),
    })
}

fn is_forbidden(reply: &Reply) -> bool {
    reply.text("error") == Some("ForbiddenOperationException")
}

fn rejected(reply: &Reply, url: &str, what: &str) -> AuthError {
    match reply.text("error") {
        Some(code) => AuthError::Denied {
            code: code.to_string(),
            description: reply
                .text("errorMessage")
                .unwrap_or("服务器未给描述")
                .to_string(),
        },
        None => AuthError::network(
            url.split('?').next().unwrap_or(url),
            format!("{what}失败（HTTP {}）", reply.status),
        ),
    }
}

#[cfg(test)]
mod tests;
