//! 「测试连接」的结论（**界面要画的那一份**）：缺项 / 分类 / 目标 / 耗时 / 脱敏详情。
//!
//! 为什么从 `state.rs` 拆出来：那个文件被 400 行硬线卡着（`components/gate.rs` 的
//! `every_source_file_stays_under_the_line_limit` 按全仓判红），而这几件是**纯数据 + 翻译**，
//! 与草稿状态机没有耦合。分类判断不在本文件：那是
//! `unlucky_cli::commands::agent::probe` 的事（四类各有真 TCP 假服务器用例），这里只翻译。

use unlucky_i18n::Key;

/// 缺哪一项（「未配置」时界面上要说清差什么）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingField {
    /// 已关闭 / 提供商是 disabled
    Disabled,
    /// 提供商取值不认识
    Provider,
    /// 端点为空
    BaseUrl,
    /// 模型名为空
    Model,
    /// Anthropic 缺密钥
    ApiKey,
}

impl MissingField {
    /// 界面文案 key。
    #[must_use]
    pub const fn label(self) -> Key {
        match self {
            Self::Disabled => Key::SettingsAiMissingDisabled,
            Self::Provider => Key::SettingsAiReasonProvider,
            Self::BaseUrl => Key::SettingsAiMissingBaseUrl,
            Self::Model => Key::SettingsAiMissingModel,
            Self::ApiKey => Key::SettingsAiMissingApiKey,
        }
    }
}

/// 「测试连接」的结论大类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeKind {
    /// 通过
    Passed,
    /// 鉴权失败
    Auth,
    /// 模型不存在
    ModelNotFound,
    /// 端点不可达
    Unreachable,
    /// 超时
    Timeout,
    /// 其它失败
    Failed,
    /// 配置不完整，**没有发请求**
    NotConfigured,
}

impl ProbeKind {
    /// 界面文案 key。
    #[must_use]
    pub const fn label(self) -> Key {
        match self {
            Self::Passed => Key::SettingsAiTestPassed,
            Self::Auth => Key::SettingsAiTestAuth,
            Self::ModelNotFound => Key::SettingsAiTestModel,
            Self::Unreachable => Key::SettingsAiTestUnreachable,
            Self::Timeout => Key::SettingsAiTestTimeout,
            Self::Failed => Key::SettingsAiTestOther,
            Self::NotConfigured => Key::SettingsAiTestNotConfigured,
        }
    }

    /// 是不是「不通」那一类（界面用错误色）。
    #[must_use]
    pub const fn is_failure(self) -> bool {
        !matches!(self, Self::Passed)
    }
}

/// 一次探测的结论（数值 + 分类 + 脱敏后的详情）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeResult {
    /// 大类。
    pub kind: ProbeKind,
    /// 未配置时缺的那一项。
    pub missing: Option<MissingField>,
    /// 实际请求的端点（`resolved_base_url`）。
    pub base_url: String,
    /// 实际请求的模型名。
    pub model: String,
    /// 耗时（毫秒）。
    pub elapsed_ms: u128,
    /// 脱敏后的原因（可能为空）。
    pub detail: String,
}

impl ProbeResult {
    /// 「测试连接」的结论（`unlucky_cli` 的探测器产出）→ 界面要画的那一份。
    ///
    /// 只做**翻译**：分类判断全在 `unlucky_cli::commands::agent::probe`（那边有四条真 TCP 假
    /// 服务器用例逐个钉住），这里不重新猜一遍。`elapsed_ms` 由发起处计时（含建连 + 首 token）。
    #[must_use]
    pub fn from_outcome(
        outcome: unlucky_cli::commands::agent::probe::ProbeOutcome,
        elapsed_ms: u128,
    ) -> Self {
        use unlucky_cli::commands::agent::probe::{Missing, ProbeOutcome};

        let missing = match &outcome {
            ProbeOutcome::NotConfigured { missing } => Some(match missing {
                Missing::Disabled => MissingField::Disabled,
                Missing::Provider => MissingField::Provider,
                Missing::BaseUrl => MissingField::BaseUrl,
                Missing::Model => MissingField::Model,
                Missing::ApiKey => MissingField::ApiKey,
            }),
            _ => None,
        };
        // r1 复审 P2-1：**失败也带 target**（端点 + 模型）。以前这里对失败类一律给空串，界面上
        // 就成了「目标：  · 模型 …」，用户改完 key 也无法确认改的是不是那一份配置。
        let (kind, target, detail) = match outcome {
            ProbeOutcome::NotConfigured { .. } => (ProbeKind::NotConfigured, None, String::new()),
            ProbeOutcome::Passed { target } => (ProbeKind::Passed, Some(target), String::new()),
            ProbeOutcome::Auth { target, detail } => (ProbeKind::Auth, Some(target), detail),
            ProbeOutcome::ModelNotFound { target, detail } => {
                (ProbeKind::ModelNotFound, Some(target), detail)
            }
            ProbeOutcome::Unreachable { target, detail } => {
                (ProbeKind::Unreachable, Some(target), detail)
            }
            ProbeOutcome::Timeout { target, detail } => (ProbeKind::Timeout, Some(target), detail),
            ProbeOutcome::Failed { target, detail } => (ProbeKind::Failed, Some(target), detail),
        };
        Self {
            kind,
            missing,
            base_url: target
                .as_ref()
                .map_or_else(String::new, |t| t.base_url.clone()),
            model: target.map_or_else(String::new, |t| t.model.clone()),
            elapsed_ms,
            detail,
        }
    }

    /// 截图种子（`ULL_AI_STATE`）：**走 [`ProbeResult::from_outcome`] 的同一条路**。
    ///
    /// 为什么必须这样（r1 复审 P2-1）：上一版种子直接手填 `ProbeResult` 的字段，于是「失败态
    /// 的目标是空串」这个缺陷只在产品路径上出现，截图永远看不见——**出图不镜像产品路径，等于
    /// 自欺**。现在种子只会伪造「探测结论是什么」，形状（哪些字段有值）由产品那条路决定。
    #[must_use]
    pub fn screenshot_seed(
        kind: ProbeKind,
        base_url: String,
        model: String,
        elapsed_ms: u128,
    ) -> Self {
        use unlucky_cli::commands::agent::probe::{ProbeOutcome, Target};

        let target = Target { base_url, model };
        let detail = "ProviderResponseError: status 401 Unauthorized: {\"error\":{\"message\":\"Incorrect API key provided\"}}";
        let outcome = match kind {
            ProbeKind::Passed => ProbeOutcome::Passed { target },
            ProbeKind::Auth => ProbeOutcome::Auth {
                target,
                detail: detail.to_string(),
            },
            ProbeKind::ModelNotFound => ProbeOutcome::ModelNotFound {
                target,
                detail: detail.to_string(),
            },
            ProbeKind::Unreachable => ProbeOutcome::Unreachable {
                target,
                detail: "Http client error: error sending request".to_string(),
            },
            ProbeKind::Timeout => ProbeOutcome::Timeout {
                target,
                detail: "8 s".to_string(),
            },
            ProbeKind::Failed => ProbeOutcome::Failed {
                target,
                detail: detail.to_string(),
            },
            // 未配置态没有 target，也就不该有「测过」的结论：种子里退到「没发请求」。
            ProbeKind::NotConfigured => ProbeOutcome::NotConfigured {
                missing: unlucky_cli::commands::agent::probe::Missing::Model,
            },
        };
        Self::from_outcome(outcome, elapsed_ms)
    }

    /// 失败的三类之外的那几类（`NotConfigured` 没发请求）。
    #[must_use]
    pub fn sent_a_request(&self) -> bool {
        self.kind != ProbeKind::NotConfigured
    }
}
