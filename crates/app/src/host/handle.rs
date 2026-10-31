//! `HostTask` → `CoreEvent` 的翻译（从 `host/mod.rs` 拆出，守 400 行硬限）。
//!
//! 父模块的私有字段对子模块可见，所以这里直接读写 `self.tx` / `self.launches` 等。

use crate::dto::{AppEvent, CoreEvent, InstanceRuntime, LogOutcome};
use crate::toast::ToastKind;

use super::{logs, CoreHost, HostTask, LiveLaunch};

impl CoreHost {
    /// 后台消息 → 事件。
    pub(super) fn handle(&mut self, task: HostTask) {
        match task {
            HostTask::Versions(Ok(versions)) => self.send(CoreEvent::VersionListChanged(versions)),
            HostTask::Versions(Err(reason)) => {
                self.toast(ToastKind::Warn, format!("拉取版本清单失败：{reason}"));
            }
            HostTask::Install(progress) => {
                self.install = Some(*progress.clone());
                self.send(CoreEvent::InstallProgress(Some(*progress)));
            }
            HostTask::InstallDone { progress, result } => {
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
                self.launches.retain(|live| live.instance != finished.id);
                let runtime = if finished.outcome == LogOutcome::Ok {
                    InstanceRuntime::Idle
                } else {
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
