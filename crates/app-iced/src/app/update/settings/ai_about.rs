//! 助手（AI）分组消息的落点（M174）：草稿编辑 / 保存 / 测试连接。
//!
//! 消息口在 `pages/settings/ai_about.rs`（`pages::settings::ai_about::Message`）；状态与读写落在
//! `pages/settings/ai/{state,store}.rs`。本文件只有三件事：
//!
//! | 消息 | 做什么 |
//! |---|---|
//! | `ToggleEnabled` / `PickProvider` / `EditBaseUrl` / `EditModel` / `EditKey` / `Reveal` | 只改草稿（`dirty = true`） |
//! | `Save` | 先 [`State::validate`](pages::settings::ai::State::validate)，再 `ai::store::save`（真写 `launcher.json`） |
//! | `Test` | `Task::perform` 起一次真探测（`unlucky_cli::commands::agent::probe`），结果走 `ProbeDone` |
//!
//! ## 为什么「保存」不派 `AppCommand::SetSetting`
//!
//! 那条指令是可序列化的 DTO，`ai.api_key` 走它就意味着明文进了命令通道（事件日志 / mock 回放都会
//! 看到）。`pages/settings/ai/store.rs` 的模块文档记着完整理由：这里走的是同一条 schema 通路
//! （`SettingStore::set` + `save`），只是不经过那条通道。
//!
//! ## 为什么「测试」读文件而不是读草稿
//!
//! 测的是**已经保存的那份配置**（`probe_file` 每次现读 `launcher.json`）。用户在输入框里改了但没
//! 点保存时，按钮旁边那句「有未保存的改动」会提醒他——而不是偷偷拿草稿去测一个不存在的配置。

use super::super::*;

use crate::pages::settings::ai::{Probe, ProbeResult};

impl App {
    /// 助手分组的一条消息。
    pub(super) fn on_settings_ai_about(
        &mut self,
        message: pages::settings::ai_about::Message,
    ) -> Task<Message> {
        use pages::settings::ai_about::Message;

        match message {
            Message::ToggleEnabled(enabled) => {
                self.settings.ai_about.enabled = enabled;
                self.touch();
            }
            Message::PickProvider(provider) => {
                self.settings.ai_about.provider = provider;
                self.touch();
            }
            Message::EditBaseUrl(value) => {
                self.settings.ai_about.base_url = value;
                self.touch();
            }
            Message::EditModel(value) => {
                self.settings.ai_about.model = value;
                self.touch();
            }
            Message::EditKey(secret) => {
                self.settings.ai_about.key = secret;
                self.touch();
            }
            Message::Reveal(reveal) => {
                self.settings.ai_about.reveal = reveal;
                self.view_dirty = true;
            }
            Message::Save => return self.save_ai_settings(),
            Message::Test => return self.test_ai_connection(),
            Message::ProbeDone(result) => {
                self.settings.ai_about.probe = Probe::Done(*result);
                self.view_dirty = true;
            }
        }

        Task::none()
    }

    /// 记一次「草稿变了」：清掉旧回执（它已经在说另一份配置了），并让视图重建。
    fn touch(&mut self) {
        self.settings.ai_about.dirty = true;
        self.settings.ai_about.notice = None;
        self.view_dirty = true;
    }

    /// 从真值（`config/launcher.json`）灌一次草稿；截图通道改用 `ULL_AI_STATE` 的种子。
    ///
    /// 什么时候调：① 构造之后（首屏就要显示已存的值）；② 配置文件被外部改过（`update/clock.rs`
    /// 的 mtime 轮询）——`ull config set ai.model …` 或 AI 的 `set_config` 改完 1 秒内可见。
    /// **脏草稿不覆盖**（[`State::hydrate`](pages::settings::ai::State::hydrate) 自己判），
    /// 否则外部一改文件就把用户正在输入的内容冲掉。
    pub(in crate::app) fn hydrate_ai_settings(&mut self) {
        if self.options.screenshot.is_some() {
            let raw = std::env::var("ULL_AI_STATE").unwrap_or_else(|_| "unconfigured".to_string());
            if let Some(state) = pages::settings::ai::State::screenshot_seed(&raw) {
                self.settings.ai_about = state;
            }
            return;
        }
        let Some(path) = self.bridge.path().map(std::path::Path::to_path_buf) else {
            return;
        };
        let snapshot = pages::settings::ai::store::load(&path);
        if self.settings.ai_about.hydrate(snapshot) {
            self.view_dirty = true;
        }
    }

    /// 保存：本地校验 → 落盘 → 用**落盘后读回来的真值**刷新草稿。
    fn save_ai_settings(&mut self) -> Task<Message> {
        use pages::settings::ai::{store, SaveNotice};

        if let Err(refusal) = self.settings.ai_about.validate() {
            self.settings.ai_about.notice = Some(SaveNotice::Refused(refusal));
            self.view_dirty = true;
            return Task::none();
        }
        let Some(path) = self.bridge.path().map(std::path::Path::to_path_buf) else {
            self.settings.ai_about.notice = Some(SaveNotice::NoStorage);
            self.view_dirty = true;
            return Task::none();
        };

        let saved = store::save(&path, &self.settings.ai_about);
        match saved {
            Ok(snapshot) => {
                // 先清脏再 hydrate：我们刚写进去的就是这一份，不存在「冲掉用户输入」。
                self.settings.ai_about.dirty = false;
                self.settings.ai_about.hydrate(snapshot);
                self.settings.ai_about.key = pages::settings::ai::Secret::default();
                self.settings.ai_about.notice = Some(SaveNotice::Saved);
            }
            Err(reason) => {
                self.settings.ai_about.notice =
                    Some(pages::settings::ai::SaveNotice::Failed(reason));
            }
        }
        self.view_dirty = true;
        Task::none()
    }

    /// 测试连接：真探测在后台跑（`probe_file` 现读设置文件），结果经 `ProbeDone` 回来。
    fn test_ai_connection(&mut self) -> Task<Message> {
        if !self.settings.ai_about.can_test() {
            return Task::none();
        }
        let Some(path) = self.bridge.path().map(std::path::Path::to_path_buf) else {
            self.settings.ai_about.probe = Probe::Done(ProbeResult::from_outcome(
                unlucky_cli::commands::agent::probe::ProbeOutcome::NotConfigured {
                    missing: unlucky_cli::commands::agent::probe::Missing::BaseUrl,
                },
                0,
            ));
            self.view_dirty = true;
            return Task::none();
        };

        self.settings.ai_about.probe = Probe::Running;
        self.view_dirty = true;
        Task::perform(
            async move {
                let started = std::time::Instant::now();
                let outcome = unlucky_cli::commands::agent::probe::probe_file(
                    &path,
                    unlucky_cli::commands::agent::probe::DEFAULT_TIMEOUT,
                );
                ProbeResult::from_outcome(outcome, started.elapsed().as_millis())
            },
            |result| {
                Message::Page(pages::Message::Settings(
                    pages::settings::GroupMessage::AiAbout(
                        pages::settings::ai_about::Message::ProbeDone(Box::new(result)),
                    ),
                ))
            },
        )
    }
}
