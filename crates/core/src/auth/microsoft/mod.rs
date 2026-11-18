//! 微软轨：device-code 与授权码（浏览器）两条登录流 + 会话/刷新语义
//! （`PlanDoc/07-accounts.md` §1、`analyse/04` §4–§5、`research/08` §3）。
//!
//! - [`client_id`]：Azure 公共客户端 id 的**配置状态**（来源 / 配没配 / 启动日志那一行，零网络）；
//! - [`session`]：`MicrosoftSession`（凭据载荷）与**刷新判定**（纯函数，可脱离网络单测）；
//! - [`endpoints`] + [`flow`]：端点表与 device-code 发起 / 轮询 / refresh（OAuth 层）；
//! - [`authcode`] + [`callback`] + [`preflight`] + [`login`]：授权码流（授权 URL + 本地回调 +
//!   回环注册预检 + 回退设备码的编排）；
//! - [`chain`]：XBL → XSTS → login_with_xbox → entitlements → profile（Xbox/MC 层）。
//!
//! 两条流只差「怎么拿到 MS access_token」：拿到之后都进同一个 [`chain::complete_chain`]，
//! 落盘/刷新也共用 [`MicrosoftSession`]，所以 GUI 账户页的两颗按钮不会长出两套凭据语义。
//! # 默认走哪条（M178）
//!
//! **设备码是默认**，授权码是可选加速路：授权码流要求应用注册里登记回环重定向 URI
//! （`http://localhost/auth`，端口由微软忽略），而本项目的注册当初只为 device-code 而建。
//! 没登记时授权端点当场拒掉且**不**把浏览器送回本地回调，用户只看到
//! `invalid_request ... 'redirect_uri' is not valid` 那张 AAD 原始错误页。所以：
//!
//! | 入口 | 走什么 |
//! |---|---|
//! | [`MicrosoftAuth::device_code_login`] | 设备码，**一个监听口都不开** |
//! | [`MicrosoftAuth::browser_then_device_login`] | 先预检回环注册：没登记就直接设备码；登记了走授权码，失败回退设备码 |
//! | [`MicrosoftAuth::authcode_login`] | 强制授权码（测试/排障用，不做预检） |
//!
//! # client id 从哪来
//!
//! 两条流都必须带一个**已在 Azure 注册的公共客户端 id**（租户必须是 `consumers`）。
//! 授权码流额外要求注册里登记过回环重定向 URI（`http://localhost/auth`，端口不参与匹配；
//! 见 [`login`] 引的微软官方原文）。id 与 HMCL 同款模型：二进制里不硬编码，
//! 改由 `ULL_MSA_CLIENT_ID`（运行期）或构建期 `option_env!` 提供。没有配时
//! [`MicrosoftAuth::from_env`] 返回 [`AuthError::Unsupported`]，文案是
//! [`client_id::MISSING_CLIENT_ID_MESSAGE`]（「未配置」+ 怎么配，见 [`client_id`]）——
//! 这比编一个假 id 让用户看到 `invalid_client` 诚实。
//!
//! 措辞纪律（M117）：id 缺失是**没配置**不是「本期未支持」，判定与启动日志都在 [`client_id`]。

pub mod authcode;
pub mod callback;
pub mod chain;
pub mod client_id;
pub mod endpoints;
pub mod flow;
pub mod login;
pub mod preflight;

pub mod session;

pub use authcode::{AuthCodePolicy, AuthCodePrompt, RedirectUri};
pub use callback::CallbackServer;
// 回环重定向 URI 的注册预检（M178）：`RedirectRegistration` 是宿主排障时唯一要看的结论。
pub use preflight::{RedirectRegistration, PROBE_PORT};
// client id 的判定与启动日志行留一层 re-export：调用方（CLI / GUI / 启动日志）只认
// `auth::microsoft::*` 这一条路径，不必知道它是单独一块（M117）。
pub use client_id::{
    client_id_status, resolve_client_id, ClientIdSource, ClientIdStatus, CLIENT_ID_BUILD,
    CLIENT_ID_ENV, MISSING_CLIENT_ID_MESSAGE,
};
pub use endpoints::MicrosoftEndpoints;
pub use flow::{DeviceCodePrompt, PollPolicy};
pub use login::{BrowserFallback, BrowserFallbackReason, LoginOutcome};
pub use session::{MicrosoftSession, RefreshDecision, RefreshReason, USER_TYPE_MSA};

use std::time::{SystemTime, UNIX_EPOCH};

use reqwest::Client;

use crate::auth::error::{AuthError, AuthResult};
use crate::auth::journal::{AuthFailureEvent, AuthJournal};
use crate::download::config::EngineConfig;
use crate::download::http::build_client;

/// 微软账户登录/刷新。
#[derive(Debug, Clone)]
pub struct MicrosoftAuth {
    client: Client,
    client_id: String,
    endpoints: MicrosoftEndpoints,
    poll: PollPolicy,
    authcode: AuthCodePolicy,
    browser_first: bool,
    journal: Option<AuthJournal>,
}

impl MicrosoftAuth {
    /// 按显式 client id 构造（HTTP 基座复用 `core::download` 的客户端构造）。
    ///
    /// # Errors
    ///
    /// HTTP 客户端建不起来时返回 [`AuthError::Network`]。
    pub fn new(client_id: impl Into<String>) -> AuthResult<Self> {
        let client = build_client(&EngineConfig::default())
            .map_err(|error| AuthError::network("http-client", error.to_string()))?;
        Ok(Self {
            client,
            client_id: client_id.into(),
            endpoints: MicrosoftEndpoints::consumers(),
            poll: PollPolicy::default(),
            authcode: AuthCodePolicy::default(),
            browser_first: false,
            journal: None,
        })
    }

    /// 从环境/构建期取 client id（CLI 与 GUI 的默认入口）。
    ///
    /// 与 [`Self::new`] 的唯一差别：这条**生产入口**默认挂上便携数据根的失败日志
    /// （`.ULL/logs/auth.jsonl`，见 [`AuthJournal::portable_default`]），
    /// 交互式命令失败时 403 的状态码/正文摘要不会再随终端滚走。
    ///
    /// # Errors
    ///
    /// 两处都没配时返回 [`AuthError::Unsupported`]，文案是
    /// [`MISSING_CLIENT_ID_MESSAGE`]（「未配置」+ 怎么配）。这条路径**零网络**：
    /// id 判定在网络之前，判断失败就地早退，连 HTTP 客户端都不建。
    pub fn from_env() -> AuthResult<Self> {
        let Some((client_id, _source)) = resolve_client_id() else {
            return Err(AuthError::Unsupported(
                MISSING_CLIENT_ID_MESSAGE.to_string(),
            ));
        };
        let mut auth = Self::new(client_id)?;
        auth.journal = AuthJournal::portable_default();
        Ok(auth)
    }

    /// 换失败日志落点（`None` = 不落盘；CI/单测用）。
    #[must_use]
    pub fn with_journal(mut self, journal: Option<AuthJournal>) -> Self {
        self.journal = journal;
        self
    }

    /// 关掉失败日志。
    #[must_use]
    pub fn without_journal(mut self) -> Self {
        self.journal = None;
        self
    }

    /// 失败日志落点（测试断言/宿主展示用）。
    #[must_use]
    pub fn journal(&self) -> Option<&AuthJournal> {
        self.journal.as_ref()
    }

    /// 换 HTTP 客户端（测试注入 / 复用宿主连接池）。
    #[must_use]
    pub fn with_client(mut self, client: Client) -> Self {
        self.client = client;
        self
    }

    /// 换端点表（测试整表指向 mock 服务器）。
    #[must_use]
    pub fn with_endpoints(mut self, endpoints: MicrosoftEndpoints) -> Self {
        self.endpoints = endpoints;
        self
    }

    /// 换轮询策略（测试用小间隔）。
    #[must_use]
    pub fn with_poll_policy(mut self, poll: PollPolicy) -> Self {
        self.poll = poll;
        self
    }

    /// 换授权码流参数（回调超时 / 固定端口 / 绑定重试 / 固定 state；测试与 CLI 用）。
    #[must_use]
    pub fn with_authcode_policy(mut self, policy: AuthCodePolicy) -> Self {
        self.authcode = policy;
        self
    }

    /// 「浏览器优先」（M178）：授权码流只是**可选加速路**，默认不走。
    ///
    /// 只有宿主确认用户显式要求时才置位（GUI 的「用浏览器登录」按钮）。置位后仍然先做
    /// 回环注册预检（[`preflight`]）：没登记就**不绑端口、不弹浏览器**，直接设备码。
    #[must_use]
    pub fn with_browser_first(mut self, enabled: bool) -> Self {
        self.browser_first = enabled;
        self
    }

    /// 是否允许尝试浏览器（授权码）流。
    #[must_use]
    pub fn browser_first(&self) -> bool {
        self.browser_first
    }

    /// 当前端点表。
    #[must_use]
    pub fn endpoints(&self) -> &MicrosoftEndpoints {
        &self.endpoints
    }

    /// device-code 登录全流程。
    ///
    /// `present` 在拿到用户码后**立刻**被调用（CLI 打到 stderr、GUI 弹框/二维码），
    /// 然后才开始轮询——用户有整整 `expires_in` 的时间去另一台设备上输码。
    ///
    /// 失败时除返回错误外，还会往失败日志（若配了）追加一条结构化记录。
    ///
    /// # Errors
    ///
    /// 网络/超时/取消/被拒/无拥有权/无档案，见 [`AuthError`]。
    pub async fn device_code_login<F>(&self, mut present: F) -> AuthResult<MicrosoftSession>
    where
        F: FnMut(&DeviceCodePrompt),
    {
        let result = self.login_and_chain(&mut present).await;
        if let Err(error) = &result {
            self.record_failure("device_code_login", error);
        }
        result
    }

    /// 登录主体（`device_code_login` 的失败日志由外层统一记）。
    async fn login_and_chain(
        &self,
        present: &mut impl FnMut(&DeviceCodePrompt),
    ) -> AuthResult<MicrosoftSession> {
        let grant =
            flow::request_device_code(&self.client, &self.endpoints, &self.client_id).await?;
        present(&grant.prompt);
        let tokens = flow::poll_device_token(
            &self.client,
            &self.endpoints,
            &self.client_id,
            &grant,
            self.poll,
        )
        .await?;
        chain::complete_chain(&self.client, &self.endpoints, tokens, now_ms()).await
    }

    /// 授权码流登录（浏览器 + 本地回调，`research/08` §3 方案 A，GUI 的默认入口）。
    ///
    /// `present` 在端口绑定之后、等待回调之前被调用：宿主在这一刻拉起系统默认浏览器
    /// （core 不做 UI，也不假设有 GUI——CLI 可以只把 URL 打到 stderr）。
    /// 失败时与 [`Self::device_code_login`] 一样往失败日志追加一条结构化记录（M38）。
    ///
    /// # Errors
    ///
    /// 端口绑不上/网络失败、等回调超时、用户在授权页取消、OAuth 拒绝，以及整链的
    /// [`AuthError`]（含白名单未批时 `login_with_xbox` 的 403 诊断）。
    pub async fn authcode_login<F>(&self, present: F) -> AuthResult<MicrosoftSession>
    where
        F: FnOnce(&AuthCodePrompt),
    {
        let result = authcode::login(
            &self.client,
            &self.endpoints,
            &self.client_id,
            self.authcode.clone(),
            present,
        )
        .await;
        if let Err(error) = &result {
            self.record_failure("authcode_login", error);
        }
        result
    }

    /// 刷新：Live refresh → 整链重跑（**原样覆盖 refresh_token**）。
    ///
    /// # Errors
    ///
    /// 凭据被撤销时返回 [`AuthError::CredentialExpired`]（唯一该重新交互的信号）。
    pub async fn refresh(&self, session: &MicrosoftSession) -> AuthResult<MicrosoftSession> {
        let result = self.refresh_and_chain(session).await;
        if let Err(error) = &result {
            self.record_failure("refresh", error);
        }
        result
    }

    /// 刷新主体（失败日志由外层统一记）。
    async fn refresh_and_chain(&self, session: &MicrosoftSession) -> AuthResult<MicrosoftSession> {
        let tokens = flow::refresh_tokens(
            &self.client,
            &self.endpoints,
            &self.client_id,
            &session.refresh_token,
        )
        .await?;
        let refreshed =
            chain::complete_chain(&self.client, &self.endpoints, tokens, now_ms()).await?;
        if !session.same_profile(&refreshed) {
            // 防「刷出来是另一个账号」（承 HMCL WrongAccountException）。
            return Err(AuthError::Malformed(format!(
                "刷新后档案变了（{} → {}）",
                session.profile_id, refreshed.profile_id
            )));
        }
        Ok(refreshed)
    }

    /// 尽力而为地记一条失败事件（写日志失败绝不影响登录本身的结论）。
    fn record_failure(&self, scope: &str, error: &AuthError) {
        if let Some(journal) = &self.journal {
            let _ = journal.record(&AuthFailureEvent::of(scope, error));
        }
    }

    /// 网络侧校验：`GET /minecraft/profile` 是否还认这个 token。
    ///
    /// 404 的二义在这里**不吞**：档案被删是真实错误，不该被当成「token 失效」静默刷新
    /// （`analyse/04` §11 观察项 3 那个隐式约定，我们写明）。
    ///
    /// # Errors
    ///
    /// 网络失败、无拥有权、无档案等。
    pub async fn validate(&self, session: &MicrosoftSession) -> AuthResult<bool> {
        if session.is_expired(now_ms()) {
            return Ok(false);
        }
        match chain::fetch_profile(&self.client, &self.endpoints, &session.access_token).await {
            Ok(_) => Ok(true),
            // 「档案被删 / 没买游戏」是真实错误，穿透上抛（别静默当成 token 失效去刷新）。
            Err(error @ AuthError::Denied { .. }) => {
                let fatal = matches!(
                    &error,
                    AuthError::Denied { code, .. } if code == "no_profile" || code == "no_license"
                );
                if fatal {
                    Err(error)
                } else {
                    Ok(false)
                }
            }
            // 链上非 2xx：拦截页（HTML/纯文本）说明网络有问题，别伪装成「token 失效」；
            // 服务端的 JSON 拒绝 / 空体仍按「token 不被接受」处理（与 M31 语义一致）。
            Err(error @ AuthError::Response(_)) => {
                let interception = matches!(
                    &error,
                    AuthError::Response(detail) if detail.is_interception()
                );
                if interception {
                    Err(error)
                } else {
                    Ok(false)
                }
            }
            Err(other) => Err(other),
        }
    }

    /// 需要就刷新（本地判定，不打网络），返回 `(会话, 是否刷新过)`。
    ///
    /// 这是 GUI/CLI 的「静默续期」入口：`Reuse` 时零网络，`Refresh` 时整链重跑。
    ///
    /// # Errors
    ///
    /// 刷新失败时原样上抛（含 [`AuthError::CredentialExpired`]）。
    pub async fn ensure_fresh(
        &self,
        session: &MicrosoftSession,
    ) -> AuthResult<(MicrosoftSession, Option<RefreshReason>)> {
        match session.refresh_decision(now_ms()) {
            RefreshDecision::Reuse => Ok((session.clone(), None)),
            RefreshDecision::Refresh(reason) => {
                let refreshed = self.refresh(session).await?;
                Ok((refreshed, Some(reason)))
            }
        }
    }
}

/// 当前 Unix 毫秒。
#[must_use]
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{now_ms, MicrosoftAuth};

    #[test]
    fn explicit_client_id_builds() {
        let auth = MicrosoftAuth::new("0000-test-client").expect("构造");
        assert!(auth
            .endpoints()
            .profile
            .contains("api.minecraftservices.com"));
        // 默认**不是**浏览器优先（M178：设备码才是默认路）。
        assert!(!auth.browser_first());
    }

    #[test]
    fn now_ms_is_sane() {
        // 2020-01-01 之后、2100 之前（防止本地时钟/单位抄错）。
        let now = now_ms();
        assert!(now > 1_577_836_800_000 && now < 4_102_444_800_000);
    }
}
