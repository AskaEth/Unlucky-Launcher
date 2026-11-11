//! [`super`]（rig 适配层）的用例（M174 r1 复审 P2-5 拆出来：结构化映射让 `rig.rs` 撞了 400 行）。
//!
//! 纯搬移。分类的端到端证据不在本文件：那要真 HTTP，用例在
//! `crates/cli/src/commands/agent/probe/tests.rs`（四类各一台假服务器）。

use super::*;

fn tool(id: &str, name: &str) -> super::super::chat::ToolCall {
    super::super::chat::ToolCall {
        id: id.to_string(),
        name: name.to_string(),
        arguments: serde_json::json!({ "path": "appearance.theme_mode" }),
    }
}

#[test]
fn disabled_config_is_rejected_before_touching_the_network() {
    let Err(err) = RigProvider::new(ProviderConfig::disabled()) else {
        panic!("禁用态应在构造阶段被拒");
    };
    assert!(err.to_string().contains("禁用"));
}

#[test]
fn anthropic_without_key_is_rejected() {
    assert!(RigProvider::new(ProviderConfig::anthropic("claude-sonnet-4")).is_err());
    assert!(
        RigProvider::new(ProviderConfig::anthropic("claude-sonnet-4").with_api_key("sk-ant"))
            .is_ok()
    );
}

#[test]
fn local_states_build_offline() {
    assert!(RigProvider::new(ProviderConfig::llamacpp_local("qwen2.5-7b")).is_ok());
    assert!(RigProvider::new(ProviderConfig::ollama("llama3.1")).is_ok());
}

#[test]
fn request_maps_messages_and_tools() {
    let request = ChatRequest {
        model: "mock".to_string(),
        messages: vec![
            ChatMessage::system("你是助手"),
            ChatMessage::user("把主题换成浅色"),
            ChatMessage::Assistant {
                text: String::new(),
                tool_calls: vec![tool("call_1", "set_config")],
            },
            ChatMessage::tool("call_1", "set_config", "{\"status\":\"written\"}"),
        ],
        tools: vec![crate::registry::ToolDefinition {
            name: "set_config".to_string(),
            description: "改配置".to_string(),
            input_schema: serde_json::json!({ "type": "object" }),
        }],
        temperature: Some(0.5),
        max_tokens: Some(512),
    };
    let built = build_request(&request).expect("应能构造");
    assert_eq!(built.chat_history.len(), 4);
    assert_eq!(built.tools.len(), 1);
    assert_eq!(built.tools[0].name.as_str(), "set_config");
    assert_eq!(built.temperature, Some(0.5));
    assert_eq!(built.max_tokens, Some(512));
    // 工具结果在 rig 0.45 里是 user 角色的 ToolResult 内容块
    let json = serde_json::to_string(&built.chat_history[3]).expect("可序列化");
    assert!(json.contains("toolresult"), "{json}");
}

#[test]
fn empty_history_is_a_readable_error() {
    let request = ChatRequest {
        model: "m".to_string(),
        messages: Vec::new(),
        tools: Vec::new(),
        temperature: None,
        max_tokens: None,
    };
    let err = build_request(&request).unwrap_err();
    assert!(err.to_string().contains("对话历史为空"));
}
