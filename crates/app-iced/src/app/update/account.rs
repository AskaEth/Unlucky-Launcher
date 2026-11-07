//! 账户页消息的落点（M114）：页面只发意图，这里翻成 `AppCommand` 或本地的表单状态写入。
//!
//! 三条纪律（与 `update.rs` 同源）：
//!
//! 1. **不做乐观回显**：切账户/删账户/登录都只派命令，等 `AccountsChanged` /
//!    `DeviceLoginChanged` / `AuthlibLoginChanged` 回来才改列表；
//! 2. **密码只走一次**：`Submit` 把 `AuthlibRequest` 交给后端后立刻清掉表单里的密码
//!    （成功失败都清——重试用新输入，不在内存里多留一份凭据）；
//! 3. **表单态是纯 UI 状态**：输入框改动只写 `App::account_form` + 置脏，不派命令。
//!
//! ## 返回 `Task`（M114 r2 / 复审 P2-1）
//!
//! 复制用户码/链接必须把 `iced::clipboard::write` 的 `Task` **交回 runtime**——iced 0.14 的
//! `Task` 是惰性 stream，丢掉返回值等于永不执行（日志页的 Copy summary 就是正确写法）。

use iced::Task;
use unlucky_presenter::dto::{AccountKind, AppCommand, AuthlibRequest, DeviceLoginPhase};

use crate::app::Message;
use crate::pages::account::{Hint, Message as Account, Panel, Track};

use super::super::state::App;

impl App {
    /// 账户页消息。
    pub(super) fn on_account(&mut self, message: Account) -> Task<Message> {
        // 复制是唯一需要**交回 Task** 的分支：先把它的 Task 抓出来，其余分支照旧改状态
        let clipboard = match &message {
            Account::Copy(text) => Some(text.clone()),
            _ => None,
        };

        match message {
            Account::Open(track) => {
                self.account_form.open(track);
                self.account_form.hint = None;
                self.view_dirty = true;
            }
            Account::ToggleAdd => {
                self.account_form.section_open = !self.account_form.section_open;
                self.account_form.hint = None;
                self.view_dirty = true;
            }
            Account::Close => {
                self.account_form.close();
                // 关面板必须把**两条轨**的登录态都收回 Idle（复审 r2 P2-1：只看 `busy()` 时，
                // `Failed`/`Succeeded` 不在 busy ⇒ 状态仍是非 Idle ⇒ `Page::build` 又把面板
                // 强制回 Microsoft，按钮成了死按钮）。`CancelDeviceLogin` 在 host 侧本来就
                // 同时收两条轨并发 Idle，所以判据用「非 Idle」而不是「busy」。
                if self.state.device_login.phase != DeviceLoginPhase::Idle
                    || self.state.authlib_login.phase != DeviceLoginPhase::Idle
                {
                    self.dispatch(AppCommand::CancelDeviceLogin);
                }
                self.view_dirty = true;
            }
            Account::DismissNotice => self.dispatch(AppCommand::DismissAccountNotice),
            Account::Server(value) => {
                self.account_form.server = value;
                self.view_dirty = true;
            }
            Account::Login(value) => {
                self.account_form.login = value;
                self.view_dirty = true;
            }
            Account::Password(value) => {
                self.account_form.password = value;
                self.view_dirty = true;
            }
            Account::OfflineName(value) => {
                self.account_form.offline_name = value;
                self.view_dirty = true;
            }
            Account::Probe => {
                let server = self.account_form.server.clone();
                if !server.trim().is_empty() {
                    self.dispatch(AppCommand::ProbeAuthlibServer(server));
                }
            }
            Account::Submit => match self.account_form.panel {
                Panel::Offline => {
                    // 名字**不清空**：失败时用户还要看见自己输入的（复审 r1 P2-2 的另一半）
                    let name = self.account_form.offline_name.trim().to_string();
                    if !name.is_empty() {
                        self.dispatch(AppCommand::AddOfflineAccount(name));
                    }
                }
                Panel::Authlib => {
                    if self.account_form.authlib_ready() {
                        let request = AuthlibRequest::new(
                            self.account_form.server.clone(),
                            self.account_form.login.clone(),
                            std::mem::take(&mut self.account_form.password),
                        );
                        self.dispatch(AppCommand::AddAuthlibAccount(request));
                    }
                }
                Panel::Microsoft => return self.on_account(Account::StartBrowser),
                Panel::None => {}
            },
            Account::StartDevice => self.dispatch(AppCommand::StartDeviceLogin),
            Account::StartBrowser => self.dispatch(AppCommand::StartBrowserLogin),
            Account::Cancel => {
                self.account_form.close();
                self.dispatch(AppCommand::CancelDeviceLogin);
            }
            Account::Copy(_) => {}
            Account::Switch(id) => self.dispatch(AppCommand::SwitchAccount(id)),
            Account::Remove(id) => self.dispatch(AppCommand::RemoveAccount(id)),
            Account::Relogin(id) => return self.relogin(&id),
        }
        self.refresh_view();

        match clipboard {
            Some(text) => iced::clipboard::write(text),
            None => Task::none(),
        }
    }

    /// 「重新登录」按**账户类型**分派（复审 r1 P2-3）。
    ///
    /// - 微软轨：走授权码流（原路）；
    /// - 第三方轨：第三方站没有「一键重登」——预填服务器地址与登录名，让用户补密码；
    /// - 本地档案：没有服务器账户可重登 ⇒ 明确说明，不假装能做。
    fn relogin(&mut self, id: &str) -> Task<Message> {
        let Some(account) = self
            .state
            .accounts
            .iter()
            .find(|account| account.id == id)
            .cloned()
        else {
            return Task::none();
        };
        self.account_form.hint = None;
        match account.kind {
            AccountKind::Microsoft => {
                self.account_form.panel = Panel::Microsoft;
                self.dispatch(AppCommand::StartBrowserLogin);
            }
            AccountKind::Authlib => {
                self.account_form.open(Track::Authlib);
                self.account_form.server = account.server_url.clone();
                self.account_form.login = account.login_name.clone();
                self.account_form.password.clear();
                self.account_form.hint = Some(Hint::AuthlibReauth);
                self.view_dirty = true;
                self.refresh_view();
            }
            AccountKind::Offline => {
                self.account_form.hint = Some(Hint::OfflineNoRelogin);
                self.view_dirty = true;
                self.refresh_view();
            }
        }
        Task::none()
    }
}

/// 取证开关 `ULL_ACCOUNT_STATE` 的落地（**只在 `--screenshot` 下生效**）。
///
/// 为什么需要：设备码过期要等 15 分钟、表单要手点两下，截图通道等不到自然发生的状态。
pub(super) fn force_account_state(app: &mut App) {
    if !app.has_screenshot() {
        return;
    }
    let Ok(state) = std::env::var("ULL_ACCOUNT_STATE") else {
        return;
    };
    match state.trim().to_lowercase().as_str() {
        // 设备码已过期：起点往前挪一小时（倒计时自己算出来就是 0）
        "expired" => {
            app.state.device_login_started =
                Some(std::time::Instant::now() - std::time::Duration::from_secs(3_600));
        }
        // 第三方站**失败**态：面板打开 + mock 那边给出的 `AuthlibLoginChanged(Failed)`
        "authlib-failed" => {
            app.account_form.panel = crate::pages::account::Panel::Authlib;
            app.account_form.server = "https://example.invalid/api/yggdrasil/".to_string();
            app.account_form.login = "alex@example.com".to_string();
        }
        // 离线档案失败态：面板打开（横幅由 mock 的 notice 场景给）
        "notice" => {
            app.account_form.panel = crate::pages::account::Panel::Offline;
            app.account_form.offline_name = "ab".to_string();
        }
        // 第三方站表单：直接把面板打开并预填地址
        "authlib" | "authlib-form" => {
            app.account_form.panel = crate::pages::account::Panel::Authlib;
            if app.account_form.server.trim().is_empty() {
                app.account_form.server = "https://littleskin.cn/api/yggdrasil/".to_string();
            }
            app.account_form.login = "steve@example.com".to_string();
            app.account_form.password = "not-a-real-password".to_string();
        }
        // 「关掉之后」：mock 给的是 failed 种子，这里按一次 Close（走真实的
        // `on_account` → `CancelDeviceLogin` 通道），出「面板真的关了」的对照图。
        "closed" => {
            let _ = app.on_account(crate::pages::account::Message::Close);
        }
        // 有账号时「Add an account」展开态（P2-4 的收敛后形态）
        "add" => {
            app.account_form.section_open = true;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
