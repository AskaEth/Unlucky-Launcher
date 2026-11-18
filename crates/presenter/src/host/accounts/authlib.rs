//! 第三方轨（authlib-injector）：服务发现校验、密码登录、皮肤原图抓取。
//!
//! 全部跑在后台线程里（网络），出口只有 [`AuthlibLoginView`] / [`SkinImage`]——
//! 密码与 accessToken 在调用栈里就地丢弃，永不进 DTO 回程（M114 凭据红线）。
//!
//! ## 三条流的坐标
//!
//! | 动作 | 端点 | 产出 |
//! |---|---|---|
//! | 服务发现（「校验」按钮） | `GET {apiRoot}` | 服务器名 / skinDomains（`meta` 缺失即判「不是认证服务器」） |
//! | 登录 | `POST {apiRoot}authserver/authenticate` | `AuthlibSession`（入库，不外传） |
//! | 皮肤 | `GET {apiRoot}sessionserver/session/minecraft/profile/{uuid}` → textures → 皮肤 URL | 皮肤 PNG 原图 |
//!
//! `core::download::http::build_client` 是唯一的 HTTP 基座（复用下载器的客户端配置），
//! 所以本 crate **不新增任何依赖**（`presenter-no-ui` 与依赖最小化同时成立）。

use unlucky_core::auth::account::{AccountKind as CoreAccountKind, AccountMeta, Credentials};
use unlucky_core::auth::authlib::{
    normalize_api_root, AuthlibAuth, AuthlibServerMeta, AuthlibSession,
};
use unlucky_core::auth::microsoft::now_ms;
use unlucky_core::auth::{AccountStore, Secret};
use unlucky_core::download::blocking::block_on;

use crate::dto::{AuthlibLoginView, AuthlibRequest, LoginFailure, SkinImage};

mod util;

pub use util::scrub_known;

use util::{base64_decode, client_token, get_bytes, get_text};

use super::super::env::HostEnv;
use super::super::job::{spawn, HostTask, JobSender};
use super::LoginJob;

/// 校验认证服务器地址（「校验」按钮的后端）。
///
/// # Errors
///
/// 地址不合法、网络不可达、HTTP 非 2xx、响应不是 authlib-injector 的服务发现 JSON 时，
/// 返回**具名**的用户可读文案（这是任务要求的「失败态 + 具名错误」）。
pub fn probe_authlib(env: &HostEnv, input: &str) -> Result<(String, AuthlibServerMeta), Failure> {
    let api_root = normalize_api_root(input)
        .map_err(|error| Failure::new(LoginFailure::Other, error.to_string()))?;
    let (status, text) =
        get_text(env, &api_root).map_err(|error| Failure::new(LoginFailure::Unreachable, error))?;
    if !(200..300).contains(&status) {
        return Err(Failure::new(
            LoginFailure::NotAuthlibServer,
            format!("服务器返回 HTTP {status}（{api_root} 不像可用的 authlib 认证服务器）"),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|_| {
        Failure::new(
            LoginFailure::NotAuthlibServer,
            format!("响应不是 JSON：{api_root} —— 可能填成了网页地址，而不是 API 根（通常以 /api/yggdrasil/ 结尾）"),
        )
    })?;
    if value.get("meta").is_none() && value.get("skinDomains").is_none() {
        return Err(Failure::new(
            LoginFailure::NotAuthlibServer,
            "响应里没有 meta / skinDomains：这看起来不是 authlib-injector 服务器",
        ));
    }
    let meta = AuthlibServerMeta::from_value(&value);
    Ok((api_root, meta))
}

/// 第三方轨的失败：**原因码 + 原文**（原文进日志，页面据码出 i18n）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// 原因码
    pub code: LoginFailure,
    /// 原始诊断（已脱敏）
    pub detail: String,
}

impl Failure {
    fn new(code: LoginFailure, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    /// `AuthError` → 原因码（只看变体）。
    fn from_error(error: &unlucky_core::auth::AuthError) -> Self {
        use unlucky_core::auth::AuthError;
        let code = match error {
            AuthError::Network { .. } | AuthError::Timeout { .. } => LoginFailure::Unreachable,
            AuthError::Denied { .. } => LoginFailure::BadCredentials,
            AuthError::Store { .. } => LoginFailure::StoreFailed,
            _ => LoginFailure::Other,
        };
        Self::new(code, error.to_string())
    }
}

/// 起一次第三方轨登录（后台线程）。
pub fn start_login(env: HostEnv, tx: JobSender, job: LoginJob, request: AuthlibRequest) {
    spawn("authlib-login", move || {
        let mut request = request;
        // 日志只打脱敏形态；密码在下面用完立刻丢弃。
        eprintln!("[auth] authlib 登录开始（{}）", request.redacted());
        stage(&tx, &request.server, "正在校验服务器地址…");

        let (api_root, meta) = match probe_authlib(&env, &request.server) {
            Ok(found) => found,
            Err(failure) => return fail(&tx, failure),
        };
        let server_name = meta
            .name
            .clone()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| api_root.clone());
        stage(
            &tx,
            &api_root,
            &format!("服务器 {server_name} 可用，正在登录…"),
        );

        let auth = match AuthlibAuth::new() {
            Ok(auth) => auth,
            Err(error) => return fail(&tx, Failure::new(LoginFailure::Other, error.to_string())),
        };
        let client_token = Secret::new(client_token());
        let password_text = std::mem::take(&mut request.password);
        let password = Secret::new(password_text.clone());
        let login_name = request.login.clone();
        let result = block_on(auth.authenticate(&api_root, &login_name, &password, &client_token));
        drop(password);

        if job.is_cancelled() {
            let _ = tx.send(HostTask::Authlib(Box::default()));
            return;
        }
        match result {
            Ok(session) => match store(&env, session, &api_root, &login_name) {
                Ok(account) => {
                    let _ = tx.send(HostTask::Authlib(Box::new(AuthlibLoginView::succeeded(
                        &server_name,
                        &account,
                    ))));
                    let _ = tx.send(HostTask::Accounts(super::accounts(
                        &env,
                        Some(account.as_str()),
                    )));
                }
                Err(failure) => fail(&tx, failure),
            },
            // 错误原文里可能有服务端回显的凭据：先按**形状**脱敏（`AuthlibLoginView::failed`），
            // 再按**已知值**兜一遍（万一服务端把密码原样回显）。
            Err(error) => {
                let failure = Failure::from_error(&error);
                fail(
                    &tx,
                    Failure::new(failure.code, scrub_known(&failure.detail, &password_text)),
                );
            }
        }
        drop(password_text);
    });
}

/// 只校验服务器地址（「校验服务器」按钮）；结果走同一条 `AuthlibLoginChanged`。
pub fn spawn_probe(env: HostEnv, tx: JobSender, server: String) {
    spawn("authlib-probe", move || {
        stage(&tx, &server, "正在校验服务器地址…");
        match probe_authlib(&env, &server) {
            Ok((api_root, meta)) => {
                let name = meta
                    .name
                    .as_deref()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map_or_else(|| api_root.clone(), str::to_string);
                let _ = tx.send(HostTask::Authlib(Box::new(AuthlibLoginView::reachable(
                    &api_root, &name,
                ))));
            }
            Err(failure) => fail(&tx, failure),
        }
    });
}

/// 抓第三方轨账户的皮肤原图（`profile_id` → textures → 皮肤 URL → PNG 字节）。
///
/// # Errors
///
/// 服务器不可达、响应缺 textures、纹理不是 PNG 时返回原因（调用方按「没有皮肤」处理）。
pub fn fetch_skin(env: &HostEnv, api_root: &str, profile_id: &str) -> Result<SkinImage, String> {
    let base = normalize_api_root(api_root).map_err(|error| error.to_string())?;
    let url = format!("{base}sessionserver/session/minecraft/profile/{profile_id}");
    let (status, text) = get_text(env, &url)?;
    if !(200..300).contains(&status) {
        return Err(format!("皮肤接口返回 HTTP {status}"));
    }
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| format!("皮肤接口响应不是 JSON：{error}"))?;
    let encoded = value
        .get("properties")
        .and_then(|properties| properties.as_array())
        .and_then(|properties| {
            properties
                .iter()
                .find(|entry| entry.get("name").and_then(|name| name.as_str()) == Some("textures"))
        })
        .and_then(|entry| entry.get("value"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| "档案里没有 textures 属性".to_string())?;
    let raw = base64_decode(encoded)?;
    let textures: serde_json::Value =
        serde_json::from_slice(&raw).map_err(|error| format!("textures 属性解不开：{error}"))?;
    let skin_url = textures
        .get("textures")
        .and_then(|textures| textures.get("SKIN"))
        .and_then(|skin| skin.get("url"))
        .and_then(|url| url.as_str())
        .ok_or_else(|| "textures 里没有 SKIN 条目".to_string())?;
    let (status, bytes) = get_bytes(env, skin_url)?;
    if !(200..300).contains(&status) {
        return Err(format!("皮肤图片返回 HTTP {status}"));
    }
    if !bytes.starts_with(b"\x89PNG") {
        return Err("皮肤图片不是 PNG".to_string());
    }
    Ok(SkinImage {
        png: bytes,
        source: skin_url.to_string(),
    })
}

/// 入库（元数据 + 凭据分文件）；返回展示名。
fn store(
    env: &HostEnv,
    session: AuthlibSession,
    api_root: &str,
    login_name: &str,
) -> Result<String, Failure> {
    let profile = session
        .selected_profile
        .clone()
        .filter(|profile| !profile.name.trim().is_empty() && !profile.id.is_empty())
        .ok_or_else(|| Failure::new(LoginFailure::NoProfile, "服务器没有返回角色"))?;
    let mut meta = AccountMeta::new(
        CoreAccountKind::AuthlibInjector,
        profile.id,
        profile.name,
        now_ms(),
    );
    meta.login_name = Some(login_name.to_string());
    meta.server_url = Some(api_root.to_string());
    let name = meta.display_name().to_string();
    let mut account_store = AccountStore::open(env.layout.auth_data());
    account_store
        .insert(meta, Credentials::AuthlibInjector(session))
        .and_then(|()| account_store.save())
        .map_err(|error| Failure::new(LoginFailure::StoreFailed, error.to_string()))?;
    Ok(name)
}

/// 发一条「进行中」状态（UI 据此只转圈不误报）。
fn stage(tx: &JobSender, server: &str, message: &str) {
    let _ = tx.send(HostTask::Authlib(Box::new(AuthlibLoginView::running(
        server, message,
    ))));
}

/// 失败收尾：**先脱敏再上屏**（服务端错误原文里可能带 token）。
/// 失败收尾：原文进日志，页面拿**原因码**（复审 r1 P1-1）。
fn fail(tx: &JobSender, failure: Failure) {
    eprintln!(
        "[auth] authlib 失败（{}）：{}",
        failure.code.code(),
        failure.detail
    );
    let _ = tx.send(HostTask::Authlib(Box::new(AuthlibLoginView::failed_as(
        failure.code,
        failure.detail,
    ))));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_decodes_textures_payload() {
        // `{"textures":{}}` 的 base64
        let encoded = "eyJ0ZXh0dXJlcyI6e319";
        assert_eq!(base64_decode(encoded).expect("解码"), br#"{"textures":{}}"#);
        // URL-safe 变体 + 填充 + 换行都要吃
        assert_eq!(
            base64_decode("eyJ0ZXh0dXJlcyI6e319\n").expect("解码"),
            br#"{"textures":{}}"#
        );
        assert!(base64_decode("!!!").is_err());
        assert!(base64_decode("").is_err());
    }

    #[test]
    fn client_token_is_stable_length_and_unique() {
        let a = client_token();
        let b = client_token();
        assert_eq!(a.len(), 32);
        assert_ne!(
            a, b,
            "两次生成的 clientToken 不能相同（会被服务端当成重放）"
        );
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    use crate::dto::REDACTED;

    #[test]
    fn known_secret_is_scrubbed_from_error_text() {
        let text = scrub_known("登录失败：hunter2 被拒绝", "hunter2");
        assert!(!text.contains("hunter2"), "{text}");
        assert!(text.contains(REDACTED));
        assert_eq!(scrub_known("普通错误", ""), "普通错误");
    }

    #[test]
    fn a_web_page_address_is_rejected_with_a_named_error() {
        // 不发网络请求：只测「规范化 + 空输入」这条不联网的分支
        let dir = std::env::temp_dir().join(format!("ull-authlib-probe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let env = HostEnv::at_root(&dir);
        std::fs::create_dir_all(env.layout.cache()).expect("cache");
        let failure = probe_authlib(&env, "   ").expect_err("空地址必须报错");
        assert!(failure.detail.contains("不能为空"), "{failure:?}");
        let failure = probe_authlib(&env, "https://").expect_err("只给协议头也必须报错");
        assert!(!failure.detail.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}
