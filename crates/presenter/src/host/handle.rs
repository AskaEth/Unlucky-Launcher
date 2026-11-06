//! `HostTask` → `CoreEvent` 的翻译（从 `host/mod.rs` 拆出，守 400 行硬限）。
//!
//! 父模块的私有字段对子模块可见，所以这里直接读写 `self.tx` / `self.launches` 等。

use crate::dto::{AppEvent, CoreEvent, InstanceRuntime, LogOutcome, PackEvent};
use crate::toast::ToastKind;

use super::{addon, logs, CoreHost, HostTask, LiveLaunch};

impl CoreHost {
    /// 后台消息 → 事件。
    pub(super) fn handle(&mut self, task: HostTask) {
        match task {
            HostTask::Versions(Ok(versions)) => self.send(CoreEvent::VersionListChanged(versions)),
            HostTask::Versions(Err(reason)) => {
                self.toast(ToastKind::Warn, format!("拉取版本清单失败：{reason}"));
            }
            HostTask::LoaderVersions {
                kind,
                mc_version,
                result,
            } => {
                // 失败既要落进快照（页面禁用确认 + 显示具名原因），也要一条 Toast：
                // 用户可能正盯着上面的版本列表，下拉那一行的红字容易被忽略。
                if let Err(reason) = &result {
                    self.toast(
                        ToastKind::Warn,
                        format!("取不到 {} 的加载器版本：{reason}", kind.label()),
                    );
                }
                self.send(CoreEvent::LoaderVersions {
                    kind,
                    mc_version,
                    result,
                });
            }
            // M109：内容库检索。`Err` 不弹 toast——页面自己有失败态（四态之一），
            // toast 只做「顺手提一句」会把可重试的错误变成一次性噪音。
            HostTask::Content(result) => match *result {
                Ok(results) => self.send(CoreEvent::ContentResults(Box::new(results))),
                Err(reason) => self.send(CoreEvent::ContentFailed(reason)),
            },
            // M113: import/export —— 导入/导出事件带代次，取消后回来的旧事件一律丢掉
            HostTask::Pack { event, epoch } => {
                // pack 用**自己**的代次（复审 r1 P2-7）：共用安装的计数器会让
                // 「关一次导入面板」把下载页正在跑的安装静默丢掉。
                if epoch != addon::pack_epoch(&self.env) {
                    return;
                }
                // 导入成功可能登记了新实例：与 `InstallDone` 同一条刷新（复审 r1 P2-1），
                // 否则新实例要等下一次无关的 `RefreshAll` 才出现在实例页与目标 chips 里。
                if matches!(*event, PackEvent::Report(_)) {
                    self.refresh_instances();
                }
                self.send(CoreEvent::Pack(event));
            }
            // M105 的形状（代次丢包）为准：旧任务（已取消 / 被下一轮顶掉）的进度一律丢掉。
            // 这是「点了取消没反应」的根因：取消时 `InstallProgress(None)` 刚把面板收回，
            // 旧任务的下一条「下载中」事件立刻又把它点亮。
            HostTask::Install { progress, epoch } => {
                if epoch != self.install_epoch {
                    return;
                }
                self.install = Some(*progress.clone());
                self.send(CoreEvent::InstallProgress(Some(*progress)));
            }
            HostTask::InstallDone {
                progress,
                result,
                epoch,
            } => {
                // 收尾同样看代次：取消之后那次「已取消」的收尾不该再动面板（面板早就回弹了），
                // 也不该清掉**新一轮**安装的取消句柄。
                if epoch != self.install_epoch {
                    return;
                }
                self.install_cancel = None;
                self.install = Some(*progress.clone());
                self.send(CoreEvent::InstallProgress(Some(*progress)));
                match result {
                    Ok(_) => {
                        self.refresh_instances();
                        self.send(CoreEvent::LogSessionsChanged(logs::sessions(&self.env)));
                    }
                    Err(reason) => self.toast(ToastKind::Error, reason),
                }
            }
            HostTask::Accounts(accounts) => {
                self.accounts = accounts.clone();
                self.send(CoreEvent::AccountsChanged(accounts));
                self.refresh_instances();
            }
            HostTask::Login(view) => self.send(CoreEvent::DeviceLoginChanged(*view)),
            HostTask::Authlib(view) => self.send(CoreEvent::AuthlibLoginChanged(*view)),
            HostTask::Avatar { id, skin } => {
                // 抓到/没抓到都记下来（`None` = 试过了，不再重试），再把列表盖好发出去
                self.skins.insert(id, skin);
                self.attach_skins();
                self.send(CoreEvent::AccountsChanged(self.accounts.clone()));
            }
            HostTask::Java(page) => self.send(CoreEvent::JavaChanged(*page)),
            HostTask::Logs {
                sessions,
                lines,
                diagnosis,
            } => {
                self.send(CoreEvent::LogSessionsChanged(sessions));
                self.send(CoreEvent::LogLinesChanged(lines));
                if let Some(report) = diagnosis {
                    self.send(CoreEvent::DiagnosisReady(report));
                }
            }
            HostTask::Sessions(sessions) => self.send(CoreEvent::LogSessionsChanged(sessions)),
            HostTask::Tail(batch) => {
                if batch.running {
                    self.send(CoreEvent::LogLinesChanged(batch.lines));
                } else {
                    self.send(CoreEvent::LogAppended(*batch));
                }
            }
            HostTask::LaunchStarted {
                id,
                session_id,
                pid,
            } => {
                self.pending.remove(&id);
                self.failed.remove(&id);
                self.launches.retain(|live| live.instance != id);
                self.launches.push(LiveLaunch {
                    instance: id.clone(),
                    session: session_id.clone(),
                    pid,
                });
                // 实时尾随一次只跟一场（下载页/日志页共用一个滚动窗口）；
                // 已有别的实例在尾随时不抢占——它的日志仍在 JSONL 里，收尾后可回放。
                if self.tail.session().is_none() {
                    self.tail.attach(
                        self.env
                            .sessions_root()
                            .join("game")
                            .join(format!("{session_id}.jsonl")),
                        &session_id,
                    );
                }
                self.send(CoreEvent::InstanceRuntimeChanged {
                    id,
                    runtime: InstanceRuntime::Running { pid },
                });
            }
            HostTask::Runtime { id, runtime } => {
                self.send(CoreEvent::InstanceRuntimeChanged { id, runtime });
            }
            HostTask::LaunchEnd(finished) => {
                // 收尾可能发生在 LaunchStarted 之前（build_request / spawn 失败只发
                // LaunchEnd）：pending 必须在这里也清掉，否则 in_flight 恒真、该实例
                // 本会话内再也起不来。
                self.pending.remove(&finished.id);
                self.launches.retain(|live| live.instance != finished.id);
                let runtime = if finished.outcome == LogOutcome::Ok {
                    self.failed.remove(&finished.id);
                    InstanceRuntime::Idle
                } else {
                    // 记住失败原因：随后的 refresh_instances 会重建快照，
                    // 不记的话磁贴的「失败/重试」外观会被 Idle 盖掉。
                    self.failed
                        .insert(finished.id.clone(), finished.summary.clone());
                    InstanceRuntime::Failed {
                        reason: finished.summary.clone(),
                    }
                };
                self.send(CoreEvent::InstanceRuntimeChanged {
                    id: finished.id.clone(),
                    runtime,
                });
                self.send(CoreEvent::LaunchFinished((*finished).clone()));
                // 只拆自己那场的尾随：并发时另一实例的实时日志不该被这次收尾掐掉。
                if self.tail.session() == Some(finished.session.as_str()) {
                    self.tail.detach();
                }
                self.send(CoreEvent::LogSessionsChanged(logs::sessions(&self.env)));
                self.refresh_instances();
            }
            HostTask::Crash(report) => {
                self.send(CoreEvent::DiagnosisReady(*report));
            }
            HostTask::Notice(notice) => self.push_notice(*notice),
            HostTask::Toast(toast) => self.events.push_back(AppEvent::Toast(*toast)),
        }
    }

    /// 尾随运行中会话（每 `TAIL_POLL_TICKS` 拍一次）。
    pub(super) fn poll_tail(&mut self) {
        let Some((session, lines)) = self.tail.poll() else {
            return;
        };
        if lines.is_empty() {
            return;
        }
        let running = self.launches.iter().any(|live| live.session == session);
        self.send(CoreEvent::LogAppended(crate::dto::LiveLogBatch {
            session,
            lines,
            running,
        }));
    }
}
