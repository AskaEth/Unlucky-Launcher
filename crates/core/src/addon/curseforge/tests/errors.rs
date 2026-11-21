//! Key 缺失 / 401 / 403 / 429 / 网络 / 解析失败六档，以及重试的上下界（纯离线）。

use crate::addon::curseforge::{
    ApiKey, CfError, CfErrorKind, CfKeyState, CfResponse, CurseForge, ModQuery, RetryPolicy, CF_API,
};
use crate::download::blocking::block_on;

use super::fixtures;
use super::support::{fast_policy, key, FixtureTransport, TEST_KEY};

fn search_prefix() -> String {
    format!("{CF_API}/mods/search")
}

#[test]
fn a_missing_key_is_a_named_state_and_sends_no_request() {
    let transport = FixtureTransport::new();
    let client = CurseForge::new(&transport);
    assert_eq!(client.key_state(), CfKeyState::Missing);
    assert!(!client.key_state().is_configured());

    let error = block_on(client.search(&ModQuery::default())).expect_err("缺 Key 必须失败");
    assert_eq!(
        error.kind(),
        &CfErrorKind::NotConfigured,
        "缺 Key 是**具名**状态，不是空列表也不是静默 403"
    );
    assert!(error.is_not_configured(), "上层要能单独认出这一档");
    assert!(
        error.is_unavailable(),
        "缺 Key 与 Key 无效都属于「源不可用」"
    );
    assert!(!error.is_retryable(), "再看一眼 Key 也没用，不该重试");
    assert_eq!(transport.calls(), 0, "缺 Key 时一个字节都不许发出去");
    assert!(!error.message().is_empty());
}

#[test]
fn every_endpoint_gates_on_the_key_before_the_network() {
    let transport = FixtureTransport::new();
    let client = CurseForge::new(&transport);
    let file = crate::addon::curseforge::ModFile {
        id: 1,
        mod_id: 2,
        ..Default::default()
    };
    block_on(client.project(394_468)).expect_err("详情也要闸门");
    block_on(client.file(2, 1)).expect_err("单文件也要闸门");
    block_on(client.files(&crate::addon::curseforge::ModFileQuery::new(2)))
        .expect_err("文件列表也要闸门");
    block_on(client.download_url(&file)).expect_err("下载地址也要闸门");
    assert_eq!(
        transport.calls(),
        0,
        "四条端点都不许在缺 Key 时发请求（否则用户看到的是 401，不知道去填 Key）"
    );
}

#[test]
fn a_blank_key_is_not_configured_rather_than_invalid() {
    let error = ApiKey::new("   ").expect_err("空白 Key 等于没配");
    assert_eq!(
        error.kind(),
        &CfErrorKind::NotConfigured,
        "空 Key 报 InvalidKey 会让用户去「换一把」，而他要做的是「填一把」"
    );
    assert!(
        ApiKey::new("  spaced-key  ").is_ok(),
        "首尾空白要被去掉，不是拒掉"
    );
}

#[test]
fn configured_key_is_visible_before_any_request() {
    let transport = FixtureTransport::new();
    let api_key = key();
    let client = CurseForge::with_key(&transport, &api_key);
    assert_eq!(client.key_state(), CfKeyState::Configured);
    assert_eq!(client.key_state().as_str(), "configured");
}

#[test]
fn unauthorized_and_forbidden_are_invalid_key_and_are_not_retried() {
    for status in [401u16, 403] {
        let transport = FixtureTransport::new().with_status(&search_prefix(), status, None);
        let api_key = key();
        let client = CurseForge::with_key(&transport, &api_key).with_policy(fast_policy(3));
        let error = block_on(client.search(&ModQuery::default())).expect_err("必须失败");
        assert_eq!(error.kind(), &CfErrorKind::InvalidKey, "status={status}");
        assert!(error.kind().is_unavailable());
        assert_eq!(
            transport.calls(),
            1,
            "401/403 不许重试（重试只会更糟，还白吃配额）status={status}"
        );
    }
}

#[test]
fn rate_limiting_carries_the_sites_retry_after_and_stops_at_the_bound() {
    let transport = FixtureTransport::new().with_status(&search_prefix(), 429, Some(7));
    let api_key = key();
    let client = CurseForge::with_key(&transport, &api_key).with_policy(fast_policy(3));
    let error = block_on(client.search(&ModQuery::default())).expect_err("必须失败");
    assert_eq!(
        error.kind(),
        &CfErrorKind::RateLimited {
            retry_after: Some(7)
        },
        "限流这一档必须带上站点给的 Retry-After"
    );
    assert_eq!(error.retry_after(), Some(7));
    assert!(
        error.message().contains("Retry-After=7s"),
        "人读说明里也要有等待秒数：{}",
        error.message()
    );
    assert_eq!(
        transport.calls(),
        3,
        "重试次数必须有上界（恰好 attempts 次）"
    );
}

#[test]
fn retrying_can_be_switched_off_entirely() {
    let transport = FixtureTransport::new().with_status(&search_prefix(), 429, Some(1));
    let api_key = key();
    let client = CurseForge::with_key(&transport, &api_key).with_policy(RetryPolicy::disabled());
    assert!(!client.policy().is_enabled());
    let error = block_on(client.search(&ModQuery::default())).expect_err("必须失败");
    assert_eq!(
        error.kind(),
        &CfErrorKind::RateLimited {
            retry_after: Some(1)
        }
    );
    assert_eq!(transport.calls(), 1, "关掉重试就是只发一次");
}

#[test]
fn a_limited_request_that_succeeds_on_the_next_try_is_not_an_error() {
    let transport = FixtureTransport::new().with_sequence(
        &search_prefix(),
        vec![
            CfResponse::new(429, Some(0), "{}"),
            CfResponse::new(200, None, fixtures::SEARCH),
        ],
    );
    let api_key = key();
    let client = CurseForge::with_key(&transport, &api_key).with_policy(fast_policy(3));
    let page = block_on(client.search(&ModQuery::default())).expect("第二次应当成功");
    assert_eq!(page.hits.len(), 2);
    assert_eq!(transport.calls(), 2, "成功之后不许再发第三次");
}

#[test]
fn server_errors_are_retried_and_then_reported_as_server() {
    let transport = FixtureTransport::new().with_status(&search_prefix(), 503, None);
    let api_key = key();
    let client = CurseForge::with_key(&transport, &api_key).with_policy(fast_policy(2));
    let error = block_on(client.search(&ModQuery::default())).expect_err("必须失败");
    assert_eq!(error.kind(), &CfErrorKind::Server);
    assert_eq!(transport.calls(), 2);
}

#[test]
fn not_found_is_not_retried() {
    let transport = FixtureTransport::new().with_status(&search_prefix(), 404, None);
    let api_key = key();
    let client = CurseForge::with_key(&transport, &api_key).with_policy(fast_policy(3));
    let error = block_on(client.search(&ModQuery::default())).expect_err("必须失败");
    assert_eq!(error.kind(), &CfErrorKind::NotFound);
    assert_eq!(transport.calls(), 1);
}

#[test]
fn a_bad_request_is_unsupported_because_our_url_is_wrong_not_the_site() {
    let transport = FixtureTransport::new().with_status(&search_prefix(), 400, None);
    let api_key = key();
    let client = CurseForge::with_key(&transport, &api_key).with_policy(fast_policy(3));
    let error = block_on(client.search(&ModQuery::default())).expect_err("必须失败");
    assert_eq!(
        error.kind(),
        &CfErrorKind::Unsupported,
        "400 是我们拼错了请求：报成 Server 会让用户一直等站点恢复"
    );
    assert_eq!(transport.calls(), 1, "拼错重试也一样错");
}

#[test]
fn transport_failures_are_network_errors_and_are_retried() {
    let transport = FixtureTransport::new().with_error(
        &search_prefix(),
        CfError::network("连不上 api.curseforge.com"),
    );
    let api_key = key();
    let client = CurseForge::with_key(&transport, &api_key).with_policy(fast_policy(3));
    let error = block_on(client.search(&ModQuery::default())).expect_err("必须失败");
    assert_eq!(error.kind(), &CfErrorKind::Network);
    assert_eq!(transport.calls(), 3);
}

#[test]
fn broken_json_is_malformed_and_is_not_dressed_up_as_an_empty_page() {
    let transport = FixtureTransport::new().with(&search_prefix(), "not json at all");
    let api_key = key();
    let client = CurseForge::with_key(&transport, &api_key);
    let error = block_on(client.search(&ModQuery::default())).expect_err("必须失败");
    assert_eq!(error.kind(), &CfErrorKind::Malformed);
    assert_eq!(transport.calls(), 1, "解析失败重试也没用");
}

#[test]
fn a_response_without_the_data_envelope_is_malformed_not_an_empty_page() {
    // 站点方言变了时最危险的下场是「静默 0 条」：页面画成「没有匹配的内容」，
    // 用户会去换关键词，而真相是我们读错了信封。
    let transport =
        FixtureTransport::new().with(&search_prefix(), r#"{"pagination":{"totalCount":3}}"#);
    let api_key = key();
    let client = CurseForge::with_key(&transport, &api_key);
    let error = block_on(client.search(&ModQuery::default())).expect_err("必须失败");
    assert_eq!(error.kind(), &CfErrorKind::Malformed);
}

#[test]
fn the_request_carries_the_key_and_json_accept_headers() {
    let transport = FixtureTransport::new().with(&search_prefix(), fixtures::SEARCH);
    let api_key = key();
    let client = CurseForge::with_key(&transport, &api_key);
    block_on(client.search(&ModQuery::default())).expect("应当成功");
    let seen = transport.seen();
    let headers = &seen[0].headers;
    assert!(
        headers
            .iter()
            .any(|(name, value)| name == "x-api-key" && value == TEST_KEY),
        "Key 必须真的发给站点（否则「有 Key」与「没 Key」在用例里等价）：{headers:?}"
    );
    assert!(
        headers
            .iter()
            .any(|(name, value)| name == "accept" && value == "application/json"),
        "headers={headers:?}"
    );
}

#[test]
fn the_key_never_reaches_an_error_message_or_a_debug_render() {
    let transport = FixtureTransport::new().with_status(&search_prefix(), 403, None);
    let api_key = key();
    let client = CurseForge::with_key(&transport, &api_key).with_policy(fast_policy(1));
    let error = block_on(client.search(&ModQuery::default())).expect_err("必须失败");
    assert!(
        !error.to_string().contains(TEST_KEY),
        "错误串里不许出现 Key：{error}"
    );
    assert!(
        !format!("{client:?}").contains(TEST_KEY),
        "客户端的 Debug 不许印 Key：{client:?}"
    );
    assert!(
        format!("{:?}", key()).contains("redacted"),
        "ApiKey 的 Debug 必须是脱敏的"
    );
}
