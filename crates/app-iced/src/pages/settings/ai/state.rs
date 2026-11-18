//! 「助手」分组的运行期状态：四条草稿 + 一颗探测结论 + 校验。
//!
//! 状态住在 `pages::settings::Groups.ai_about`（[`State`]），由 `App` 持有：
//! 用户正在输入的值**不能**放进 `pages::Model`（那个模型每次事件都整棵重建，见
//! `app/state.rs` 里 `download_draft` 的同一条理由）。

use unlucky_i18n::Key;

use super::outcome::{MissingField, ProbeKind, ProbeResult};

/// 四个提供商（值 = `ai.provider` 的词表，真源是 `core::setting::keys::AI_PROVIDERS`，
/// `ai/tests.rs` 里对表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProviderId {
    /// OpenAI 兼容（含 llama.cpp / LM Studio / DeepSeek / Qwen）
    #[default]
    OpenAiCompatible,
    /// Anthropic 原生
    Anthropic,
    /// Ollama 原生
    Ollama,
    /// 禁用
    Disabled,
}

impl ProviderId {
    /// 四态（界面上的顺序 = 这张表）。
    pub const ALL: [Self; 4] = [
        Self::OpenAiCompatible,
        Self::Anthropic,
        Self::Ollama,
        Self::Disabled,
    ];

    /// `ai.provider` 的取值。
    #[must_use]
    pub const fn setting(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "openai_compatible",
            Self::Anthropic => "anthropic",
            Self::Ollama => "ollama",
            Self::Disabled => "disabled",
        }
    }

    /// 取值 → 四态（未知/空值回落 OpenAI 兼容，与 schema 的默认值同一档）。
    #[must_use]
    pub fn from_setting(value: &str) -> Self {
        match value.trim() {
            "anthropic" => Self::Anthropic,
            "ollama" => Self::Ollama,
            "disabled" => Self::Disabled,
            _ => Self::OpenAiCompatible,
        }
    }

    /// 该形态有没有统一默认端点（没有 ⇒ base_url 必填）。
    #[must_use]
    pub const fn default_base_url(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "",
            Self::Anthropic => "https://api.anthropic.com",
            Self::Ollama => "http://127.0.0.1:11434",
            Self::Disabled => "",
        }
    }

    /// 界面标签 key。
    #[must_use]
    pub const fn label(self) -> Key {
        match self {
            Self::OpenAiCompatible => Key::SettingsAiProviderOpenAi,
            Self::Anthropic => Key::SettingsAiProviderAnthropic,
            Self::Ollama => Key::SettingsAiProviderOllama,
            Self::Disabled => Key::SettingsAiProviderDisabled,
        }
    }
}

/// 用户刚输入的密钥草稿：`Debug` 掩码，只有 [`Secret::expose`] 能拿到明文。
///
/// 为什么不用裸 `String`：`Groups` / `State` 都 `derive(Debug)`，`App` 里任何一处 `{:?}`（日志、
/// panic 消息、测试断言失败输出）都会把明文打出来。类型自己守住这一条，比每次记得手写便宜。
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// 从输入框的内容构造。
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// 是否空（空白串也算空）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
    }

    /// 明文（**只给落盘用**；不要拿它拼任何上屏/日志文本）。
    #[must_use]
    pub fn expose(&self) -> &str {
        self.0.trim()
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_empty() {
            f.write_str("Secret(空)")
        } else {
            f.write_str("Secret(***)")
        }
    }
}

/// 设置文件里那一份真值（`ai.*` 五键）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Snapshot {
    /// `ai.enabled`
    pub enabled: bool,
    /// `ai.provider`
    pub provider: ProviderId,
    /// `ai.base_url`
    pub base_url: String,
    /// `ai.model`
    pub model: String,
    /// 已存密钥的字符数（0 = 没存）。**只带长度不带值**：DTO 里不该有密钥。
    pub key_chars: usize,
}

/// 探测三态。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Probe {
    /// 还没测
    #[default]
    Idle,
    /// 正在测
    Running,
    /// 测完了
    Done(ProbeResult),
}

/// 保存结果（用户点「保存」之后的回执）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveNotice {
    /// 已落盘
    Saved,
    /// 被校验拦下（没写盘）
    Refused(Refusal),
    /// schema/磁盘拒绝（没写盘）
    Failed(String),
    /// 设置文件不可用（内存态运行）
    NoStorage,
}

/// 拒绝保存的原因（本地校验，先于 schema）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// 端点不是 http(s)
    BaseUrl,
    /// 缺模型名
    Model,
}

impl Refusal {
    /// 界面文案 key。
    #[must_use]
    pub const fn label(self) -> Key {
        match self {
            Self::BaseUrl => Key::SettingsAiReasonBaseUrl,
            Self::Model => Key::SettingsAiReasonModel,
        }
    }
}

/// 「助手」分组的运行期状态。
#[derive(Debug)]
pub struct State {
    /// `ai.enabled`
    pub enabled: bool,
    /// `ai.provider`
    pub provider: ProviderId,
    /// `ai.base_url` 草稿
    pub base_url: String,
    /// `ai.model` 草稿
    pub model: String,
    /// 新密钥草稿（空 = 不改已存的那把）
    pub key: Secret,
    /// 已存密钥的字符数（0 = 没存）
    pub key_chars: usize,
    /// 是否明文显示密钥输入框
    pub reveal: bool,
    /// 有未保存的改动
    pub dirty: bool,
    /// 设置文件可用（内存态运行时置假：保存与测试都禁用）
    pub storage: bool,
    /// 上一次保存的回执
    pub notice: Option<SaveNotice>,
    /// 探测结论
    pub probe: Probe,
}

impl Default for State {
    fn default() -> Self {
        Self {
            enabled: true,
            provider: ProviderId::OpenAiCompatible,
            base_url: String::new(),
            model: String::new(),
            key: Secret::default(),
            key_chars: 0,
            reveal: false,
            dirty: false,
            storage: false,
            notice: None,
            probe: Probe::Idle,
        }
    }
}

impl State {
    /// 用设置文件里的真值重置草稿（**只在没脏的时候**：不许冲掉用户正在输入的内容）。
    ///
    /// 返回是否真的改了（调用方据此决定要不要重建视图模型）。
    pub fn hydrate(&mut self, snapshot: Snapshot) -> bool {
        self.storage = true;
        self.key_chars = snapshot.key_chars;
        if self.dirty {
            return false;
        }
        let changed = self.enabled != snapshot.enabled
            || self.provider != snapshot.provider
            || self.base_url != snapshot.base_url
            || self.model != snapshot.model;
        self.enabled = snapshot.enabled;
        self.provider = snapshot.provider;
        self.base_url = snapshot.base_url;
        self.model = snapshot.model;
        self.keep_probe_if_targeted();
        changed
    }

    /// 端点/模型变了之后，「已通过」的旧结论不再是关于现在这套配置的——清掉，免得说谎。
    fn keep_probe_if_targeted(&mut self) {
        if let Probe::Done(result) = &self.probe {
            if result.base_url != self.resolved_base_url() || result.model != self.model.trim() {
                self.probe = Probe::Idle;
            }
        }
    }

    /// 实际生效的端点（空 = 用该形态的默认值）。
    #[must_use]
    pub fn resolved_base_url(&self) -> String {
        let draft = self.base_url.trim();
        if draft.is_empty() {
            self.provider.default_base_url().to_string()
        } else {
            draft.to_string()
        }
    }

    /// 配置齐了没有（不齐时「测试连接」禁用，界面上说缺哪一项）。
    #[must_use]
    pub fn missing(&self) -> Option<MissingField> {
        if !self.enabled || self.provider == ProviderId::Disabled {
            return Some(MissingField::Disabled);
        }
        if self.resolved_base_url().is_empty() {
            return Some(MissingField::BaseUrl);
        }
        if self.model.trim().is_empty() {
            return Some(MissingField::Model);
        }
        if self.provider == ProviderId::Anthropic && self.key.is_empty() && self.key_chars == 0 {
            return Some(MissingField::ApiKey);
        }
        None
    }

    /// 现在能不能点「测试连接」。
    #[must_use]
    pub fn can_test(&self) -> bool {
        self.storage && self.missing().is_none() && !matches!(self.probe, Probe::Running)
    }

    /// 状态矩阵的三档（未配置 / 已配置 / 已通过 / 测试失败）。
    #[must_use]
    pub fn status(&self) -> Status {
        match &self.probe {
            Probe::Done(result) if result.kind == ProbeKind::Passed => Status::Verified,
            Probe::Done(result) if result.kind.is_failure() => Status::Failed,
            _ if self.missing().is_some() => Status::Unconfigured,
            _ => Status::Configured,
        }
    }

    /// 保存前校验（用户输入侧）；过不了就不写盘。
    ///
    /// # Errors
    /// 端点不是 `http(s)://` 或该形态缺模型名时返回原因。
    pub fn validate(&self) -> Result<(), Refusal> {
        if self.provider == ProviderId::Disabled || !self.enabled {
            return Ok(());
        }
        let base_url = self.resolved_base_url();
        if base_url.is_empty()
            || !(base_url.starts_with("http://") || base_url.starts_with("https://"))
        {
            return Err(Refusal::BaseUrl);
        }
        if self.model.trim().is_empty() {
            return Err(Refusal::Model);
        }
        Ok(())
    }

    /// 取证开关 `ULL_AI_STATE`（只给 `--screenshot` 用，见 `app/state/construct.rs`）。
    ///
    /// 为什么要它：D30 的状态矩阵要出「未配置 / 已配置并测过 / 测试失败」三张图，而真实路径要
    /// 点按钮、要联网。语义与 `ULL_UPDATE_STATE` / `ULL_ACCOUNT_STATE` 一致：只影响这一次进程。
    #[must_use]
    pub fn screenshot_seed(raw: &str) -> Option<Self> {
        let mut state = Self {
            storage: true,
            ..Self::default()
        };
        match raw.trim() {
            "unconfigured" => {}
            "configured" => {
                state.base_url = "https://api.deepseek.com/v1".to_string();
                state.model = "deepseek-chat".to_string();
                state.key_chars = 32;
            }
            other => {
                let kind = match other {
                    "verified" => ProbeKind::Passed,
                    "failed-auth" => ProbeKind::Auth,
                    "failed-model" => ProbeKind::ModelNotFound,
                    "failed-unreachable" => ProbeKind::Unreachable,
                    "failed-timeout" => ProbeKind::Timeout,
                    _ => return None,
                };
                state.base_url = "https://api.deepseek.com/v1".to_string();
                state.model = "deepseek-chat".to_string();
                state.key_chars = 32;
                // r1 复审 P2-1：种子的**形状**必须由产品路径（`ProbeResult::from_outcome`）决定，
                // 这里只伪造「结论是哪一档」。手填 `ProbeResult` 的字段会让「失败态目标为空」
                // 这类缺陷只在产品上出现、截图永远看不见。
                state.probe = Probe::Done(ProbeResult::screenshot_seed(
                    kind,
                    state.resolved_base_url(),
                    state.model.clone(),
                    412,
                ));
            }
        }
        Some(state)
    }
}

/// 页头的状态档（D30 的状态矩阵：四档各自一句文案 + 一种颜色）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// 未配置（缺项，按钮禁用）
    Unconfigured,
    /// 已配置（还没测或测过但目标改了）
    Configured,
    /// 连接已通过
    Verified,
    /// 测试失败
    Failed,
}

impl Status {
    /// 文案 key。
    #[must_use]
    pub const fn label(self) -> Key {
        match self {
            Self::Unconfigured => Key::SettingsAiStatusUnconfigured,
            Self::Configured => Key::SettingsAiStatusConfigured,
            Self::Verified => Key::SettingsAiStatusVerified,
            Self::Failed => Key::SettingsAiStatusTestFailed,
        }
    }
}
