//! 内置对话会话（M384）：[`Brain`] 的**生产**构造点。
//!
//! ## 它补的是哪个洞
//!
//! 到 M383 为止，`Brain::new` 在全仓的**唯一**调用点是测试支撑
//! （`crates/agent/tests/support/mod.rs`）——于是「工具内核在 GUI 上根本不可达」
//! （`DevDocs/artifacts/gapreg/GAPS.md` §2.7 A1/A2）。零件其实都是生产质量：
//!
//! | 零件 | 在哪 | 状态 |
//! |---|---|---|
//! | 真 provider | [`crate::brain::rig::RigProvider`] | 生产（`ull agent probe` 的「测试连接」用的就是它） |
//! | 工具内核 | [`crate::tools::build_v1_registry`] | 生产（MCP 侧在用） |
//! | 审计 | [`crate::audit::JsonlAudit`] | 生产（MCP 侧在用） |
//! | 确认/YOLO | [`crate::confirm::YoloSwitch`] | 生产（编排层判定；M389 起是活开关） |
//!
//! 缺的只有**装配**：把五样东西按固定顺序拼起来，并持有跨轮的历史。本模块就是那一处，
//! 两个形态（GUI 与将来的别的壳）都从这里出会话，不再各自抄一遍装配代码。
//!
//! ## 装配顺序为什么是这个顺序
//!
//! ```text
//! ports ──► build_v1_registry ──► Arc<ToolRegistry>   （工具是唯一真相源，先立）
//! config ─► factory ──────────► Arc<dyn ChatProvider> （配置不全在这里 fail-fast）
//! data root ─► audit_dir ─────► JsonlAudit           （审计是旁路，但**装配期**就要落定目录）
//! gate / policy ──────────────► BrainDeps            （谁问人、问不问，全在编排层）
//!         └──────────────────► Brain                 （循环 + 工具编排）
//! ```
//!
//! **provider 先于 Brain 构造**：配置不完整（缺端点/模型/密钥）时立刻返回可读错误，调用方据此
//! 显式降级（GUI 走「未配置」而不是把用户丢进一个「看起来很能聊其实每次都失败」的会话）。
//!
//! ## 不做什么
//!
//! - **不持有 UI**：事件只经 [`AgentEventSink`]，确认只经 [`ConfirmGate`]——两者都由装配方实现，
//!   本模块不认识 iced / 队列 / 线程；
//! - **不做线程**：`run_turn` 是 async 的，谁在哪个线程上驱动由调用方决定（GUI 在后台线程里
//!   用 core 的阻塞桥跑，MCP 侧不走本模块）；
//! - **不落会话历史到磁盘**：历史在内存里（`Vec<ChatMessage>`），落盘是 P1 的 `agent/sessions/*.jsonl`
//!   欠账，本模块先用 [`Session::history`] 暴露出来。

use std::path::PathBuf;
use std::sync::Arc;

use crate::audit::{audit_dir, AuditSink, JsonlAudit};
use crate::brain::chat::{ChatMessage, ProviderFactory};
use crate::brain::provider::{ProviderConfig, ProviderError};
use crate::brain::{Brain, BrainConfig, BrainDeps, BrainError, CancelToken, TurnOutcome};
use crate::clock::{Clock, SharedClock, SystemClock};
use crate::confirm::{ConfirmGate, YoloSwitch};
use crate::event::AgentEventSink;
use crate::port::Ports;
use crate::registry::{RegistryError, ToolRegistry};
use crate::tools::build_v1_registry;

/// 装配失败的两类原因（都在装配期，不是对话期）。
#[derive(Debug)]
pub enum SessionError {
    /// 注册表装配失败（工具名重复 / schema 非法）：属于编码错误。
    Registry(RegistryError),
    /// provider 造不出来：配置不完整（缺端点/模型/密钥）或端点构造失败。
    Provider(ProviderError),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Registry(error) => write!(f, "工具注册表装配失败：{error}"),
            Self::Provider(error) => write!(f, "provider 装配失败：{error}"),
        }
    }
}

impl std::error::Error for SessionError {}

impl From<RegistryError> for SessionError {
    fn from(error: RegistryError) -> Self {
        Self::Registry(error)
    }
}

impl From<ProviderError> for SessionError {
    fn from(error: ProviderError) -> Self {
        Self::Provider(error)
    }
}

/// 一次会话的装配参数（**不含** gate / sink：那两个由 UI 侧提供）。
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// provider 快照（设置 → AI 的四项 + 采样参数）。
    pub provider: ProviderConfig,
    /// YOLO 的判据（M389：活开关，不是快照）。
    ///
    /// `new` 收 `impl Into<YoloSwitch>`：既有调用方传 `ConfirmPolicy`（等价于固定开关，
    /// 签名不必改——`crates/agent/tests` 在 scope 外）；生产装配点随后用
    /// [`SessionConfig::with_yolo_switch`] 换成「现读设置文件」的探针。
    pub yolo: YoloSwitch,
    /// 数据根（`<launcher_home>`）：审计落 `<root>/agent/audit/audit-<date>.jsonl`。
    pub data_root: PathBuf,
    /// agent 会话 id（审计记录里的 `agent_session_id`，不是游戏会话 id）。
    pub agent_session_id: String,
    /// 循环参数（步数上限 / 系统提示词）。
    pub brain: BrainConfig,
    /// 空闲时限（M388）：**连续**这么久没有任何响应就判超时。
    ///
    /// 这里是「配置里的值」；每轮开头还会被 `ULL_AGENT_TIMEOUT_SECS` 覆盖一次
    /// （见 [`crate::brain::guard::resolve_timeout`]）——所以它不是启动期快照。
    pub request_timeout: std::time::Duration,
}

impl SessionConfig {
    /// 由 provider 快照与数据根装配，会话 id 按当前时刻生成（审计里可区分进程内多次会话）。
    #[must_use]
    pub fn new(
        provider: ProviderConfig,
        policy: impl Into<YoloSwitch>,
        data_root: impl Into<PathBuf>,
    ) -> Self {
        let stamp = SystemClock.now_unix_ms();
        Self {
            provider,
            yolo: policy.into(),
            data_root: data_root.into(),
            agent_session_id: format!("gui-{stamp}"),
            brain: BrainConfig::default(),
            request_timeout: crate::brain::guard::DEFAULT_IDLE_TIMEOUT,
        }
    }

    /// 换 YOLO 的判据（M389）：GUI / MCP 的装配点给「现读 `ai.yolo`」的探针。
    ///
    /// 换进来之后，设置里一改 `ai.yolo`，**这一条已经建好的会话**下一次高敏感工具调用就按新值判。
    #[must_use]
    pub fn with_yolo_switch(mut self, yolo: YoloSwitch) -> Self {
        self.yolo = yolo;
        self
    }

    /// 换会话 id（测试与多会话场景用）。
    #[must_use]
    pub fn with_session_id(mut self, agent_session_id: impl Into<String>) -> Self {
        self.agent_session_id = agent_session_id.into();
        self
    }

    /// 换空闲时限（装配方给出的「配置层那一位」；每轮还会被 `ULL_AGENT_TIMEOUT_SECS` 覆盖）。
    #[must_use]
    pub fn with_request_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.request_timeout = timeout;
        self
    }
}

/// 一个能连续跑多轮的内置会话：`Brain` + 归属它的历史 + 装配期那份 provider 配置。
pub struct Session {
    brain: Brain,
    registry: Arc<ToolRegistry>,
    history: Vec<ChatMessage>,
    agent_session_id: String,
    /// 取消旗标（M388）：UI 侧持有同一份，按「停止」时置位。
    cancel: CancelToken,
}

impl Session {
    /// 装配一条会话。
    ///
    /// `factory` 决定用哪个 provider 实现（生产是 [`crate::brain::rig::RigProvider::factory`]，
    /// 测试是假件）；**先造 provider 再装注册表**的反面——注册表先立，因为它是纯内存的，
    /// 而 provider 失败要尽早冒出来（配置不完整时连审计目录都不该建）。
    ///
    /// # Errors
    /// 注册表非法（[`SessionError::Registry`]）或 provider 造不出来（[`SessionError::Provider`]）。
    pub fn open(
        ports: &Ports,
        factory: &ProviderFactory,
        config: SessionConfig,
        gate: Arc<dyn ConfirmGate>,
        sink: Arc<dyn AgentEventSink>,
    ) -> Result<Self, SessionError> {
        // 先验配置再问工厂：`validate()` 是「配置不完整」的**唯一**口径（缺端点/模型/密钥），
        // 不能指望每个工厂实现都记得调用它——漏了就会变成「装好了，但第一句话才炸」。
        config.provider.validate()?;
        let provider = factory(&config.provider)?;
        let registry = Arc::new(build_v1_registry(ports)?);
        let clock: SharedClock = Arc::new(SystemClock);
        let audit: Arc<dyn AuditSink> = Arc::new(JsonlAudit::new(
            audit_dir(&config.data_root),
            Arc::clone(&clock),
        ));
        let deps = BrainDeps::new(audit, gate, config.yolo.clone(), clock, sink);
        // M388：取消旗标由会话创建、交给 UI（`Session::cancel_handle`）。`Brain` 是它的读者。
        let cancel = CancelToken::new();
        let brain = Brain::new(provider, Arc::clone(&registry), deps, config.brain.clone())
            .with_cancel(cancel.clone())
            .with_timeout(config.request_timeout);
        Ok(Self {
            brain,
            registry,
            history: Vec::new(),
            agent_session_id: config.agent_session_id,
            cancel,
        })
    }

    /// 取消旗标的句柄（M388）：交给 UI，按「停止」时 [`CancelToken::cancel`]。
    ///
    /// 必须是**同一份**（clone 共享同一个 `Arc`）：这不是「再开一个取消通道」，而是把内核
    /// 正在等的那一个 future 的判据交给外层。`crates/app-iced/src/presenter/real.rs` 就是这么接的。
    #[must_use]
    pub fn cancel_handle(&self) -> CancelToken {
        self.cancel.clone()
    }

    /// 跑一轮对话（历史由本会话持有，调用方不用管）。
    ///
    /// ## 超时 / 取消之后为什么历史要回滚（M388）
    ///
    /// `run_turn` 一开始就把用户那句推进了历史，中途还会推进「带工具调用的 assistant」与
    /// 「tool 结果」。被打断的那一轮留下的是一条**没有结局的半截对话**：下一轮把它当上下文
    /// 发出去，模型会看到一个「用户问了、助手没答」的悬空消息（有的端点直接拒）。所以
    /// 超时/取消时把历史恢复成**提交前的快照**——这也正是「取消后必须能立刻重试」的地基：
    /// 重试用的是干净的历史，不是半截的那一份。
    ///
    /// # Errors
    /// AI 被禁用（[`BrainError::Disabled`]）、provider 报错（[`BrainError::Provider`]）、
    /// 空闲超时（[`BrainError::Timeout`]）或本轮被取消（[`BrainError::Cancelled`]）。
    pub async fn run_turn(&mut self, user_input: &str) -> Result<TurnOutcome, BrainError> {
        // 旗标**每轮清一次**：上一条「停止」不许把下一句话也取消掉（重试的前提）。
        self.cancel.reset();
        let snapshot = self.history.clone();
        let session_id = self.agent_session_id.clone();
        let outcome = self
            .brain
            .run_turn(&session_id, &mut self.history, user_input)
            .await;
        if matches!(
            outcome,
            Err(BrainError::Cancelled | BrainError::Timeout { .. })
        ) {
            self.history = snapshot;
        }
        outcome
    }

    /// 当前历史（只读；含 user / assistant / tool 三种消息，**不含**每轮重注入的 system）。
    #[must_use]
    pub fn history(&self) -> &[ChatMessage] {
        &self.history
    }

    /// 会话 id（审计记录里的那个）。
    #[must_use]
    pub fn agent_session_id(&self) -> &str {
        &self.agent_session_id
    }

    /// 装配后的 provider 配置（`Debug` 里 key 是掩码）。
    #[must_use]
    pub fn provider_config(&self) -> &ProviderConfig {
        self.brain.provider_config()
    }

    /// 这条会话挂着的工具名（顺序 = 模型看到的顺序；报告与测试都按它对齐）。
    #[must_use]
    pub fn tool_names(&self) -> Vec<&str> {
        self.registry.names()
    }
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("agent_session_id", &self.agent_session_id)
            .field("provider", &self.brain.provider_config().redacted_debug())
            .field("tools", &self.registry.len())
            .field("history", &self.history.len())
            .finish()
    }
}

#[cfg(test)]
mod tests;
