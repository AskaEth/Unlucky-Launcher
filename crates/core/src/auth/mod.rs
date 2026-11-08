//! 账户体系：三轨（离线 UUID / 微软 device-code 与授权码 / authlib-injector）、凭据存储边界。
//!
//! 设计定稿：`DevDocs/PlanDoc/07-accounts.md`（决策 D9/Q21），协议细节来自
//! `DevDocs/analyse/04-authentication.md`（HMCL v3.17 拆解，逐报文对照）。
//!
//! # 三条铁律
//!
//! 1. **凭据永不外泄**：token/密码不落日志、不进事件流、不打印（`AGENTS.md` §6、D4）。
//!    类型层保证：[`Secret`] 的 `Debug`/`Display` 恒为 `<redacted>`，取明文只能 `.expose()`；
//!    对外展示一律走元数据（[`AccountMeta`]，不含凭据）。
//! 2. **账户操作永不 YOLO**：本模块不向 AI 暴露任何账户读写面（07 §3）；
//!    CLI 也只开 `list / add offline / add msa / remove` 四条（authlib 密码登录走 GUI 输入框，
//!    避免密码进 argv）。
//! 3. **元数据 / 凭据分文件**：`data/auth/accounts.json` + `data/auth/credentials.dat`，
//!    先写凭据后写元数据，原子写，单条坏记录只跳过不整体失败。
//!
//! # 公共面
//!
//! - [`account`]：`AccountId` / `AccountKind` / `AccountMeta` / `Credentials`（三轨统一抽象）；
//! - [`store`]：`AccountStore`（加载/增删/落盘，`data/auth/`）；
//! - [`offline`]：HMCL 兼容离线 UUID + 启动占位；
//! - [`microsoft`]：微软两条登录流（device-code / 授权码+本地回调）+ 共用后链
//!   （XBL→XSTS→login_with_xbox→entitlements→profile）+ 刷新判定；
//! - [`authlib`]：标准 Yggdrasil 协议（authenticate/refresh/validate/invalidate，账户侧）；
//! - [`error`]：错误分流（取消 / 超时 / 被拒 / 凭据过期）；
//! - [`body`]：非 2xx 应答的正文诊断（分类 JSON/HTML/空体 + 脱敏摘要，M38）；
//! - [`journal`]：认证失败事件落盘（`.ULL/logs/auth.jsonl`，M38）；
//! - [`secret`]：凭据值包装与脱敏。
//!
//! # 本期不做（后续 mission）
//!
//! OS keychain / AES-256-GCM 凭据加密（07 §2 分级 1–2）、authlib-injector 的
//! `-javaagent` 三件套与 jar 下载、皮肤与多角色选择、离线 UUID 的「官方风格」双模式
//! （07 §1 升级 2，口径待定案）。接口约定见 `DevDocs/SubAgentsTalk/auth-接口约定.md`。

pub mod account;
pub mod authlib;
pub mod body;
pub mod error;
pub mod http;
pub mod journal;
pub mod microsoft;
pub mod offline;
pub mod secret;
pub mod store;

pub use account::{AccountId, AccountKind, AccountMeta, Credentials};
pub use body::{BodyKind, ResponseDetail};
pub use error::{AuthError, AuthResult};
pub use journal::{AuthFailureEvent, AuthJournal};
pub use microsoft::{
    AuthCodePolicy, AuthCodePrompt, MicrosoftAuth, MicrosoftSession, RefreshDecision, RefreshReason,
};
pub use secret::Secret;
pub use store::{AccountStore, AuthPaths};
