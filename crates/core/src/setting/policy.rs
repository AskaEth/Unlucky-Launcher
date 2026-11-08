//! 键的 AI 边界：可写白名单与敏感分级（弹窗确认）。
//!
//! 出处：`DevDocs/PlanDoc/02-ai-agent-design.md` §3.1（AI 安全边界：白名单 + 类型/范围校验 +
//! 分级确认）、`DevDocs/PlanDoc/04-data-and-config.md` §3（schema 化）。
//!
//! 从 [`super::spec`] 拆出来的原因很实际：`spec.rs` 摸到 400 行硬上限（`guards.yml` 的
//! file-size 门禁），而「策略」与「键的形状」本来就是两件事。

/// 敏感分级：决定 AI 改这个键时要不要弹窗。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Sensitivity {
    /// 普通键：静默生效。
    Low,
    /// 静默生效但审计标记（便于回看「AI 动过什么」）。
    Medium,
    /// 敏感键：必须 UI 弹窗确认。
    High,
}

impl Sensitivity {
    /// 稳定名。
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    /// 是否必须人工确认。
    #[must_use]
    pub const fn requires_confirmation(&self) -> bool {
        matches!(self, Self::High)
    }
}

/// 一个键的 AI 可写策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiPolicy {
    /// AI 是否允许写这个键（白名单）。
    pub writable: bool,
    /// 敏感分级。
    pub sensitivity: Sensitivity,
}

impl AiPolicy {
    /// 普通键：AI 可写，静默生效。
    #[must_use]
    pub const fn silent() -> Self {
        Self {
            writable: true,
            sensitivity: Sensitivity::Low,
        }
    }

    /// 可写但要审计标记。
    #[must_use]
    pub const fn audited() -> Self {
        Self {
            writable: true,
            sensitivity: Sensitivity::Medium,
        }
    }

    /// 可写但必须弹窗确认（java 路径 / 数据目录 / 代理 / AI 端点 / 隔离档位）。
    #[must_use]
    pub const fn sensitive() -> Self {
        Self {
            writable: true,
            sensitivity: Sensitivity::High,
        }
    }

    /// 已废弃键：AI 不可写，读兼容映射到替代键后由替代键承担策略。
    #[must_use]
    pub const fn retired() -> Self {
        Self {
            writable: false,
            sensitivity: Sensitivity::Low,
        }
    }

    /// 锁死：AI 永远不能写（凭据/密钥，D9「AI 永不触凭据」）。
    #[must_use]
    pub const fn locked() -> Self {
        Self {
            writable: false,
            sensitivity: Sensitivity::High,
        }
    }
}
