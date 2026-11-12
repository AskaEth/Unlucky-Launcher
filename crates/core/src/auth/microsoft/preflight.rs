//! 授权码流的「回环重定向 URI 注册预检」（M178）。
//!
//! ## 为什么需要预检
//!
//! 授权码流把浏览器送回 `http://localhost:{port}/auth`。**随机端口**只有在「回环 URI 的
//! 端口不参与匹配」这件事成立时才可能被接受——微软官方文档写得很清楚
//! （[*Redirect URI (reply URL) best practices and limitations*](https://learn.microsoft.com/en-us/entra/identity-platform/reply-url)，
//! 小节 *Localhost exceptions*）：
//!
//! > "Due to ephemeral port ranges often required by native applications, the port component
//! > (for example, :5001 or :443) is ignored for the purposes of matching a localhost redirect URI.
//! > As a result, all of these URIs are considered equivalent: `http://localhost/MyApp`,
//! > `http://localhost:1234/MyApp`, `http://localhost:5000/MyApp`, `http://localhost:8080/MyApp`"
//!
//! 换句话说：**注册一次 `http://localhost/auth` 就能接住任意端口**；但**一次都没注册**时，
//! 任何端口的回环 URI 都会被授权端点当场拒掉（`invalid_request: ... 'redirect_uri' is not valid`），
//! 而且**不会**再跳回本地回调——用户只在浏览器里看到一张 AAD 错误页，启动器只能干等超时。
//! 本项目的 Azure 应用注册当初是为 device-code 建的，重定向 URI 一项都没登记
//! （`DevDocs/ops/azure-msa-app.md`），所以这条流在官方产物上必然失败。
//!
//! ## 预检做什么、不做什么
//!
//! 做：拿**即将使用的那个** `redirect_uri`（只有端口可以是任意的，所以用固定探测端口拼形状）
//! 去授权端点发一次**不带凭据的 GET**，看它是否当场报 `invalid_request` / `AADSTS50011`。
//! 不注册 → 不绑本地端口、不弹浏览器，直接回退设备码。这样「没登记回调地址」的机器上
//! 监听口一次都不开（守卫断言 `LoginOutcome::listeners_opened == 0`，那个数是
//! [`super::callback::CallbackServer::bind_counting`] 在 `TcpListener::bind` 现场加的）。
//!
//! 不做：预检失败**不等于**登录被拒——它只决定「要不要试浏览器」。
//! `Unknown`（网络问题/整体超时）同样按「别试」处理：设备码流不依赖任何注册项，永远更稳。
//! 整次预检有 [`PREFLIGHT_TIMEOUT`] 的上限：服务器接了连接却不出响应时，不能让「浏览器登录」
//! 按钮把界面卡在 Requesting（取消按钮只收面板、停不掉已经跑起来的线程）。

use reqwest::Client;

use crate::auth::error::AuthError;

use super::authcode::{authorization_url, RedirectUri};
use super::endpoints::MicrosoftEndpoints;

/// 预检用的回环端口。
///
/// **不是要绑它**：端口对回环匹配不参与（见模块文档引的那段官方原文），所以随便一个
/// 「看起来像临时端口」的值就能代表真实绑定后的那个。固定值让日志与测试可复现。
pub const PROBE_PORT: u16 = 53_691;

/// 预检请求里的 `state`（不参与任何校验，只为拼出形状完整的授权 URL）。
const PROBE_STATE: &str = "ull-redirect-probe";

/// 整次预检的时间上限（复审 r1 P2-2）。
///
/// 为什么必须有：下载引擎那个 HTTP 客户端只设了 `connect_timeout`（10s），**没有**整体超时
/// ——`read_timeout` 从未应用到 reqwest 客户端。授权端点接了连接却不出响应（或 header 后停住）时，
/// 这一次「用浏览器登录」会把界面卡在 Requesting，而取消按钮只收面板、停不掉已经跑起来的线程。
/// 超时按 [`RedirectRegistration::Unknown`] 处理：回退设备码，原因码「没确认」。
pub const PREFLIGHT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// 授权端点对 `redirect_uri` 的态度。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedirectRegistration {
    /// 注册里有这个回环 URI（端口随意）→ 可以真的走授权码流。
    Registered,
    /// 明确没登记（`invalid_request` / `AADSTS50011`）→ 别试，直接设备码。
    NotRegistered {
        /// 诊断原文（只进日志）
        detail: String,
    },
    /// 问不出来（网络/超时/非预期应答）→ 同样不试，但原因是「没确认」而不是「没配」。
    Unknown {
        /// 诊断原文（只进日志）
        detail: String,
    },
}

impl RedirectRegistration {
    /// 是否值得继续走授权码流。
    #[must_use]
    pub fn is_registered(&self) -> bool {
        matches!(self, Self::Registered)
    }
}

/// 预检：注册里有没有这个回环重定向 URI。
///
/// 判据（顺序即优先级）：
/// 1. 应答里出现 `invalid_request` / `AADSTS50011` / `redirect_uri is not valid` 一类字样 → 没登记；
/// 2. 非 2xx/3xx → 按「没登记」处理（AAD 对不合法的 redirect_uri 直接渲染错误页，状态码 400）；
/// 3. 其余 2xx/3xx 且没有拒绝字样 → 认为已登记（那就是登录页本身）；
/// 4. 连不上 → `Unknown`。
///
/// 阅读提示：这条判据**宁可错杀**——错过一次浏览器登录只是慢一步，误放行会让用户撞上
/// AAD 的原始错误页，而那正是本 mission 要消灭的现象。
pub(super) async fn check(
    client: &Client,
    endpoints: &MicrosoftEndpoints,
    client_id: &str,
    redirect_uri: &RedirectUri,
) -> RedirectRegistration {
    check_with_deadline(
        client,
        endpoints,
        client_id,
        redirect_uri,
        PREFLIGHT_TIMEOUT,
    )
    .await
}

/// [`check`] 的可注入 deadline 版本（单测用短 deadline，不必等 10 秒）。
pub(super) async fn check_with_deadline(
    client: &Client,
    endpoints: &MicrosoftEndpoints,
    client_id: &str,
    redirect_uri: &RedirectUri,
    deadline: std::time::Duration,
) -> RedirectRegistration {
    match tokio::time::timeout(deadline, probe(client, endpoints, client_id, redirect_uri)).await {
        Ok(verdict) => verdict,
        Err(_) => RedirectRegistration::Unknown {
            detail: format!(
                "预检 {} 毫秒内没有结论，按「没确认」处理",
                deadline.as_millis()
            ),
        },
    }
}

/// 真正的预检请求（不含超时；由 [`check_with_deadline`] 套 deadline）。
async fn probe(
    client: &Client,
    endpoints: &MicrosoftEndpoints,
    client_id: &str,
    redirect_uri: &RedirectUri,
) -> RedirectRegistration {
    let url = authorization_url(endpoints, client_id, redirect_uri, PROBE_STATE);
    let response = client.get(&url).send().await;
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            return RedirectRegistration::Unknown {
                detail: format!("预检请求失败：{error}"),
            }
        }
    };
    let status = response.status().as_u16();
    // 登录页可能带上几十上百 KB 的脚本；拒绝页只有一小段。读整页是为了抓正文里的错误码，
    // reqwest 的 `text()` 会把正文一次读尽（授权页不含任何凭据，不需要脱敏）。
    let body = response.text().await.unwrap_or_default();
    if rejects_redirect_uri(&body) {
        return RedirectRegistration::NotRegistered {
            detail: format!("HTTP {status}：{}", snippet(&body)),
        };
    }
    if !(200..400).contains(&status) {
        return RedirectRegistration::NotRegistered {
            detail: format!("HTTP {status}：{}", snippet(&body)),
        };
    }
    RedirectRegistration::Registered
}

/// 这个错误是不是「重定向 URI 没登记」那一类（`AuthError` 侧重判，回调/换 token 用）。
///
/// 与 [`rejects_redirect_uri`] 同源：一个看页面正文，一个看结构化错误。两者都覆盖
/// `invalid_request` 与 `AADSTS50011`，因为同一件事在两条流上的形状不一样。
#[must_use]
pub(super) fn error_means_unregistered(error: &AuthError) -> bool {
    match error {
        AuthError::Denied { code, description } => {
            is_redirect_code(code) || redirects_rejected(description)
        }
        AuthError::Response(detail) => {
            redirects_rejected(detail.server_message.as_deref().unwrap_or_default())
                || redirects_rejected(detail.digest.as_str())
        }
        _ => false,
    }
}

fn is_redirect_code(code: &str) -> bool {
    matches!(
        code,
        "invalid_request"
            | "invalid_redirect_uri"
            | "redirect_uri_mismatch"
            | "unauthorized_client"
    )
}

fn redirects_rejected(text: &str) -> bool {
    let lowered = text.to_lowercase();
    lowered.contains("aadsts50011")
        || lowered.contains("invalid_request")
        || lowered.contains("redirect_uri is not valid")
        || lowered.contains("redirect_uri+is+not+valid")
        || lowered.contains("reply url specified in the request does not match")
        || (lowered.contains("redirect") && lowered.contains("not valid"))
}

/// 页面正文里有没有「重定向 URI 不合法」的拒绝标记。
fn rejects_redirect_uri(body: &str) -> bool {
    let lowered = body.to_lowercase();
    lowered.contains("aadsts50011")
        || lowered.contains("invalid_request")
        || lowered.contains("redirect_uri is not valid")
        || lowered.contains("redirect_uri+is+not+valid")
        || (lowered.contains("redirect") && lowered.contains("not valid"))
}

/// 正文摘要（单行、截断）：只进日志，不上屏。
fn snippet(body: &str) -> String {
    let one_line: String = body
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let collapsed = one_line.split_whitespace().collect::<Vec<_>>().join(" ");
    match collapsed.char_indices().nth(200) {
        Some((index, _)) => format!("{}…", &collapsed[..index]),
        None => collapsed,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        check_with_deadline, error_means_unregistered, rejects_redirect_uri, snippet,
        RedirectRegistration,
    };
    use crate::auth::body::ResponseDetail;
    use crate::auth::error::AuthError;
    use crate::auth::microsoft::{MicrosoftEndpoints, RedirectUri};

    /// 用户 2026-11-08 实测的那张错误页（原文截取）必须被判成「没登记」。
    #[test]
    fn the_reported_aad_page_is_recognized_as_unregistered() {
        let page = "error=invalid_request&error_description=The+provided+value+for+the+input+\
parameter+%27redirect_uri%27+is+not+valid.+The+expected+value+is+a+URI+which+matches+a+\
redirect+URI+registered+for+this+client+application.";
        assert!(rejects_redirect_uri(page));
        // 登录页本身（有脚本、有表单）不能被误判成拒绝。
        assert!(!rejects_redirect_uri(
            "<html><head><title>Sign in to your account</title></head><body>...</body></html>"
        ));
    }

    #[test]
    fn aadsts50011_is_also_a_registration_problem() {
        assert!(rejects_redirect_uri(
            "AADSTS50011: The redirect URI 'http://localhost:61492/auth' specified in the request does not match"
        ));
    }

    /// 两条流上的同一件事：授权页拒绝（`Denied`）与 token 端点拒绝（`Response`）都要认。
    #[test]
    fn structured_errors_of_both_shapes_are_recognized() {
        assert!(error_means_unregistered(&AuthError::Denied {
            code: "invalid_request".to_string(),
            description: "bad redirect".to_string(),
        }));
        assert!(error_means_unregistered(&AuthError::Response(Box::new(
            ResponseDetail {
                step: "token".to_string(),
                what: "换 token".to_string(),
                status: 400,
                content_type: None,
                kind: crate::auth::BodyKind::Json,
                server_message: Some("AADSTS50011: reply url mismatch".to_string()),
                digest: "digest".to_string(),
            }
        ))));
        // 不相干的失败不是注册问题（别把网络/白名单也归到这条）。
        assert!(!error_means_unregistered(&AuthError::Canceled));
    }

    #[test]
    fn registration_state_is_answered_by_one_predicate() {
        assert!(RedirectRegistration::Registered.is_registered());
        assert!(!RedirectRegistration::NotRegistered {
            detail: String::new()
        }
        .is_registered());
        assert!(!RedirectRegistration::Unknown {
            detail: String::new()
        }
        .is_registered());
    }

    /// **复审 r1 P2-2**：服务器接了连接却永不出响应时，预检必须靠**整体 deadline** 收场
    /// （不是 connect_timeout：连接早就建好了），结论是「没确认」⇒ 上层回退设备码。
    ///
    /// 抓得住：把 `tokio::time::timeout` 去掉（那条路会一直挂着，本用例在 deadline 后
    /// 断言超时不出来，必然失败）。
    /// 抓不住：真机上网关的其它卡法（这里只有一个「不回字」的假服务器）。
    #[tokio::test]
    async fn a_silent_authorize_server_hits_the_overall_deadline() {
        // 假授权端点：accept 之后一个字节都不写（connect 成功、读永远等不到）。
        let silent = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("假服务器起不来");
        let port = silent.local_addr().expect("地址").port();
        tokio::spawn(async move {
            let mut held = Vec::new();
            loop {
                let Ok((stream, _)) = silent.accept().await else {
                    break;
                };
                held.push(stream);
            }
        });

        let client =
            crate::download::http::build_client(&crate::download::config::EngineConfig::default())
                .expect("HTTP 客户端");
        let mut endpoints = MicrosoftEndpoints::all_on("http://127.0.0.1:1");
        endpoints.authorize = format!("http://127.0.0.1:{port}/authorize");
        let verdict = check_with_deadline(
            &client,
            &endpoints,
            "test-client-id",
            &RedirectUri::new(53_691),
            std::time::Duration::from_millis(200),
        )
        .await;
        match verdict {
            RedirectRegistration::Unknown { detail } => {
                assert!(detail.contains("200"), "文案要带 deadline：{detail}");
            }
            other => panic!("静默服务器必须走 Unknown，实际 {other:?}"),
        }
    }

    #[test]
    fn snippet_is_single_line_and_capped() {
        let long = format!("a\nb\t{}", "x".repeat(400));
        let text = snippet(&long);
        assert!(!text.contains('\n'), "{text}");
        assert!(text.ends_with('…'), "{text}");
        assert_eq!(text.chars().count(), 201);
    }
}
