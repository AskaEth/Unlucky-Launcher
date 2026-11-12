//! 「测试连接」（M174）：拿 `config/launcher.json` 里的 AI 四项，走**现有 provider 实现**
//! （[`RigProvider`]）发一次最小请求，并把失败分成四类。
//!
//! ## 分类从哪来（r1 复审 P2-5 之后）
//!
//! **不在本模块猜文本**：分类由 `unlucky_agent::brain::rig::map_error` 从 rig 0.45 的结构化
//! 报告（`ErrorReport` 的 `kind` / `http_status` / `code`）翻译成
//! [`ProviderFailure`]，本模块只做「枚举 → 界面结论」的映射：
//!
//! | [`ProviderFailure`] | 本模块产出 |
//! |---|---|
//! | `Auth`（401/403） | [`ProbeOutcome::Auth`] |
//! | `ModelNotFound`（404 / `model_not_found`） | [`ProbeOutcome::ModelNotFound`] |
//! | `Unreachable`（传输层：连接被拒 / DNS / TLS） | [`ProbeOutcome::Unreachable`] |
//! | `Timeout`（8 秒时限 / 传输层超时） | [`ProbeOutcome::Timeout`] |
//! | `NotConfigured`（没发请求） | [`ProbeOutcome::NotConfigured`] |
//! | `Other`（解析 / 5xx / 未知） | [`ProbeOutcome::Failed`] |
//!
//! ## 三条硬纪律
//!
//! 1. **配置不完整时一个请求都不发**：[`ProbeOutcome::NotConfigured`] 在造 provider **之前**返回，
//!    连工厂都不会被调用（守卫②：假 provider 的构造计数必须是 0）。
//! 2. **密钥永不外流**：`detail` 出本模块前先过 [`unlucky_agent::redact::redact_text`]，再把
//!    **本次的密钥原文**替换成 [`MASK`]——即使服务端把 key 回显在错误体里，界面上也看不到它。
//!    [`AiSettings`] 的 `Debug` 是手写的（掩码），[`ProbeOutcome`] 里没有密钥字段。
//! 3. **每次现读文件**：[`probe_file`] 读的是「现在这一份」，所以设置页保存完再测，测的就是刚
//!    存下的 `ai.base_url`（不是启动时缓存的旧值）。
//!
//! 失败结论**一律带上被请求的 target**（r1 复审 P2-1）：只说「鉴权失败」而不说「对哪个端点、
//! 哪个模型」，用户改完 key 也没法确认自己改的是不是那一份配置。

use std::path::Path;
use std::time::Duration;

use unlucky_agent::brain::provider::{
    ProviderConfig, ProviderError, ProviderFailure, ProviderKind,
};
use unlucky_agent::brain::rig::RigProvider;
use unlucky_agent::brain::{ChatMessage, ChatRequest, NoopObserver, ProviderFactory};
use unlucky_agent::redact::{redact_text, MASK};
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;

/// 一次「测试连接」的默认时限。
///
/// 8 秒：够一个跨洋 HTTPS 往返 + 服务端首 token；再长用户会以为按钮没反应。
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(8);

/// 探测请求只发这一句：够触发一次真实补全，又不烧 token。
const PING: &str = "ping";

/// 从设置文件读出来的 AI 四项（快照）。
///
/// `api_key` 在这里是**明文**（要用它发请求），所以 `Debug` 手写成掩码。
#[derive(Clone, PartialEq, Eq)]
pub struct AiSettings {
    /// `ai.enabled`
    pub enabled: bool,
    /// `ai.provider`（设置词表：`openai_compatible` / `anthropic` / `ollama` / `disabled`）
    pub provider: String,
    /// `ai.base_url`（空 = 用该形态的默认端点）
    pub base_url: String,
    /// `ai.model`
    pub model: String,
    /// `ai.api_key`（空/缺 = 没配置）
    pub api_key: Option<String>,
}

impl std::fmt::Debug for AiSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiSettings")
            .field("enabled", &self.enabled)
            .field("provider", &self.provider)
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_deref().map(|_| MASK))
            .finish()
    }
}

/// 缺哪一项配置（`NotConfigured` 的载荷：界面上要说清楚「差什么」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Missing {
    /// `ai.enabled=false` 或 `ai.provider=disabled`
    Disabled,
    /// `ai.provider` 是空/未知值
    Provider,
    /// 该形态需要端点，但 `ai.base_url` 是空的
    BaseUrl,
    /// `ai.model` 是空的
    Model,
    /// 该形态（Anthropic）必须有密钥，但 `ai.api_key` 是空的
    ApiKey,
}

impl Missing {
    /// 稳定名（CLI 输出 / 日志 / 测试断言用；界面走 i18n）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Provider => "provider",
            Self::BaseUrl => "base_url",
            Self::Model => "model",
            Self::ApiKey => "api_key",
        }
    }
}

/// 被请求的那一份配置（端点 + 模型）：失败结论也要带它，用户才知道「改的是不是这一份」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// 实际请求的端点（`resolved_base_url`）。
    pub base_url: String,
    /// 实际请求的模型名。
    pub model: String,
}

/// 一次探测的结论（**不含密钥**；`detail` 已脱敏，可直接上屏/进日志）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// 配置不完整：**没有发出任何请求**（因此也没有 target）。
    NotConfigured {
        /// 缺的是哪一项。
        missing: Missing,
    },
    /// 通了（拿到一次真实补全的回复）。
    Passed {
        /// 实际请求的目标。
        target: Target,
    },
    /// 鉴权失败（401 / 403）。
    Auth {
        /// 实际请求的目标。
        target: Target,
        /// 脱敏后的原因。
        detail: String,
    },
    /// 模型不存在（404 / `model_not_found`）。
    ModelNotFound {
        /// 实际请求的目标。
        target: Target,
        /// 脱敏后的原因。
        detail: String,
    },
    /// 端点不可达（连接被拒 / DNS / TLS）。
    Unreachable {
        /// 实际请求的目标。
        target: Target,
        /// 脱敏后的原因。
        detail: String,
    },
    /// 超时。
    Timeout {
        /// 实际请求的目标。
        target: Target,
        /// 脱敏后的原因（含时限）。
        detail: String,
    },
    /// 其它（协议 / 解析 / 5xx）：不硬塞进上面四类。
    Failed {
        /// 实际请求的目标。
        target: Target,
        /// 脱敏后的原因。
        detail: String,
    },
}

impl ProbeOutcome {
    /// 是否「通」。
    #[must_use]
    pub fn is_pass(&self) -> bool {
        matches!(self, Self::Passed { .. })
    }

    /// 稳定名（`passed` / `auth` / `model_not_found` / `unreachable` / `timeout` /
    /// `not_configured` / `failed`）。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::NotConfigured { .. } => "not_configured",
            Self::Passed { .. } => "passed",
            Self::Auth { .. } => "auth",
            Self::ModelNotFound { .. } => "model_not_found",
            Self::Unreachable { .. } => "unreachable",
            Self::Timeout { .. } => "timeout",
            Self::Failed { .. } => "failed",
        }
    }

    /// 这条结论是关于哪个端点/模型的（`NotConfigured` 没有）。
    #[must_use]
    pub fn target(&self) -> Option<&Target> {
        match self {
            Self::NotConfigured { .. } => None,
            Self::Passed { target }
            | Self::Auth { target, .. }
            | Self::ModelNotFound { target, .. }
            | Self::Unreachable { target, .. }
            | Self::Timeout { target, .. }
            | Self::Failed { target, .. } => Some(target),
        }
    }
}

impl AiSettings {
    /// 从设置文件读四项（文件缺失/坏掉时给「全默认」= 未配置，不报错）。
    #[must_use]
    pub fn load(path: &Path) -> Self {
        let store = SettingStore::load(path, launcher_schema());
        let doc = store.doc();
        let text = |key: &str| doc.effective_str(key).unwrap_or_default().to_string();
        let api_key = doc
            .effective_str("ai.api_key")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        Self {
            enabled: doc.effective_bool("ai.enabled").unwrap_or(true),
            provider: text("ai.provider"),
            base_url: text("ai.base_url"),
            model: text("ai.model"),
            api_key,
        }
    }

    /// 四项 → [`ProviderConfig`]；缺项时给出**具体缺哪一项**（不发请求）。
    ///
    /// # Errors
    /// 禁用、端点/模型/密钥缺失或 `ai.provider` 不是已知取值时返回 [`Missing`]。
    pub fn to_config(&self) -> Result<ProviderConfig, Missing> {
        if !self.enabled {
            return Err(Missing::Disabled);
        }
        let Some(kind) = ProviderKind::from_setting(&self.provider) else {
            return Err(Missing::Provider);
        };
        if kind == ProviderKind::Disabled {
            return Err(Missing::Disabled);
        }
        let base_url = self.base_url.trim();
        if base_url.is_empty() && kind.default_base_url().is_empty() {
            return Err(Missing::BaseUrl);
        }
        let model = self.model.trim();
        if model.is_empty() {
            return Err(Missing::Model);
        }
        if kind.requires_api_key() && self.api_key.is_none() {
            return Err(Missing::ApiKey);
        }
        let mut config = match kind {
            ProviderKind::OpenAiCompatible => ProviderConfig::openai_compatible(base_url, model),
            ProviderKind::Anthropic => ProviderConfig::anthropic(model),
            ProviderKind::Ollama => ProviderConfig::ollama(model),
            // 上面已经拦掉；留着是因为穷尽匹配比 `_ =>` 值钱（加形态时编译器会提醒）。
            ProviderKind::Disabled => return Err(Missing::Disabled),
        };
        if !base_url.is_empty() {
            config.base_url = base_url.to_string();
        }
        if let Some(key) = self.api_key.as_deref() {
            config = config.with_api_key(key);
        }
        Ok(config)
    }

    /// 本次的密钥（脱敏用；空值给 `None`）。
    fn secret(&self) -> Option<&str> {
        self.api_key.as_deref().filter(|key| !key.trim().is_empty())
    }
}

/// 读设置文件并测一次连接（真 provider、真网络、带时限）。
#[must_use]
pub fn probe_file(path: &Path, timeout: Duration) -> ProbeOutcome {
    probe_with(&RigProvider::factory(), &AiSettings::load(path), timeout)
}

/// 测一次连接（工厂可注入：单测用计数假件证明「未配置时一次都没发」）。
///
/// # Panics
///
/// 在已有 tokio 运行时上下文里调用会 panic（`core::download::blocking::block_on` 的既有约定）；
/// 本函数的两个调用方（GUI 的 thread-pool 任务、CLI）都不在运行时里。
#[must_use]
pub fn probe_with(
    factory: &ProviderFactory,
    settings: &AiSettings,
    timeout: Duration,
) -> ProbeOutcome {
    let config = match settings.to_config() {
        Ok(config) => config,
        Err(missing) => return ProbeOutcome::NotConfigured { missing },
    };
    let target = Target {
        base_url: config.resolved_base_url().to_string(),
        model: config.model.clone(),
    };

    let provider = match factory(&config) {
        Ok(provider) => provider,
        Err(error) => return outcome_from_error(&error, settings, target),
    };

    let request = ChatRequest {
        model: target.model.clone(),
        messages: vec![ChatMessage::user(PING)],
        tools: Vec::new(),
        temperature: Some(0.0),
        max_tokens: Some(16),
    };

    unlucky_core::download::blocking::block_on(async move {
        match tokio::time::timeout(timeout, provider.complete(&request, &NoopObserver)).await {
            Ok(Ok(_reply)) => ProbeOutcome::Passed { target },
            Ok(Err(error)) => outcome_from_error(&error, settings, target),
            Err(_) => ProbeOutcome::Timeout {
                target,
                detail: format!("{} s", timeout.as_secs_f32()),
            },
        }
    })
}

/// provider 错误 → 分类结论（`detail` 先脱敏再出门；分类来自 [`ProviderError::kind`]）。
fn outcome_from_error(
    error: &ProviderError,
    settings: &AiSettings,
    target: Target,
) -> ProbeOutcome {
    let detail = scrub(&error.to_string(), settings.secret());
    match error.kind() {
        ProviderFailure::Auth => ProbeOutcome::Auth { target, detail },
        ProviderFailure::ModelNotFound => ProbeOutcome::ModelNotFound { target, detail },
        ProviderFailure::Unreachable => ProbeOutcome::Unreachable { target, detail },
        ProviderFailure::Timeout => ProbeOutcome::Timeout { target, detail },
        // 工厂阶段就报「配置不完整」（`validate()` 拒绝）：仍然没发请求，但缺项已经在上游
        // `to_config()` 拦过，这里退到 `Provider` 这个具名缺项。
        ProviderFailure::NotConfigured => ProbeOutcome::NotConfigured {
            missing: Missing::Provider,
        },
        ProviderFailure::Other => ProbeOutcome::Failed { target, detail },
    }
}

/// 文本脱敏：先走 agent 的通用脱敏（主目录 / Bearer），再把本次密钥原文换成 [`MASK`]。
///
/// 第二道是必须的：`redact_text` 认不出「某个服务端回显的自定义密钥」，只有本模块知道本次的
/// 密钥原文。长度上限 400：错误体可能是整页 HTML，界面上放不下，日志里也没必要。
pub fn scrub(text: &str, secret: Option<&str>) -> String {
    let mut out = redact_text(text);
    if let Some(secret) = secret.filter(|value| !value.trim().is_empty()) {
        out = out.replace(secret, MASK);
    }
    out.chars().take(400).collect()
}

#[cfg(test)]
#[path = "probe/tests.rs"]
mod tests;
