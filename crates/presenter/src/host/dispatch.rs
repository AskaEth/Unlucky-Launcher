//! `CoreHost::dispatch` 的指令实现（从 `host/mod.rs` 拆出，守 400 行上限）。
//!
//! 父模块的私有字段对子模块可见，所以这里直接读写 `self.env` / `self.tx` 等。

use crate::dto::{
    AccountNoticeCode, AppCommand, Backend, CoreEvent, CreateRequest, LoaderKind, PackEvent,
};
use crate::toast::ToastKind;

use super::accounts::{self, LoginJob};
use super::{
    addon, game, install, java, launch, loader_versions, logs, mods, saves, settings, versions,
    CoreHost,
};

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
            AppCommand::ListLoaderVersions { kind, mc_version } => {
                loader_versions::spawn_list(self.env.clone(), self.tx.clone(), kind, mc_version);
            }
            AppCommand::InstallVersion(version) => {
                let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                self.install_cancel = Some(std::sync::Arc::clone(&cancel));
                let epoch = self.next_install_epoch();
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
                    epoch,
                );
            }
            AppCommand::CreateInstance(request) => {
                let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                self.install_cancel = Some(std::sync::Arc::clone(&cancel));
                let epoch = self.next_install_epoch();
                install::spawn_install(self.env.clone(), self.tx.clone(), request, cancel, epoch);
            }
            AppCommand::CancelInstall => {
                // 取消的三级语义（M105 实测后定稿，见 `install::CANCELLED`）：
                // ① **面板立刻收回**（不等后台，用户点了就有反应）；
                // ② **作废旧任务的代次**：后台线程是异步停的（取消延迟 = 一个数据块），
                //    它带回来的进度事件一律因代次不符被 `handle` 丢掉——面板不会被重新点亮。
                //    这是「点了没反应」的真根因：以前生效了又被旧事件盖回去；
                // ③ 后台线程经 `install::run` 的取消检查停在段边界、**绝不登记实例**，
                //    core 的 `execute_with_cancel` 还会就地掐断在飞的那个文件。
                //
                // 为什么不在这里 join 后台线程：UI 线程绝不能为了「等它停」而阻塞
                // （一个数据块的延迟在人眼下就是「立刻」，但足以让 UI 卡住一帧以上）。
                if let Some(cancel) = &self.install_cancel {
                    cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                }
                // M113: import/export —— 同一块面板的「取消」对导入也生效：立标记 + 作废 pack 代次。
                // 返回值告诉我们「刚才有没有导入在跑」：有的话**不能**再发 install 那条取消信号
                // （它说的是「安装已取消、实例未创建」，而导入的取消是**管线级**的：core 在检查点
                // 停手并回滚，文案见 `PackCancelled`；M195 之前它会在后台跑完并登记实例）。
                let pack_was_running = addon::cancel_pack(&self.env);
                if pack_was_running {
                    self.send(CoreEvent::Pack(Box::new(PackEvent::Cancelled)));
                }
                self.install_epoch = self.install_epoch.wrapping_add(1);
                self.install = None;
                self.send(CoreEvent::InstallProgress(None));
                // 只发信号、不发文案（复审 P2-2）：presenter 没有 i18n 依赖，
                // 写死的中文会和页面语言打架。提示语由 app 层按 `Key` 现求。
                // 导入在跑时不发这条：下载页那条文案说的是「实例未创建」，对导入是撒谎。
                if !pack_was_running {
                    self.send(CoreEvent::InstallCancelled);
                }
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
            AppCommand::SetInstanceLaunchArgs { id, patch } => {
                match self.apply_launch_args(&id, &patch) {
                    Ok(()) => {
                        self.toast(ToastKind::Success, "启动参数已保存");
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
            // 账户增删切：结果走**页面横幅**（`AccountNoticeChanged`），不发 toast——
            // app-iced 不渲染 toast，发出去等于用户什么都看不到（复审 r1 P2-2）。
            AppCommand::AddOfflineAccount(name) => match accounts::add_offline(&self.env, &name) {
                Ok(display) => {
                    self.notice(AccountNoticeCode::Added, &display, "");
                    self.refresh_accounts();
                }
                Err(failure) => {
                    self.notice_failure(failure, &name);
                    self.refresh_accounts();
                }
            },
            AppCommand::RemoveAccount(key) => match accounts::remove(&self.env, &key) {
                Ok(display) => {
                    if self.active_account.as_deref() == Some(key.as_str()) {
                        self.active_account = None;
                    }
                    self.notice(AccountNoticeCode::Removed, &display, "");
                    self.refresh_accounts();
                }
                Err(failure) => {
                    self.notice_failure(failure, &key);
                    self.refresh_accounts();
                }
            },
            AppCommand::SwitchAccount(id) => {
                let name = self
                    .accounts
                    .iter()
                    .find(|account| account.id == id)
                    .map_or_else(|| id.clone(), |account| account.name.clone());
                self.active_account = Some(id);
                self.refresh_accounts();
                self.notice(AccountNoticeCode::Switched, &name, "");
            }
            AppCommand::DismissAccountNotice => {
                self.send(CoreEvent::AccountNoticeChanged(None));
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
            AppCommand::StartBrowserLogin => {
                self.login = Some(LoginJob::new());
                let job = self.login.clone().unwrap_or_default();
                self.send(CoreEvent::DeviceLoginChanged(crate::dto::DeviceLoginView {
                    phase: crate::dto::DeviceLoginPhase::Requesting,
                    message: "正在准备浏览器登录（本地回调端口）…".to_string(),
                    ..crate::dto::DeviceLoginView::new()
                }));
                accounts::start_browser_login(self.env.clone(), self.tx.clone(), job);
            }
            AppCommand::CancelDeviceLogin => {
                if let Some(job) = &self.login {
                    job.cancel();
                }
                self.login = None;
                // 第三方轨的面板也归这条「关掉登录面板」：两条轨都不该留一个转不完的圈
                if let Some(job) = &self.authlib {
                    job.cancel();
                }
                self.authlib = None;
                self.send(CoreEvent::DeviceLoginChanged(
                    crate::dto::DeviceLoginView::new(),
                ));
                self.send(CoreEvent::AuthlibLoginChanged(
                    crate::dto::AuthlibLoginView::new(),
                ));
            }
            AppCommand::AddAuthlibAccount(request) => {
                self.authlib = Some(LoginJob::new());
                let job = self.authlib.clone().unwrap_or_default();
                self.send(CoreEvent::AuthlibLoginChanged(
                    crate::dto::AuthlibLoginView::running(&request.server, "正在校验服务器地址…"),
                ));
                accounts::start_authlib_login(self.env.clone(), self.tx.clone(), job, request);
            }
            AppCommand::ProbeAuthlibServer(server) => {
                // 只探活：后台线程跑服务发现，结果回到同一条 `AuthlibLoginChanged`
                let env = self.env.clone();
                let tx = self.tx.clone();
                accounts::spawn_probe(env, tx, server);
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
                    // 「重新检测地区」：`settings::set` 已经删了 `region.json` 并让共享引擎失效，
                    // 但自更新的 `Factory` 常驻持有旧引擎 —— 不在这条路径上重建它，自更新会继续
                    // 按旧地区档案跑，直到重启（M165 复审 P2-1）。
                    if key == super::engine::DETECT_REGION_ACTION {
                        self.refresh_update_factory();
                    }
                    // M193：下载三键（并发/重试/gh-proxy）改了要立刻换引擎参数。这是**显式**的同步点
                    // ——不藏在任何读函数里。值没变时它一行不动。
                    if settings::download::apply_tuning(&self.env) {
                        self.refresh_update_factory();
                    }
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
            // M109：内容库检索（网络在后台线程，结果经 `HostTask::Content` 回来）
            AppCommand::SearchContent(query) => {
                addon::spawn_search(self.env.clone(), self.tx.clone(), query);
            }
            // M162：详情/版本列表（选中项目才发；结果经 `CoreEvent::ContentDetail` 回来）
            AppCommand::LoadContentDetail(request) => {
                addon::spawn_detail(self.env.clone(), self.tx.clone(), *request);
            }
            // M109：把某个内容项目装进指定实例。取消标记与安装面板共用同一条路
            // （`CancelInstall` 会把标记置位，下载在阶段边界收尾）。
            // M113: import/export —— 导入 / 预检 / 确认写盘 / 关面板
            AppCommand::Pack(command) => self.pack(*command),
            // M223: 导入已有版本 —— 扫描 / 预检 / 确认登记 / 关面板
            AppCommand::Adopt(command) => self.adopt(*command),
            AppCommand::InstallContent(request) => {
                let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                self.install_cancel = Some(std::sync::Arc::clone(&cancel));
                // 代次与版本安装共用同一条路（M105 的形状）：取消/被顶掉之后，旧任务带回来的
                // 进度事件一律被 `handle` 丢掉，内容库种的这次安装不会把面板重新点亮。
                let epoch = self.next_install_epoch();
                addon::spawn_install(self.env.clone(), self.tx.clone(), *request, cancel, epoch);
            }
            // M159: 自更新 —— 检查 / 下载替换 / 拒绝 / 开关
            AppCommand::CheckUpdate => self.start_update_check(true),
            AppCommand::ApplyUpdate => self.start_update_apply(),
            AppCommand::DismissUpdate => {
                self.update.dismiss();
                self.publish_update();
            }
            AppCommand::SetUpdateAutoCheck(on) => match self.update.set_auto_check(on) {
                Ok(()) => self.publish_update(),
                Err(reason) => {
                    // 写不进去也要把这次的选择留在会话里（不把开关弹回去），并说清原因。
                    self.publish_update();
                    self.toast(ToastKind::Warn, reason);
                }
            },
            // M224：本地 Mod 列表 / 启停（磁盘在后台线程碰；这里只算"游戏在不在跑"）
            AppCommand::Mods(command) => {
                let id = match &*command {
                    crate::dto::ModsCommand::List(id) => id.clone(),
                    crate::dto::ModsCommand::SetEnabled { id, .. } => id.clone(),
                };
                // 运行中挡在 host 这一侧（不只是 UI 灰掉）：Windows 上进程正持有 jar，
                // rename 必然失败，与其让用户读一句 OS 报错，不如直说。
                let running = self.in_flight(&id);
                mods::dispatch(self.env.clone(), self.tx.clone(), *command, running);
            }
            // M235：实例存档 / 截图的只读列表 + 打开文件夹；M247 加了三个写操作。
            //
            // **写操作要看"游戏在不在跑"**（M247 补齐，与 `AppCommand::Mods` 同款判据）：
            // Windows 上进程占着世界目录 ⇒ rename/删除会撞共享冲突；**Linux 上删一个正在跑的世界
            // 会成功**——那是在用户正在玩的时候把存档删掉。所以判据放在 host 这一侧、落 core 之前，
            // 在飞就回一条结构化失败「游戏正在运行，先退出再操作」，一个字节都不碰盘。
            // 读路径与「打开文件夹」不受影响（它们不改盘）。
            AppCommand::Saves(command) => {
                let id = saves::instance_of(&command).to_string();
                let running = self.in_flight(&id);
                saves::dispatch(self.env.clone(), self.tx.clone(), *command, running);
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
