//! 端口层（port）：agent 与 `ull-core` 各模块之间**唯一**的接触面。
//!
//! 为什么不直接 `use ull_core::*`：M12（setting）/ M14（game）/ M15（diagnose）/ M18（launch）
//! 与本 crate 并行开发，直接依赖会把 agent 的编译期绑死在别人的 API 日期上。这里用
//! **依赖倒置**——agent 只声明自己需要的形状（本文件的 trait + 结构体），由 `crates/app`
//! 在装配处用 `ull-core` 的实现去满足（`#![forbid(unsafe_code)]` 的纯适配层）。
//!
//! | 端口 | 提供的工具 | 对方模块 / 数据源 |
//! |------|-----------|------------------|
//! | [`SettingPort`] | `get_config` / `set_config` | M12 · `config/*.json` schema |
//! | [`GameSessionPort`] | `list_game_sessions` / `read_game_log` | M18 · `agent/sessions/game/{index,<id>}.jsonl` |
//! | [`DiagnosePort`] | `diagnose_crash` | M15 · 规则引擎 |
//!
//! 形状依据（以对方文件为准，改字段先改对方文件）：
//! - `DevDocs/SubAgentsTalk/launch-接口约定.md` §2/§5（事件行 + 索引行）
//! - `DevDocs/SubAgentsTalk/agent-ToolSpec约定.md`（本 crate 对外的对齐说明）
//!
//! 错误一律用 `Result<T, String>`：错误文本会**原样进工具结果**给模型与用户看
//! （rmcp 的 `CallToolResult::error` 同理——协议级错误不给模型看，见 research/02 §3.2）。
//! 因此端口实现写中文、面向用户、可执行（「配置键 `x` 不存在」而不是 `ENOENT`）。

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

use crate::confirm::Sensitivity;

pub mod game;

pub use game::{
    AccountPort, AccountSummary, GamePort, InstanceSummary, JavaRuntimeSummary, JavaStatus,
    VersionSummary,
};

/// 端口调用结果的错误文本（面向用户可读，直接进工具结果）。
pub type PortResult<T> = Result<T, String>;

/// agent 需要的全部端口。装配处（`crates/app` / `ull agent serve`）用它一次性注入。
#[derive(Clone)]
pub struct Ports {
    /// 配置读写（M12）。
    pub setting: Arc<dyn SettingPort>,
    /// 游戏会话与日志（M18）。
    pub sessions: Arc<dyn GameSessionPort>,
    /// 崩溃规则引擎（M15）。
    pub diagnose: Arc<dyn DiagnosePort>,
    /// 实例 / 版本 / Java（M14 + M33）。
    pub game: Arc<dyn GamePort>,
    /// 账户元数据（M31，**只有公开字段**，凭据永不出口）。
    pub accounts: Arc<dyn AccountPort>,
}

// ───────────────────────────── 配置（M12） ─────────────────────────────

/// AI 可写白名单里的一个键，带**敏感分级**（02 文档 §3.1 的「分级确认」依据）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SettingKey {
    /// 点分路径，如 `download.concurrency`、`java.selected_path`（键名以 M12 的 schema 为准）。
    pub path: String,
    /// 敏感等级（M12 `Low/Medium/High`）：只有 `High` 需要用户确认。
    pub sensitivity: Sensitivity,
    /// 值类型（`string` / `integer` / `boolean` / `enum` / `json`），用于错误提示。
    pub value_type: String,
    /// 一句话说明，同时进 `get_config` 概要与模型上下文。
    pub description: String,
}

/// 一次成功的写入，带旧值即「diff」（research/02 §6.4 的 dry-run 落点）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SettingChange {
    /// 被修改的键路径。
    pub path: String,
    /// 修改前的值；键此前不存在时为 `None`。
    pub old: Option<Value>,
    /// 写入后的值。
    pub new: Value,
}

/// 配置端口（M12 实现，`ull-core::setting` 的 schema API 直译）。
///
/// agent 侧只做**白名单 + 敏感分级 + 确认编排**，值本身的范围/类型校验归 setting 模块所有
/// （单一真相源：配置 schema 在 M12，agent 不复制一份）。
pub trait SettingPort: Send + Sync {
    /// 全量概要（`get_config()` 无参时返回）：键 → 当前值的扁平表，敏感键已脱敏。
    fn summary(&self) -> Value;
    /// 读单个键。键不存在时返回人类可读错误。
    ///
    /// **凭据契约**：`writable=false` 的凭据类键（`ai.api_key` 等）必须**拒绝或掩码**，
    /// 不得返回明文——agent 侧还有一道 `redact::is_sensitive_key` 守卫，但端口是数据的
    /// 第一道闸（模型上下文与对话历史都会带上这里的返回值）。
    fn get(&self, path: &str) -> PortResult<Value>;
    /// 写单个键（值校验由实现方负责），返回 diff。
    fn set(&self, path: &str, value: &Value) -> PortResult<SettingChange>;
    /// **AI 可写白名单**（含敏感分级）。不在表里的键 agent 一律拒绝。
    fn writable_keys(&self) -> Vec<SettingKey>;
    /// 配置 schema 的给 AI 版本（`get_config_schema` 工具）：只含 AI 可写键与
    /// `x-ai-sensitivity` 标注，凭据类键天然不在其中（M12 `SettingSchema::to_json_schema`）。
    fn schema(&self) -> Value;
}

// ──────────────────────── 游戏会话与日志（M18） ────────────────────────

/// `index.jsonl` 一行 → 一个会话摘要（同 id 多行由实现方取最后一行）。
///
/// 字段对齐 `launch-接口约定.md` §5；对方新增字段时这里补 `Option`，不破坏旧实现。
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct GameSessionSummary {
    /// 会话 id，格式 `YYYYMMDDTHHMMSS-<6 位 hex>`（UTC）。
    pub session_id: String,
    /// 实例 id。
    pub instance_id: Option<String>,
    /// 实例显示名。
    pub instance_name: Option<String>,
    /// MC 版本。
    pub game_version: Option<String>,
    /// 启动时间（RFC3339 UTC）。
    pub started_at: Option<String>,
    /// 结束时间；会话未结束时为 `None`。
    pub ended_at: Option<String>,
    /// 时长毫秒。
    pub duration_ms: u64,
    /// 退出码；未正常结束时可能缺失。
    pub exit_code: Option<i32>,
    /// `exit_type`：`normal|application_error|jvm_error|sigkill|sigterm|killed|interrupted`。
    pub exit_type: Option<String>,
    /// exit 0 但疑似静默失败（D8 升级 1）。
    pub silent_failure: bool,
    /// 是否检测到游戏窗口。
    pub window_detected: bool,
    /// 事件条数。
    pub event_count: u64,
    /// 是否发现 crash-report / hs_err。
    pub crash_found: bool,
    /// 规则引擎首选结论 id（`diagnosis` 事件的 top candidate）。
    pub diagnosis_top: Option<String>,
}

/// `read_game_log` 的查询条件：**按事件 seq 切片**，不是按行号
/// （stdout/stderr 是一行一事件，但同一条日志行可能被切成多块，见 launch §1）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogQuery {
    /// 起始 `seq`（含）。
    pub from_seq: Option<u64>,
    /// 结束 `seq`（含）。
    pub to_seq: Option<u64>,
    /// 只要这些 `kind`（空 = 全部）。未知 kind 由实现方忽略而不是报错（前向兼容）。
    pub kinds: Vec<String>,
    /// 最多返回多少条事件（工具层已钳到 [`crate::tools::session::MAX_LOG_EVENTS`]）。
    pub max_events: usize,
}

/// 一条事件行（`{v,seq,ts,t_ms,kind,session_id,payload}`），落盘时已脱敏。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LogEvent {
    /// 会话内单调递增序号。
    pub seq: u64,
    /// RFC3339 UTC。
    pub ts: String,
    /// 距会话开始的毫秒数。
    pub t_ms: u64,
    /// 事件类型（`stdout|stderr|exit|crash_found|diagnosis|…`）。
    pub kind: String,
    /// 事件负载，原样透传（已脱敏）。
    pub payload: Value,
}

/// 一页日志事件 + 全量统计（模型拿到摘要就不会盲目翻页，防 token 爆炸）。
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct LogPage {
    /// 会话 id。
    pub session_id: String,
    /// 文件内事件总数。
    pub total_events: u64,
    /// 命中过滤条件的事件数。
    pub matched_events: u64,
    /// 是否因 `max_events` 截断。
    pub truncated: bool,
    /// 全量 kind 计数（不受过滤影响）。
    pub kind_counts: BTreeMap<String, u64>,
    /// 首事件时间。
    pub first_ts: Option<String>,
    /// 末事件时间。
    pub last_ts: Option<String>,
    /// 本次返回的事件（已按 seq 升序）。
    pub events: Vec<LogEvent>,
}

/// 会话端口（M18 实现，`ull-core::launch::session_index` + `session` 的读侧）。
///
/// 读取**必须容忍最后一行不完整**（会话被 kill 时 jsonl 尾部可能截断）——这是实现方义务，
/// 由 `launch-接口约定.md` §1 规定，agent 只消费结果。
pub trait GameSessionPort: Send + Sync {
    /// 按 `started_at` 倒序、按 `session_id` 去重后的会话列表；`instance_id` 非空时过滤。
    fn list_sessions(
        &self,
        limit: usize,
        instance_id: Option<&str>,
    ) -> PortResult<Vec<GameSessionSummary>>;
    /// 按 [`LogQuery`] 切片读取一个会话的事件流。
    fn read_log(&self, session_id: &str, query: &LogQuery) -> PortResult<LogPage>;
}

// ───────────────────────── 崩溃诊断（M15） ─────────────────────────

/// 六维修复动作（M15 `diagnose::rules::FixActionKind`，internally-tagged enum，
/// YAML 里写 `kind:`）。序列化名与 M15 逐字相同：`change_java|adjust_memory|
/// change_graphics|remove_mod|reinstall|seek_help`。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FixActionKind {
    /// 换 Java（大版本上下界/指定实现）。
    ChangeJava,
    /// 调 Xmx/Xms、64 位 Java、系统虚拟内存。
    AdjustMemory,
    /// 驱动/渲染器/分辨率/覆盖层。
    ChangeGraphics,
    /// 删/禁/换 mod。
    RemoveMod,
    /// 重装/修组件（java|game_jar|loader|natives|libraries|assets|mods…）。
    Reinstall,
    /// 导出诊断包 / 需要更多信息。
    SeekHelp,
}

/// 一条修复建议（M15 的 `MaterializedFixAction { kind, tool, args }` + 一句人话）。
///
/// `tool` 是 M15 给的**语义动作 id**（`java.set` / `memory.set` / …）。P0 没有这些细分工具，
/// 所以 agent 侧按 [`FixActionKind`] 桥接到现有工具（见 [`crate::tools::diagnose`]），
/// `tool` 只原样透传给模型与审计，不用于分发。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FixAction {
    /// 六维动作类别。
    pub kind: FixActionKind,
    /// 语义动作 id（如 `memory.set`）；P1 落对应工具后可直接当工具名。
    pub tool: String,
    /// 一句话建议（可为空，agent 会按 `kind` 补兜底文案）。
    pub label: String,
    /// 已做模板替换与单位换算的参数（M15 `materialize()` 的产物）。
    pub args: std::collections::BTreeMap<String, String>,
}

/// 一条诊断候选（M15 `DiagnosisRecord::candidates[]`）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DiagnosisCandidate {
    /// 规则 id（如 `jvm.32bit`）。
    pub id: String,
    /// `critical|error|warn|info`。
    pub severity: String,
    /// 分组（`jvm|memory|graphics|integrity|mod|…`）。
    pub category: String,
    /// 置信度 0.0~1.0。
    pub confidence: f32,
    /// i18n key（如 `diagnose.cause.jvm.32bit`）。
    pub message_key: String,
    /// **渲染后**的人类可读结论（M15 保证非空；agent 侧只在极旧数据上回落 `message_key`）。
    pub message: String,
    /// 更细的处理建议（M15 `detail`，可能为空）。
    pub detail: String,
    /// 证据行（已脱敏）。
    pub evidence: Vec<String>,
    /// 该候选的修复动作。
    pub fix_actions: Vec<FixAction>,
}

/// 退出摘要（M15 `DiagnosisRecord::exit`，与 launch 的 `exit` 事件同源）。
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct ExitSummary {
    /// 退出码。
    pub code: Option<i32>,
    /// `normal|application_error|jvm_error|sigkill|sigterm|killed|interrupted`。
    pub exit_type: Option<String>,
    /// exit 0 但疑似静默失败。
    pub silent_failure: bool,
}

/// 规则引擎结论：**这就是给 LLM 的全部输入**（不是裸日志）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CrashDiagnosis {
    /// 会话 id。
    pub session_id: String,
    /// `rules|ai`（谁产出的）。
    pub source: String,
    /// 规则拿不准、需要更多信息时置 true（此时才值得让 LLM 深挖）。
    pub needs_more_info: bool,
    /// 主因候选，`candidates[0]` 即首因（M15 已按 priority→confidence 排序）。
    pub candidates: Vec<DiagnosisCandidate>,
    /// 被抑制的次要候选。
    pub secondary: Vec<DiagnosisCandidate>,
    /// 会话级关键片段（`crash_found.excerpt` / `hs_err` 摘要，已脱敏）。
    pub evidence: Vec<String>,
    /// 退出摘要。
    pub exit: ExitSummary,
}

impl CrashDiagnosis {
    /// 首选候选（主因）。
    #[must_use]
    pub fn top(&self) -> Option<&DiagnosisCandidate> {
        self.candidates.first()
    }

    /// 最高置信度（没有候选时为 0）。
    #[must_use]
    pub fn top_confidence(&self) -> f32 {
        self.top().map_or(0.0, |c| c.confidence)
    }
}

/// 诊断端口（M15 实现）。agent 只做「调规则引擎 → 把结论 + 片段交给 LLM」。
pub trait DiagnosePort: Send + Sync {
    /// 对一个会话跑规则引擎。会话不存在/没有日志时返回人类可读错误。
    fn diagnose(&self, session_id: &str) -> PortResult<CrashDiagnosis>;
}
