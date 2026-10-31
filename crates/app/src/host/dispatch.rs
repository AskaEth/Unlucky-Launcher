//! `CoreHost::dispatch` 的指令实现（从 `host/mod.rs` 拆出，守 400 行上限）。
//!
//! 父模块的私有字段对子模块可见，所以这里直接读写 `self.env` / `self.tx` 等。

use crate::dto::{AppCommand, Backend, CoreEvent, CreateRequest, LoaderKind};
use crate::toast::ToastKind;

use super::accounts::{self, LoginJob};
use super::{game, install, java, launch, logs, settings, versions, CoreHost};

impl CoreHost {
    /// 下发一条用户意图（`Backend::dispatch` 的实现体）。
    pub(super) fn dispatch_command(&mut self, command: AppCommand) {
        self.dispatch_inner(command);
    }
}
impl CoreHost {
    /// 指令分发（见 `dispatch_command`）。
    fn dispatch_inner(&mut self, command: AppCommand) {
        match command {
            AppCommand::RefreshAll => self.refresh_all(),
            AppCommand::LaunchInstance(id) => self.start_launch(&id),
            AppCommand::StopInstance(id) => match self.running_pid(&id) {
                Some(pid) => match launch::stop(pid) {
                    Ok(()) => self.toast(ToastKind::Info, "已请求停止游戏进程"),
                    Err(reason) => self.toast(ToastKind::Error, reason),
                },
                None => self.toast(ToastKind::Warn, "这个实例当前没有在跑"),
            },
            AppCommand::ListVersions(_) => versions::spawn_list(self.env.clone(), self.tx.clone()),
            AppCommand::InstallVersion(version) => {
                let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                self.install_cancel = Some(std::sync::Arc::clone(&cancel));
                install::spawn_install(
                    self.env.clone(),
                    self.tx.clone(),
                    CreateRequest {
                        name: version.clone(),
                        version,
                        loader: LoaderKind::Vanilla,
                        loader_version: String::new(),
                    },
                    cancel,
                );
            }
            AppCommand::CreateInstance(request) => {
                let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                self.install_cancel = Some(std::sync::Arc::clone(&cancel));
                install::spawn_install(self.env.clone(), self.tx.clone(), request, cancel);
            }
            AppCommand::CancelInstall => {
                // 取消只在阶段边界生效（core 的下载循环没有取消钩子）：这里先把面板收回，
                // 后台线程会在下一阶段开始前看到标记并收尾。
                if let Some(cancel) = &self.install_cancel {
                    cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                }
                self.install = None;
                self.send(CoreEvent::InstallProgress(None));
            }
            AppCommand::SetIsolation { id, mode } => {
                match game::set_isolation(&self.env, &id, &mode) {
                    Ok(()) => {
                        self.toast(ToastKind::Success, "隔离档位已更新");
                        self.refresh_instances();
                    }
                    Err(reason) => self.toast(ToastKind::Error, reason),
                }
            }
            AppCommand::RefreshLogSessions => {
                self.send(CoreEvent::LogSessionsChanged(logs::sessions(&self.env)));
            }
            AppCommand::OpenLogSession(id) => self.open_session(&id),
            AppCommand::ImportCrashLog(path) => match logs::analyze_crash_file(&path) {
                Ok((label, report)) => {
                    self.send(CoreEvent::DiagnosisReady(report));
                    self.toast(ToastKind::Info, label);
                }
                Err(reason) => self.toast(ToastKind::Warn, reason),
            },
            AppCommand::AddOfflineAccount(name) => match accounts::add_offline(&self.env, &name) {
                Ok(display) => {
                    self.toast(ToastKind::Success, format!("已添加离线账户 {display}"));
                    self.refresh_accounts();
                }
                Err(reason) => self.toast(ToastKind::Error, reason),
            },
            AppCommand::RemoveAccount(key) => match accounts::remove(&self.env, &key) {
                Ok(display) => {
                    if self.active_account.as_deref() == Some(key.as_str()) {
                        self.active_account = None;
                    }
                    self.toast(ToastKind::Info, format!("已删除账户 {display}"));
                    self.refresh_accounts();
                }
                Err(reason) => self.toast(ToastKind::Error, reason),
            },
            AppCommand::SwitchAccount(id) => {
                self.active_account = Some(id);
                self.refresh_accounts();
                self.toast(ToastKind::Info, "已切换启动账户");
            }
            AppCommand::StartDeviceLogin => {
                self.login = Some(LoginJob::new());
                let job = self.login.clone().unwrap_or_default();
                self.send(CoreEvent::DeviceLoginChanged(crate::dto::DeviceLoginView {
                    phase: crate::dto::DeviceLoginPhase::Requesting,
                    message: "正在向微软申请设备码…".to_string(),
                    ..crate::dto::DeviceLoginView::new()
                }));
                accounts::start_login(self.env.clone(), self.tx.clone(), job);
            }
            AppCommand::CancelDeviceLogin => {
                if let Some(job) = &self.login {
                    job.cancel();
                }
                self.login = None;
                self.send(CoreEvent::DeviceLoginChanged(
                    crate::dto::DeviceLoginView::new(),
                ));
            }
            AppCommand::InstallJava(vendor) => {
                self.java_busy = true;
                self.java_message = format!("正在安装 Java（{vendor}）…");
                self.refresh_java();
                java::spawn_install(
                    self.env.clone(),
                    self.tx.clone(),
                    unlucky_core::game::java::JdkVendor::parse(&vendor),
                );
            }
            AppCommand::SetSetting { key, value } => match settings::set(&self.env, &key, &value) {
                Ok(()) => {
                    self.send(CoreEvent::SettingsChanged(settings::groups(&self.env)));
                    self.java_chain.clear();
                    self.refresh_java();
                }
                Err(reason) => self.toast(ToastKind::Error, reason),
            },
            AppCommand::DismissNotice(id) => {
                self.notices.retain(|notice| notice.id != id);
                self.send(CoreEvent::NoticesChanged(self.notices.clone()));
            }
            // AI 对话：agent 的 GUI host 未落地前用脚本回复（不接真实模型）
            other @ (AppCommand::AskAgent(_) | AppCommand::StopAgent | AppCommand::SetYolo(_)) => {
                self.chat.dispatch(other);
                for event in self.chat.drain_events() {
                    self.events.push_back(event);
                }
            }
        }
    }
}
