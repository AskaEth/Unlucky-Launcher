//! Provider 四态端点配置（02 文档 §2）。
//!
//! | 形态 | 端点 | 备注 |
//! |------|------|------|
//! | OpenAI 兼容（默认） | `base_url` + `api_key` + `model` | 覆盖 OpenAI/DeepSeek/Qwen/GLM/**本地 llama.cpp server**（`http://127.0.0.1:8080/v1`）/LM Studio |
//! | Anthropic 原生 | `api_key` + `model` | Claude 系列（没有可用的 Rust 单厂商 SDK，见 research/02 §3.6） |
//! | Ollama 原生 | `http://127.0.0.1:11434` | 本地另一种主流 |
//! | 禁用 | — | 完全关闭 agent，规则引擎（`diagnose_crash`）仍可用 |
//!
//! 本文件只放**配置与校验**；对话协议在 [`super::chat`]。这样 rig 的 pre-1.0 破坏面
//! （research/02 §7 R1）被关在 [`super::rig`] 一个文件里，这里连 rig 的名字都不出现。

use std::fmt;

use serde::{Deserialize, Serialize};

/// 本地 llama.cpp server 的默认 OpenAI 兼容端点（research/02 §5.1）。
pub const LLAMACPP_DEFAULT_BASE_URL: &str = "http://127.0.0.1:8080/v1";
/// Ollama 默认端点（research/02 §5.2）。
pub const OLLAMA_DEFAULT_BASE_URL: &str = "http://127.0.0.1:11434";
/// Anthropic 官方 API 端点。
pub const ANTHROPIC_DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

/// Provider 四态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// OpenAI 兼容（含本地 llama.cpp / LM Studio / DeepSeek / Qwen…）。
    OpenAiCompatible,
    /// Anthropic 原生。
    Anthropic,
    /// Ollama 原生。
    Ollama,
    /// 完全禁用 agent。
    Disabled,
}

impl ProviderKind {
    /// 该形态是否必须提供 API key（本地推理通常不需要）。
    #[must_use]
    pub fn requires_api_key(self) -> bool {
        matches!(self, Self::Anthropic)
    }

    /// 该形态的默认端点（OpenAI 兼容没有统一默认值，必须显式填；
    /// 本地 llama.cpp 走 [`ProviderConfig::llamacpp_local`] 带的显式默认）。
    #[must_use]
    pub fn default_base_url(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "",
            Self::Anthropic => ANTHROPIC_DEFAULT_BASE_URL,
            Self::Ollama => OLLAMA_DEFAULT_BASE_URL,
            Self::Disabled => "",
        }
    }

    /// 设置页展示名。
    #[must_use]
    pub fn display_name(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "OpenAI 兼容",
            Self::Anthropic => "Anthropic 原生",
            Self::Ollama => "Ollama 原生",
            Self::Disabled => "已禁用",
        }
    }
}

/// Provider 配置（设置 → AI 的一份快照）。
///
/// `api_key` 只在进程内存在，**任何日志/审计/事件都不得带它**——
/// `Debug` 是手写的（掩码 key），要连形态一起看用 [`ProviderConfig::redacted_debug`]。
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// 四态。
    pub kind: ProviderKind,
    /// 端点；空串表示用 [`ProviderKind::default_base_url`]。
    pub base_url: String,
    /// 模型名；`Disabled` 时可为空。
    pub model: String,
    /// API key（本地推理可空）。
    pub api_key: Option<String>,
    /// 采样温度。
    pub temperature: Option<f32>,
    /// 单次回复 token 上限。
    pub max_tokens: Option<u32>,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self::disabled()
    }
}

impl ProviderConfig {
    fn base(kind: ProviderKind, base_url: &str, model: &str) -> Self {
        Self {
            kind,
            base_url: base_url.to_string(),
            model: model.to_string(),
            api_key: None,
            temperature: None,
            max_tokens: None,
        }
    }

    /// OpenAI 兼容端点（云端或自建）。
    #[must_use]
    pub fn openai_compatible(base_url: &str, model: &str) -> Self {
        Self::base(ProviderKind::OpenAiCompatible, base_url, model)
    }

    /// 本机 llama.cpp server（用户就在跑 llama-server 的默认场景）。
    #[must_use]
    pub fn llamacpp_local(model: &str) -> Self {
        Self::base(
            ProviderKind::OpenAiCompatible,
            LLAMACPP_DEFAULT_BASE_URL,
            model,
        )
    }

    /// Anthropic 原生。
    #[must_use]
    pub fn anthropic(model: &str) -> Self {
        Self::base(ProviderKind::Anthropic, ANTHROPIC_DEFAULT_BASE_URL, model)
    }

    /// 本机 Ollama。
    #[must_use]
    pub fn ollama(model: &str) -> Self {
        Self::base(ProviderKind::Ollama, OLLAMA_DEFAULT_BASE_URL, model)
    }

    /// 禁用 agent。
    #[must_use]
    pub fn disabled() -> Self {
        Self::base(ProviderKind::Disabled, "", "")
    }

    /// 带上 API key。
    #[must_use]
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// 带上温度。
    #[must_use]
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// agent 是否启用。
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.kind != ProviderKind::Disabled
    }

    /// 实际使用的端点（空则回落默认值）。
    #[must_use]
    pub fn resolved_base_url(&self) -> &str {
        if self.base_url.trim().is_empty() {
            self.kind.default_base_url()
        } else {
            self.base_url.trim()
        }
    }

    /// 配置是否可用（装配期调用一次，错就进设置页提示而不是等到对话时炸）。
    ///
    /// # Errors
    /// 禁用、缺模型名、缺端点、缺 API key 时返回可读错误。
    pub fn validate(&self) -> Result<(), ProviderError> {
        if !self.is_enabled() {
            return Err(ProviderError::new(
                "AI 已禁用：请在设置 → AI 中选择端点（规则引擎诊断不受影响）",
            ));
        }
        if self.model.trim().is_empty() {
            return Err(ProviderError::new("没有配置模型名（model）"));
        }
        if self.resolved_base_url().trim().is_empty() {
            return Err(ProviderError::new(format!(
                "{} 需要填写端点（base_url）",
                self.kind.display_name()
            )));
        }
        if self.kind.requires_api_key()
            && self
                .api_key
                .as_deref()
                .is_none_or(|key| key.trim().is_empty())
        {
            return Err(ProviderError::new(format!(
                "{} 需要 API key",
                self.kind.display_name()
            )));
        }
        Ok(())
    }

    /// 可打印的形态描述（**key 已掩码**，给日志/审计/设置页用）。
    #[must_use]
    pub fn redacted_debug(&self) -> String {
        let key = match self.api_key.as_deref() {
            None => "无".to_string(),
            Some(k) if k.trim().is_empty() => "无".to_string(),
            Some(_) => crate::redact::MASK.to_string(),
        };
        format!(
            "{} base_url={} model={} api_key={key}",
            self.kind.display_name(),
            self.resolved_base_url(),
            if self.model.is_empty() {
                "（未设置）"
            } else {
                &self.model
            },
        )
    }
}

impl fmt::Debug for ProviderConfig {
    /// 手写 Debug：**key 掩码**（`api_key` 是唯一不能出现在日志里的字段）。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderConfig")
            .field("kind", &self.kind)
            .field("base_url", &self.resolved_base_url())
            .field("model", &self.model)
            .field(
                "api_key",
                &self.api_key.as_deref().map(|_| crate::redact::MASK),
            )
            .field("temperature", &self.temperature)
            .field("max_tokens", &self.max_tokens)
            .finish()
    }
}

/// Provider 层错误（消息面向用户：端点连不上、key 无效、模型不存在）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderError {
    message: String,
}

impl ProviderError {
    /// 从可读文本构造。
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ProviderError {}

#[cfg(test)]
mod tests {
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
        let config =
            ProviderConfig::openai_compatible("https://api.deepseek.com/v1", "deepseek-chat")
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
}
