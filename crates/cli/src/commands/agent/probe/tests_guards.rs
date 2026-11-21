//! 「测试连接」的**不发请求**那一半守卫（M174）：未配置不建 provider / 改完设置打新地址 /
//! 密钥不外流 / 文件读取。与 `probe/tests.rs` 的分工：那边起真 TCP 假服务器验四类失败，
//! 这边只验「什么时候不发」「发到哪」「发出去的东西里有什么」。

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use unlucky_agent::brain::provider::ProviderConfig;
use unlucky_agent::brain::{
    ChatProvider, ChatRequest, DeltaObserver, ProviderError, ProviderFuture,
};
use unlucky_agent::redact::MASK;

use super::{settings, Behaviour, Server, AUTH_BODY};
use crate::commands::agent::probe::{probe_file, probe_with, AiSettings, Missing, ProbeOutcome};

#[test]
fn incomplete_config_never_builds_a_provider() {
    struct Counter;
    impl ChatProvider for Counter {
        fn config(&self) -> &ProviderConfig {
            unreachable!("不该被造出来")
        }
        fn complete<'a>(
            &'a self,
            _request: &'a ChatRequest,
            _observer: &'a dyn DeltaObserver,
        ) -> ProviderFuture<'a> {
            unreachable!("不该被造出来")
        }
    }

    let builds = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&builds);
    let factory: unlucky_agent::brain::ProviderFactory =
        Arc::new(move |_config: &ProviderConfig| {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(Counter) as Arc<dyn ChatProvider>)
        });

    // 四种「不完整」：禁用 / 缺端点 / 缺模型 / 缺密钥（Anthropic）
    let mut cases = vec![
        (
            AiSettings {
                enabled: false,
                ..settings("http://127.0.0.1:1/v1", "m")
            },
            Missing::Disabled,
        ),
        (
            AiSettings {
                base_url: String::new(),
                ..settings("", "m")
            },
            Missing::BaseUrl,
        ),
        (
            AiSettings {
                model: String::new(),
                ..settings("http://127.0.0.1:1/v1", "")
            },
            Missing::Model,
        ),
        (
            AiSettings {
                provider: "anthropic".to_string(),
                base_url: String::new(),
                api_key: None,
                ..settings("", "claude-sonnet-4")
            },
            Missing::ApiKey,
        ),
        (
            AiSettings {
                provider: "openai-compatible".to_string(),
                ..settings("http://127.0.0.1:1/v1", "m")
            },
            Missing::Provider,
        ),
    ];
    // `..::settings(..)` 里那些字段会被逐个覆盖，这里只要求「不全的配置都能被拦下」。
    for (case, expected) in cases.drain(..) {
        let outcome = probe_with(&factory, &case, Duration::from_millis(50));
        assert_eq!(
            outcome,
            ProbeOutcome::NotConfigured { missing: expected },
            "配置不全时必须返回 NotConfigured（{expected:?}），实际 {outcome:?}"
        );
    }
    assert_eq!(
        builds.load(Ordering::SeqCst),
        0,
        "未配置时一个 provider 都不许造（更不许发请求）"
    );
}

/// 守卫：设置页保存完再测，请求打到的是**文件里现在那个** `ai.base_url`。
#[test]
fn saved_base_url_is_the_address_actually_dialed() {
    let old = Server::start(Behaviour::Reply(
        "HTTP/1.1 401 Unauthorized",
        "application/json",
        AUTH_BODY,
    ));
    let new = Server::start(Behaviour::Reply(
        "HTTP/1.1 401 Unauthorized",
        "application/json",
        AUTH_BODY,
    ));
    let path = temp_config("base-url");
    write_config(
        &path,
        &format!(
            r#"{{"ai":{{"enabled":true,"provider":"openai_compatible","base_url":"{}","model":"gpt-test","api_key":"sk-saved"}}}}"#,
            old.base_url()
        ),
    );

    let first = probe_file(&path, Duration::from_secs(5));
    assert!(!first.is_pass(), "假服务器回 401，不该算通过");
    assert_eq!(old.hits().len(), 1, "第一次必须打到旧地址");

    // 用户在设置页改了 base_url（同一份 schema、同一个文件）之后再测一次。
    write_config(
        &path,
        &format!(
            r#"{{"ai":{{"enabled":true,"provider":"openai_compatible","base_url":"{}","model":"gpt-test","api_key":"sk-saved"}}}}"#,
            new.base_url()
        ),
    );
    let second = probe_file(&path, Duration::from_secs(5));
    assert!(!second.is_pass());
    assert_eq!(
        new.hits().len(),
        1,
        "改完 base_url 之后请求必须打到新地址（读的是文件里现在那一份）"
    );
    assert_eq!(old.hits().len(), 1, "旧地址不该再被打一次");
    let hit = &new.hits()[0];
    assert!(
        hit.head.starts_with("POST /v1/chat/completions"),
        "OpenAI 兼容形态请求的是 /v1/chat/completions，实际请求行：{}",
        hit.head.lines().next().unwrap_or("")
    );
    assert!(
        hit.body.contains("\"model\":\"gpt-test\""),
        "请求正文里的 model 不是配置里的那个：{}",
        hit.body
    );
    // P2-1：改完 base_url 之后，失败结论里说的也必须是**新**地址。
    assert_eq!(
        second.target().map(|target| target.base_url.as_str()),
        Some(new.base_url().as_str())
    );
}

/// 守卫①：密钥不得出现在 `ProbeOutcome` 的任何字段里，也不得出现在 `AiSettings` 的 `Debug` 里。
///
/// 这里让假服务器把**密钥原文回显**在 401 错误体里（最坏情况）：`detail` 必须是掩码。
#[test]
fn key_never_leaks_into_outcome_or_debug() {
    const SECRET: &str = "sk-live-LEAK-MARKER-9f3a";
    // 服务端把 key 原样回显（有些网关会这么干）。
    let body = r#"{"error":{"message":"Incorrect API key provided: sk-live-LEAK-MARKER-9f3a","code":"invalid_api_key"}}"#;
    let server = Server::start(Behaviour::Reply(
        "HTTP/1.1 401 Unauthorized",
        "application/json",
        body,
    ));
    let settings = AiSettings {
        api_key: Some(SECRET.to_string()),
        ..settings(&server.base_url(), "gpt-test")
    };

    let outcome = probe_with(
        &unlucky_agent::brain::rig::RigProvider::factory(),
        &settings,
        Duration::from_secs(5),
    );
    let printed = format!("{outcome:?}");
    assert!(
        !printed.contains(SECRET),
        "ProbeOutcome 里出现了密钥明文：{printed}"
    );
    assert!(
        printed.contains(MASK),
        "被掩码后应当看得见掩码标记：{printed}"
    );

    let debug = format!("{settings:?}");
    assert!(
        !debug.contains(SECRET),
        "AiSettings 的 Debug 漏了密钥：{debug}"
    );
    assert!(
        debug.contains(MASK),
        "AiSettings 的 Debug 应当显示掩码：{debug}"
    );
}

#[test]
fn settings_load_reads_the_five_keys() {
    let path = temp_config("load");
    write_config(
        &path,
        r#"{"ai":{"enabled":false,"provider":"ollama","base_url":"http://127.0.0.1:11434","model":"llama3","api_key":"sk-x"}}"#,
    );
    let settings = AiSettings::load(&path);
    assert!(!settings.enabled);
    assert_eq!(settings.provider, "ollama");
    assert_eq!(settings.base_url, "http://127.0.0.1:11434");
    assert_eq!(settings.model, "llama3");
    assert_eq!(settings.api_key.as_deref(), Some("sk-x"));

    // 空的 api_key（null / 空串）读成 `None`，不是 `Some("")`。
    let empty = temp_config("empty-key");
    write_config(
        &empty,
        r#"{"ai":{"provider":"ollama","model":"llama3","api_key":""}}"#,
    );
    assert_eq!(AiSettings::load(&empty).api_key, None);
    // 文件不存在也不 panic（`effective_*` 给 schema 默认值 = 未配置，不是坏值）。
    let missing = temp_config("missing");
    let settings = AiSettings::load(&missing);
    assert_eq!(
        settings.provider, "openai_compatible",
        "缺文件时是 schema 默认值"
    );
    assert!(settings.model.is_empty());
    assert_eq!(settings.api_key, None);
    assert_eq!(settings.to_config(), Err(Missing::BaseUrl));
}

// ------------------------------------------------------------------ 夹具

fn temp_config(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ull-probe-{label}-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(dir.join("config")).expect("临时目录");
    dir.join("config/launcher.json")
}

fn write_config(path: &PathBuf, body: &str) {
    std::fs::write(path, body).expect("写配置文件");
}

/// 工厂阶段的错误也按**结构化分类**走（r1 复审 P2-5 之后：这里不再看文本）。
///
/// 两条一起钉住：
/// 1. 工厂给的是 `classified(Auth, …)` ⇒ 结论就是 `auth`（分类信息不丢）；
/// 2. 工厂给的是 `ProviderError::new("401 bad key")`（**纯文本、没有分类**）⇒ 结论是 `failed`，
///    **不是** `auth`。第二条是 P2-5 的绊线：谁把「按 `"401"` 子串猜类别」加回来，这条立刻红。
#[test]
fn factory_error_carries_the_structured_classification() {
    let classified: unlucky_agent::brain::ProviderFactory = Arc::new(|_config: &ProviderConfig| {
        Err(ProviderError::classified(
            unlucky_agent::brain::ProviderFailure::Auth,
            "401 Unauthorized",
        ))
    });
    let outcome = probe_with(
        &classified,
        &settings("http://127.0.0.1:1/v1", "gpt-test"),
        Duration::from_millis(50),
    );
    assert_eq!(
        outcome.kind(),
        "auth",
        "工厂阶段的结构化分类不许丢：{outcome:?}"
    );

    // 纯文本错误（无分类）**不许**被猜成鉴权失败。
    let text_only: unlucky_agent::brain::ProviderFactory =
        Arc::new(|_config: &ProviderConfig| Err(ProviderError::new("401 bad key")));
    let outcome = probe_with(
        &text_only,
        &settings("http://127.0.0.1:1/v1", "gpt-test"),
        Duration::from_millis(50),
    );
    assert_eq!(
        outcome.kind(),
        "failed",
        "没有结构化分类的错误必须落在「其它」，不许按文本猜成 auth（P2-5）：{outcome:?}"
    );
}
