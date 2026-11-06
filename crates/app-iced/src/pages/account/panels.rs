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

use unlucky_presenter::dto::{AuthlibLoginView, DeviceLoginPhase, DeviceLoginView, LoginFailure};

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
        /// 剩余秒数
        remaining: Option<u64>,
        /// 是否已过期
        expired: bool,
    },
    /// 授权码流：等浏览器回调
    Browser {
        /// 授权链接（可复制）
        link: String,
        /// 剩余秒数
        remaining: Option<u64>,
        /// 是否已过期
        expired: bool,
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
pub(super) fn device_panel(view: &DeviceLoginView, remaining: Option<u64>) -> DevicePanel {
    let expired = remaining == Some(0);
    match view.phase {
        DeviceLoginPhase::Idle => DevicePanel::Hidden,
        DeviceLoginPhase::Requesting => DevicePanel::Requesting,
        DeviceLoginPhase::Waiting => {
            if view.user_code.trim().is_empty() {
                DevicePanel::Browser {
                    link: view.verification_uri.clone(),
                    remaining,
                    expired,
                }
            } else {
                DevicePanel::Code {
                    code: view.user_code.clone(),
                    link: view.qr_link.clone(),
                    remaining,
                    expired,
                }
            }
        }
        DeviceLoginPhase::Failed => DevicePanel::Failed {
            failure: view.failure.unwrap_or(LoginFailure::Other),
        },
        DeviceLoginPhase::Succeeded => DevicePanel::Succeeded {
            name: view.account_name.clone(),
        },
    }
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
