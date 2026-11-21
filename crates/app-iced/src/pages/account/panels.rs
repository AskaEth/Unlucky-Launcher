//! 两条登录流的面板态（微软设备码/授权码 + 第三方站校验/登录）。
//!
//! 从 `model.rs` 拆出来守 400 行硬线：这些类型是「UI 要画什么」的枚举，
//! 判定逻辑（倒计时 → 已过期、服务发现 → 可用）也在这里，页面与表单只消费结果。
//!
//! ## 为什么这里**没有** `message` / `detail` 字段（M114 r3 / 复审 r2 P2-2）
//!
//! 后端的原始诊断串是中文人话，只该进日志。r2 时它们还以 `message`/`detail` 存在于此，
//! 「页面把它们画上屏」只能靠一条**平行实现**的取样断言去拦（复审用变异证明那条断言是哑的）。
//! 现在直接把字段删掉：视图**没有材料可画**，想画原文是编译错误——比任何测试都硬。
//! 原文仍在 presenter 的 `DeviceLoginView/AuthlibLoginView::message` 里（只进 `eprintln!`）。

use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::{
    AuthlibLoginView, BrowserLoginNote, DeviceLoginPhase, DeviceLoginView, LoginFailure,
};

/// 微软登录面板要画什么（每态都不同，**不允许只有一个转圈**）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DevicePanel {
    /// 没有登录在跑
    Hidden,
    /// 正在申请设备码（进度文案由页面按 i18n 出）
    Requesting,
    /// 设备码等待用户操作
    Code {
        /// 用户码
        code: String,
        /// 验证链接
        link: String,
        /// 倒计时那行**已经渲染好的文案**（`15:00 left` / `剩 15:00`；过期时是「已过期」）
        ///
        /// 为什么文案在模型层拼（M178）：`frame::text_role_owned` 那条 owned 文字路径
        /// **画不出中文**（ASCII 正常），倒计时在中文界面会整行消失。模型持有字符串、
        /// 视图用 `&'a str` 借用，两边都不依赖那条路径。
        countdown: String,
        /// 是否已过期（决定颜色与「重试」按钮）
        expired: bool,
        /// 浏览器登录没走成、已回退到设备码的说明（M178；`None` = 没这回事）
        note: Option<BrowserLoginNote>,
    },
    /// 授权码流：等浏览器回调
    Browser {
        /// 授权链接（可复制）
        link: String,
        /// 倒计时那行已渲染好的文案（同上）
        countdown: String,
        /// 是否已过期
        expired: bool,
    },
    /// 失败（**只有原因码**；原文在 presenter 的日志里）
    Failed {
        /// 原因码（页面据此选 i18n）
        failure: LoginFailure,
        /// 回退说明（浏览器登录没走成之后设备码也失败了；M178）
        note: Option<BrowserLoginNote>,
    },
    /// 成功
    Succeeded {
        /// 账户名
        name: String,
    },
}

/// 第三方轨面板的可见状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthlibPanel {
    /// 没在跑
    Idle,
    /// 正在校验/登录
    Running,
    /// 地址校验通过（服务发现拿到服务器名）
    Ready {
        /// 服务器名
        server_name: String,
    },
    /// 失败（**只有原因码**；原文在 presenter 的日志里）
    Failed {
        /// 原因码（页面据此选 i18n）
        failure: LoginFailure,
    },
    /// 成功
    Succeeded {
        /// 账户名
        name: String,
    },
}

/// 微软登录状态 → 面板态（倒计时与「已过期」都在这里定性，UI 只画）。
pub(super) fn device_panel(
    view: &DeviceLoginView,
    remaining: Option<u64>,
    catalog: Catalog,
) -> DevicePanel {
    let expired = remaining == Some(0);
    let countdown = countdown_text(remaining, expired, catalog);
    match view.phase {
        DeviceLoginPhase::Idle => DevicePanel::Hidden,
        DeviceLoginPhase::Requesting => DevicePanel::Requesting,
        DeviceLoginPhase::Waiting => {
            if view.user_code.trim().is_empty() {
                DevicePanel::Browser {
                    link: view.verification_uri.clone(),
                    countdown,
                    expired,
                }
            } else {
                DevicePanel::Code {
                    code: view.user_code.clone(),
                    link: view.qr_link.clone(),
                    countdown,
                    expired,
                    note: view.browser_note,
                }
            }
        }
        DeviceLoginPhase::Failed => DevicePanel::Failed {
            failure: view.failure.unwrap_or(LoginFailure::Other),
            note: view.browser_note,
        },
        DeviceLoginPhase::Succeeded => DevicePanel::Succeeded {
            name: view.account_name.clone(),
        },
    }
}

/// 倒计时那一行的文案（`mm:ss` + i18n；过期/不知道有效期的两种情况都在这里收口）。
///
/// `mm:ss` 而不是 `900s`：人读的是分秒；数字与冒号在两种语言里同形，只有 `left`/`剩` 走 catalog。
fn countdown_text(remaining: Option<u64>, expired: bool, catalog: Catalog) -> String {
    if expired {
        return catalog.text(Key::AccountDeviceExpired).to_string();
    }
    remaining.map_or_else(String::new, |secs| {
        let clock = format!("{:02}:{:02}", secs / 60, secs % 60);
        catalog.render(Key::AccountDeviceCountdown, &[("time", &clock)])
    })
}

/// 第三方轨状态 → 面板态（顺带取出「校验通过的服务器名」）。
pub(super) fn authlib_panel(view: &AuthlibLoginView) -> (AuthlibPanel, String) {
    match view.phase {
        DeviceLoginPhase::Idle => (AuthlibPanel::Idle, String::new()),
        DeviceLoginPhase::Requesting => (AuthlibPanel::Running, String::new()),
        DeviceLoginPhase::Waiting => (
            AuthlibPanel::Ready {
                server_name: view.server_name.clone(),
            },
            view.server_name.clone(),
        ),
        DeviceLoginPhase::Succeeded => (
            AuthlibPanel::Succeeded {
                name: view.account_name.clone(),
            },
            view.server_name.clone(),
        ),
        DeviceLoginPhase::Failed => (
            AuthlibPanel::Failed {
                failure: view.failure.unwrap_or(LoginFailure::Other),
            },
            String::new(),
        ),
    }
}

impl AuthlibPanel {
    /// 是否正在跑（此时表单字段禁用，避免改了一半的地址被提交）。
    #[must_use]
    pub fn busy(&self) -> bool {
        matches!(self, Self::Running { .. })
    }
}
