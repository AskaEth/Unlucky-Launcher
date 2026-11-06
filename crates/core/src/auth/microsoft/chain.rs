//! 微软五步链：Live token → **XBL** → **XSTS** → **login_with_xbox** → **entitlements** → **profile**。
//!
//! 逐条承 `analyse/04` §5.2（报文级别），四处容易抄错的地方都在注释里点名：
//! 1. XBL 的 `RpsTicket` 必须带 **`d=` 前缀**（MSA 消费版票据标记）；
//! 2. `identityToken` 是 **`XBL3.0 x=<uhs>;<xsts>`**（分号分隔，`x` 后紧跟 `=`）；
//! 3. XSTS 在「无 Xbox 档案/儿童账号/地区不支持」时回 **401**（不是 200+JSON），
//!    必须读 body 里的 `XErr` 才能给出准确原因；
//! 4. `/minecraft/profile` 的 **404 二义**：没买游戏 vs 买了没建档案，
//!    要用 `entitlements/license` 复判一次才分得清。
//!
//! **XSTS/MC 令牌不缓存**：刷新就是整链重跑（`analyse/04` §5.1），所以本模块只做无状态串联。

use reqwest::Client;
use serde_json::json;

use crate::auth::body;
use crate::auth::error::AuthError;
use crate::auth::secret::{redactor_for, Secret};

use super::endpoints::MicrosoftEndpoints;
use super::flow::OAuthTokens;
use super::session::MicrosoftSession;
use crate::auth::http::{get_json, post_json, Reply};

/// Xbox Live 用户票据 + 用户哈希。
#[derive(Debug, Clone)]
pub(crate) struct XboxTicket {
    /// JWT 票据（凭据形状）。
    pub token: Secret,
    /// `uhs`（`XBL3.0 x=<uhs>;...` 的 userhash；本身不是凭据但只在本链内部用）。
    pub uhs: String,
}

/// MC 侧换到的令牌。
#[derive(Debug, Clone)]
pub(crate) struct MinecraftToken {
    /// MC access token（落盘）。
    pub access_token: Secret,
    /// 有效期（秒）。
    pub expires_in: u64,
}

/// Minecraft 档案（不可变 id + 可变 name）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GameProfile {
    /// 32 位无连字符 UUID。
    pub id: String,
    /// 玩家名。
    pub name: String,
}

/// 第 1 步：Xbox Live User Authenticate。
pub(crate) async fn xbl_authenticate(
    client: &Client,
    endpoints: &MicrosoftEndpoints,
    live_access_token: &Secret,
) -> Result<XboxTicket, AuthError> {
    let body = json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": format!("d={}", live_access_token.expose()),
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT",
    });
    let reply = post_json(client, &endpoints.xbl_authenticate, &body, None).await?;
    if let Some(code) = xerr(&reply) {
        return Err(AuthError::Xbox { code });
    }
    if !reply.is_success() {
        // M38：XErr 优先（业务码才是真原因），没有 XErr 时把正文诊断补进文案。
        return Err(body::response(
            "xbl",
            &reply,
            "Xbox Live 用户认证被拒",
            &redactor_for([live_access_token]),
        ));
    }
    let token = reply
        .text("Token")
        .ok_or_else(|| AuthError::Malformed("XBL 响应缺 Token".to_string()))?;
    let uhs = uhs_of(&reply)
        .ok_or_else(|| AuthError::Malformed("XBL 响应缺 DisplayClaims.xui[0].uhs".to_string()))?;
    Ok(XboxTicket {
        token: Secret::new(token),
        uhs,
    })
}

/// 第 2 步：XSTS Authorize（401 也读 body，`XErr` 才是真原因）。
pub(crate) async fn xsts_authorize(
    client: &Client,
    endpoints: &MicrosoftEndpoints,
    xbl: &XboxTicket,
) -> Result<XboxTicket, AuthError> {
    let body = json!({
        "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl.token.expose()] },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT",
    });
    let reply = post_json(client, &endpoints.xsts_authorize, &body, None).await?;
    if let Some(code) = xerr(&reply) {
        return Err(AuthError::Xbox { code });
    }
    if !reply.is_success() {
        return Err(body::response(
            "xsts",
            &reply,
            "XSTS 授权被拒",
            &redactor_for([&xbl.token]),
        ));
    }
    let token = reply
        .text("Token")
        .ok_or_else(|| AuthError::Malformed("XSTS 响应缺 Token".to_string()))?;
    let uhs = uhs_of(&reply).ok_or_else(|| AuthError::Malformed("XSTS 响应缺 uhs".to_string()))?;
    // 一致性断言：同一账号在 XBL 与 XSTS 两次返回的 userhash 必须相同。
    if uhs != xbl.uhs {
        return Err(AuthError::Malformed(
            "XBL 与 XSTS 返回的 userhash 不一致".to_string(),
        ));
    }
    Ok(XboxTicket {
        token: Secret::new(token),
        uhs,
    })
}

/// 第 3 步：`login_with_xbox` 换 Minecraft 令牌。
pub(crate) async fn login_with_xbox(
    client: &Client,
    endpoints: &MicrosoftEndpoints,
    xsts: &XboxTicket,
) -> Result<MinecraftToken, AuthError> {
    let body = json!({
        "identityToken": format!("XBL3.0 x={};{}", xsts.uhs, xsts.token.expose()),
    });
    let reply = post_json(client, &endpoints.login_with_xbox, &body, None).await?;
    if !reply.is_success() {
        // M38 的核心一行：403 到底是「拦截页」还是「服务端拒绝」，文案要分开引导。
        return Err(body::response(
            "login_with_xbox",
            &reply,
            "Minecraft 登录被拒",
            &redactor_for([&xsts.token]),
        ));
    }
    let access_token = reply
        .text("access_token")
        .ok_or_else(|| AuthError::Malformed("login_with_xbox 缺 access_token".to_string()))?;
    Ok(MinecraftToken {
        access_token: Secret::new(access_token),
        expires_in: reply.number("expires_in").unwrap_or(0),
    })
}

/// 第 4 步：拥有权校验（必须 200，否则后面启动时也会被服务端踢）。
pub(crate) async fn check_entitlements(
    client: &Client,
    endpoints: &MicrosoftEndpoints,
    mc_token: &Secret,
) -> Result<(), AuthError> {
    let reply = get_json(client, &endpoints.entitlements_mcstore, mc_token).await?;
    if reply.is_success() {
        return Ok(());
    }
    let detail = body::rejection(
        "entitlements",
        &reply,
        "拥有权校验未通过",
        &redactor_for([mc_token]),
    );
    // 拦截页（HTML/纯文本）与非 404 的空体先当网络问题报，别让用户去查许可；
    // JSON 拒绝与 404 沿用 M31 的 `no_entitlement`（稳定码不改）。
    let interception = detail.kind.is_interception()
        || (detail.kind == body::BodyKind::Empty && reply.status != 404);
    if interception {
        return Err(AuthError::Response(Box::new(detail)));
    }
    let what = if reply.status == 404 {
        "该账号没有 Minecraft Java 版拥有权"
    } else {
        detail.what.as_str()
    };
    Err(AuthError::Denied {
        code: "no_entitlement".to_string(),
        description: format!(
            "{what}（entitlements/mcstore 返回 HTTP {}）{}",
            reply.status,
            detail.note()
        ),
    })
}

/// 第 5 步：拉档案（404 用 license 复判二义）。
pub(crate) async fn fetch_profile(
    client: &Client,
    endpoints: &MicrosoftEndpoints,
    mc_token: &Secret,
) -> Result<GameProfile, AuthError> {
    let reply = get_json(client, &endpoints.profile, mc_token).await?;
    // 只有「像服务端应答」的 404 才走二义复判：HTML/纯文本的 404 是网关页，
    // 拿它去复判 license 会得出错误的「没买游戏」结论。
    if reply.status == 404 && !body::classify(&reply).is_interception() {
        return Err(profile_missing(client, endpoints, mc_token).await);
    }
    if !reply.is_success() {
        return Err(body::response(
            "profile",
            &reply,
            "拉取 Minecraft 档案被拒",
            &redactor_for([mc_token]),
        ));
    }
    let id = reply
        .text("id")
        .ok_or_else(|| AuthError::Malformed("档案响应缺 id".to_string()))?;
    let name = reply
        .text("name")
        .ok_or_else(|| AuthError::Malformed("档案响应缺 name".to_string()))?;
    Ok(GameProfile {
        id: id.to_string(),
        name: name.to_string(),
    })
}

/// 404 之后的二义消解：没买游戏 vs 买了没建档案。
async fn profile_missing(
    client: &Client,
    endpoints: &MicrosoftEndpoints,
    mc_token: &Secret,
) -> AuthError {
    let reply = match get_json(client, &endpoints.entitlements_license, mc_token).await {
        Ok(reply) => reply,
        // 复判本身失败时，退回「档案不存在」这个更保守的说法。
        Err(_) => {
            return AuthError::Denied {
                code: "no_profile".to_string(),
                description: "该账号没有 Minecraft 档案（复判 license 也失败）".to_string(),
            }
        }
    };
    if has_minecraft_license(&reply) {
        AuthError::Denied {
            code: "no_profile".to_string(),
            description: "账号已拥有 Minecraft，但还没有创建档案（去 minecraft.net 起个名字）"
                .to_string(),
        }
    } else {
        AuthError::Denied {
            code: "no_license".to_string(),
            description: "该账号没有购买 Minecraft Java 版".to_string(),
        }
    }
}

/// `entitlements/license` 响应里是否有 `game_minecraft`。
fn has_minecraft_license(reply: &Reply) -> bool {
    reply
        .body
        .as_ref()
        .and_then(|body| body.get("items"))
        .and_then(|items| items.as_array())
        .is_some_and(|items| {
            items.iter().any(|item| {
                item.get("name")
                    .and_then(|name| name.as_str())
                    .is_some_and(|name| name == "game_minecraft")
            })
        })
}

/// 整链：Live token → [`MicrosoftSession`]（刷新与新登录共用这一段）。
pub(crate) async fn complete_chain(
    client: &Client,
    endpoints: &MicrosoftEndpoints,
    tokens: OAuthTokens,
    now_ms: u64,
) -> Result<MicrosoftSession, AuthError> {
    let xbl = xbl_authenticate(client, endpoints, &tokens.access_token).await?;
    let xsts = xsts_authorize(client, endpoints, &xbl).await?;
    let mc = login_with_xbox(client, endpoints, &xsts).await?;
    check_entitlements(client, endpoints, &mc.access_token).await?;
    let profile = fetch_profile(client, endpoints, &mc.access_token).await?;
    Ok(MicrosoftSession::new(
        profile.id,
        profile.name,
        mc.access_token,
        tokens.refresh_token,
        mc.expires_in,
        now_ms,
        None,
    ))
}

/// 从响应里取 `XErr`（XSTS 的 401 业务码）。
fn xerr(reply: &Reply) -> Option<u32> {
    let body = reply.body.as_ref()?;
    for key in ["XErr", "errorCode"] {
        if let Some(code) = body.get(key).and_then(|value| value.as_u64()) {
            if code != 0 {
                return Some(code as u32);
            }
        }
    }
    None
}

fn uhs_of(reply: &Reply) -> Option<String> {
    reply
        .body
        .as_ref()?
        .get("DisplayClaims")?
        .get("xui")?
        .as_array()?
        .first()?
        .get("uhs")?
        .as_str()
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::{has_minecraft_license, uhs_of, xerr, Reply};
    use serde_json::{json, Value};

    fn reply(status: u16, body: Option<Value>) -> Reply {
        Reply {
            status,
            body,
            content_type: Some("application/json".to_string()),
            raw: None,
        }
    }

    #[test]
    fn xerr_is_read_from_401_body() {
        let sample = reply(
            401,
            Some(json!({ "Identity": "0", "XErr": 2_148_916_238_u32, "Message": "" })),
        );
        assert_eq!(xerr(&sample), Some(2_148_916_238));
        assert_eq!(xerr(&reply(200, Some(json!({ "XErr": 0 })))), None);
        assert_eq!(xerr(&reply(200, None)), None);
    }

    #[test]
    fn uhs_comes_from_first_xui_entry() {
        let sample = reply(
            200,
            Some(json!({ "DisplayClaims": { "xui": [{ "uhs": "1234" }] } })),
        );
        assert_eq!(uhs_of(&sample).as_deref(), Some("1234"));
        assert_eq!(
            uhs_of(&reply(200, Some(json!({ "DisplayClaims": {} })))),
            None
        );
    }

    #[test]
    fn license_detection() {
        let with = reply(
            200,
            Some(
                json!({ "items": [{ "name": "product_minecraft" }, { "name": "game_minecraft" }] }),
            ),
        );
        let without = reply(
            200,
            Some(json!({ "items": [{ "name": "product_minecraft" }] })),
        );
        assert!(has_minecraft_license(&with));
        assert!(!has_minecraft_license(&without));
        assert!(!has_minecraft_license(&reply(500, None)));
    }
}
