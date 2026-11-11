//! 「浏览器优先、失败自动回退设备码」（M178）——GUI 那条可选加速路的编排。
//!
//! ## 为什么这条流必须自己会退
//!
//! 授权码（浏览器）流把用户送回 `http://localhost:{port}/auth`。随机端口只有在「回环 URI
//! 的端口不参与匹配」成立时才可能被接受——微软官方文档
//! （[*Redirect URI (reply URL) best practices and limitations*](https://learn.microsoft.com/en-us/entra/identity-platform/reply-url)，
//! *Localhost exceptions*）原文：
//!
//! > "Due to ephemeral port ranges often required by native applications, the port component
//! > (for example, :5001 or :443) is ignored for the purposes of matching a localhost redirect URI."
//!
//! 也就是说**注册一次 `http://localhost/auth` 就能接住任意端口**；但本项目这个注册当初是为
//! device-code 建的，重定向 URI 一项都没登记（`DevDocs/ops/azure-msa-app.md`）。没登记时
//! 授权端点当场回 `invalid_request ... 'redirect_uri' is not valid`，**而且不会**再跳回本地
//! 回调——用户只看到一张 AAD 原始错误页，启动器干等到超时。这正是 2026-11-08 官方产物上
//! 「微软登录整体不可用」的根因。
//!
//! 于是这里的分工是：设备码当默认（不依赖任何注册项），浏览器流只在**用户显式要求**时试，
//! 而且试之前先预检（[`super::preflight`]）：没登记就一个监听口都不开。
//!
//! 回退说明有两层：结构化码 [`BrowserFallbackReason`]（宿主据它出 i18n，**能说清怎么修**）
//! 与原始诊断 [`BrowserFallback::detail`]（只进日志，不上屏）。

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::auth::error::{AuthError, AuthResult};

use super::authcode::{self, AuthCodeAttempt, AuthCodePrompt};
use super::chain;
use super::flow::{self, DeviceCodePrompt};
use super::now_ms;
use super::preflight;
use super::session::MicrosoftSession;
use super::MicrosoftAuth;

/// 「浏览器优先但没走成」的原因（**上屏只看码**，`detail` 只进日志）。
///
/// 三个码对应三种不同的下一步：
/// - [`BrowserFallbackReason::RedirectNotRegistered`]：应用注册里没登记回环重定向 URI。
///   修法是**具体可执行的**（把 `http://localhost/auth` 加进「重定向 URI」，端口不参与匹配）。
/// - [`BrowserFallbackReason::PreflightUnavailable`]：没法确认（网络/超时），只能先用设备码。
/// - [`BrowserFallbackReason::AttemptFailed`]：试过了但失败（超时、换 token 被拒…）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserFallbackReason {
    /// 授权端点当场拒绝了这个回环 `redirect_uri`（`invalid_request` / `AADSTS50011`）
    RedirectNotRegistered,
    /// 预检本身没问出结论（网络/超时/非预期应答）
    PreflightUnavailable,
    /// 预检通过、但授权码流仍然没走通
    AttemptFailed,
}

impl BrowserFallbackReason {
    /// 稳定的 ascii 码（日志 / 页面「Code: …」/ 测试断言都用它）。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::RedirectNotRegistered => "redirect_not_registered",
            Self::PreflightUnavailable => "preflight_unavailable",
            Self::AttemptFailed => "attempt_failed",
        }
    }
}

/// 回退到设备码时的说明：原因码 + 原始诊断（诊断只进日志，不上屏）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserFallback {
    /// 原因码
    pub reason: BrowserFallbackReason,
    /// 原始诊断（**只进日志**；截断到 240 字符）
    pub detail: String,
}

impl BrowserFallback {
    /// 构造（诊断串截断：只进日志，别把整张错误页灌进去）。
    #[must_use]
    pub fn new(reason: BrowserFallbackReason, detail: impl Into<String>) -> Self {
        let detail = detail.into();
        let detail = match detail.char_indices().nth(240) {
            Some((index, _)) => format!("{}…", &detail[..index]),
            None => detail,
        };
        Self { reason, detail }
    }
}

/// 一次「浏览器优先」登录的完整结果（M178）。
///
/// `listeners_opened` 是**这次登录**真的去开本地监听口的次数（0 或 1，`bind_counting` 在
/// `TcpListener::bind` 现场 +1）。它存在的唯一理由是守卫能直接数：「没登记回环」「走设备码」
/// 这两条路上必须是 0。用进程级全局计数器做不到这一点（并行跑的测试会互相搅动），所以计数
/// 是 per-login 的 `AtomicUsize`，但**增长点在绑定现场**，不在「进没进某个分支」。
#[derive(Debug, Clone)]
pub struct LoginOutcome {
    /// 拿到的会话（两条流任意一条走通）
    pub session: MicrosoftSession,
    /// 回退说明（`None` = 没走浏览器 / 浏览器走通了）
    pub browser: Option<BrowserFallback>,
    /// 这次登录一共开了几个本地监听口（设备码路恒 0）
    pub listeners_opened: usize,
}

impl LoginOutcome {
    /// 浏览器（授权码）流是不是被跳过了。
    #[must_use]
    pub fn browser_was_skipped(&self) -> bool {
        self.browser.is_some()
    }
}

impl MicrosoftAuth {
    /// 浏览器优先、失败自动回退设备码（M178：GUI 的「用浏览器登录」入口）。
    ///
    /// 顺序与判据：
    /// 1. 先预检回环注册（[`authcode::login_checked`]）。没登记 / 问不出来 → **不绑端口**、
    ///    不弹浏览器，带一条结构化说明直接进设备码；
    /// 2. 预检通过 → 真走授权码流（绑端口 + 弹浏览器）；
    /// 3. 授权码流失败（超时 / 换 token 被拒 / 端口绑不上）→ 同样回退设备码，
    ///    说明码是 [`BrowserFallbackReason::AttemptFailed`]（拒绝原因若属于「重定向 URI 没登记」
    ///    则归一成 [`BrowserFallbackReason::RedirectNotRegistered`]，好让用户看到怎么修）；
    /// 4. **用户自己取消不算失败**（[`AuthError::Canceled`] 原样上抛）：那是明确意图，
    ///    不能偷偷换一条流继续问用户要码。
    ///
    /// `present_device` 额外收到回退说明（`None` = 根本没走浏览器），宿主据此在设备码面板上
    /// 画一句「为什么改用设备码 + 怎么修」。
    ///
    /// # Errors
    ///
    /// 用户取消、设备码流本身的失败（网络/超时/被拒/无拥有权/无档案），见 [`AuthError`]。
    pub async fn browser_then_device_login<FB, FD>(
        &self,
        present_browser: FB,
        mut present_device: FD,
    ) -> AuthResult<LoginOutcome>
    where
        FB: FnOnce(&AuthCodePrompt),
        FD: FnMut(&DeviceCodePrompt, Option<&BrowserFallback>),
    {
        let mut fallback: Option<BrowserFallback> = None;
        // 绑定现场的**真计数**（`CallbackServer::bind_counting` 每次调 `TcpListener::bind` 前 +1）：
        // 不是「进没进授权码分支」的代理指标——那正是复审 r1 P2-1 变异 D 打穿的地方。
        let listeners = AtomicUsize::new(0);
        if self.browser_first {
            match authcode::login_checked(
                &self.client,
                &self.endpoints,
                &self.client_id,
                self.authcode.clone(),
                &listeners,
                present_browser,
            )
            .await
            {
                Ok(AuthCodeAttempt::LoggedIn(session)) => {
                    return Ok(LoginOutcome {
                        session,
                        browser: None,
                        listeners_opened: listeners.load(Ordering::Relaxed),
                    })
                }
                // 跳过：**没绑端口**（预检在绑定之前），计数仍是 0。
                Ok(AuthCodeAttempt::Skipped(note)) => fallback = Some(note),
                Err(AuthError::Canceled) => return Err(AuthError::Canceled),
                Err(error) => {
                    self.record_failure("authcode_login", &error);
                    let reason = if preflight::error_means_unregistered(&error) {
                        BrowserFallbackReason::RedirectNotRegistered
                    } else {
                        BrowserFallbackReason::AttemptFailed
                    };
                    fallback = Some(BrowserFallback::new(reason, error.to_string()));
                }
            }
        }
        let grant =
            flow::request_device_code(&self.client, &self.endpoints, &self.client_id).await?;
        present_device(&grant.prompt, fallback.as_ref());
        let tokens = flow::poll_device_token(
            &self.client,
            &self.endpoints,
            &self.client_id,
            &grant,
            self.poll,
        )
        .await?;
        let session =
            chain::complete_chain(&self.client, &self.endpoints, tokens, now_ms()).await?;
        Ok(LoginOutcome {
            session,
            browser: fallback,
            listeners_opened: listeners.load(Ordering::Relaxed),
        })
    }
}
