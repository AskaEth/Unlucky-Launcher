//! [`super`] 的用例（M174 r1 复审 P2-5 拆出来：`provider.rs` 撞了 400 行硬线）。
//!
//! 拆的是**纯搬移**：一条断言都没改。分类那半边的新用例在 `failure` 的模块文档里指路，
//! 结构化分类本身由 `brain/rig.rs` 的 `map_error` 产生，用例在 `probe` 的真服务器那几条。

use super::*;

#[test]
fn local_defaults_match_research() {
    assert_eq!(
        ProviderConfig::llamacpp_local("qwen2.5").resolved_base_url(),
        LLAMACPP_DEFAULT_BASE_URL
    );
    assert_eq!(
        ProviderConfig::ollama("llama3").resolved_base_url(),
        OLLAMA_DEFAULT_BASE_URL
    );
    assert_eq!(
        ProviderConfig::anthropic("claude-sonnet-4").resolved_base_url(),
        ANTHROPIC_DEFAULT_BASE_URL
    );
}

#[test]
fn setting_vocabulary_round_trips() {
    // 设置里存的是 `openai_compatible`（schema 的 `AI_PROVIDERS`），不是 serde 的
    // `open_ai_compatible`：两个词表必须是这里这一对映射，否则「用户在设置页选的值」
    // agent 侧读出来是 None（M174 的根因）。
    for kind in [
        ProviderKind::OpenAiCompatible,
        ProviderKind::Anthropic,
        ProviderKind::Ollama,
        ProviderKind::Disabled,
    ] {
        assert_eq!(ProviderKind::from_setting(kind.setting_value()), Some(kind));
    }
    assert_eq!(
        ProviderKind::from_setting("  ollama "),
        Some(ProviderKind::Ollama)
    );
    assert_eq!(ProviderKind::from_setting("open_ai_compatible"), None);
    assert_eq!(ProviderKind::from_setting(""), None);
    // schema 的允许值表与本枚举的取值表是**同一个集合**（一方加了另一方没加就红）。
    assert_eq!(
        unlucky_core::setting::keys::AI_PROVIDERS,
        [
            ProviderKind::OpenAiCompatible.setting_value(),
            ProviderKind::Anthropic.setting_value(),
            ProviderKind::Ollama.setting_value(),
            ProviderKind::Disabled.setting_value(),
        ]
    );
}

#[test]
fn validate_requires_model_base_url_and_key() {
    assert!(!ProviderConfig::disabled().is_enabled());
    assert!(ProviderConfig::disabled().validate().is_err());
    assert!(ProviderConfig::llamacpp_local("").validate().is_err());
    assert!(ProviderConfig::llamacpp_local("qwen").validate().is_ok());
    // OpenAI 兼容必须显式给端点
    assert!(ProviderConfig::openai_compatible("", "gpt-4o-mini")
        .validate()
        .is_err());
    assert!(
        ProviderConfig::openai_compatible("https://api.deepseek.com/v1", "deepseek-chat")
            .validate()
            .is_ok()
    );
    assert!(ProviderConfig::anthropic("claude").validate().is_err());
    assert!(ProviderConfig::anthropic("claude")
        .with_api_key("sk-ant")
        .validate()
        .is_ok());
}

#[test]
fn redacted_debug_never_leaks_key() {
    let config = ProviderConfig::openai_compatible("https://api.deepseek.com/v1", "deepseek-chat")
        .with_api_key("sk-super-secret");
    let text = config.redacted_debug();
    assert!(!text.contains("sk-super-secret"), "{text}");
    assert!(text.contains(crate::redact::MASK));
    // `{:?}` 也必须掩码：Debug 是 panic 消息/日志的默认出口。
    let debug = format!("{config:?}");
    assert!(!debug.contains("sk-super-secret"), "{debug}");
    assert!(debug.contains(crate::redact::MASK), "{debug}");
}

#[test]
fn empty_base_url_falls_back_to_default() {
    let mut config = ProviderConfig::ollama("llama3");
    config.base_url = String::new();
    assert_eq!(config.resolved_base_url(), OLLAMA_DEFAULT_BASE_URL);
}

#[test]
fn four_states_serialize_as_snake_case() {
    assert_eq!(
        serde_json::to_string(&ProviderKind::OpenAiCompatible).unwrap(),
        "\"open_ai_compatible\""
    );
    assert_eq!(
        serde_json::to_string(&ProviderConfig::disabled()).unwrap(),
        "{\"kind\":\"disabled\",\"base_url\":\"\",\"model\":\"\",\"api_key\":null,\"temperature\":null,\"max_tokens\":null}"
    );
}
