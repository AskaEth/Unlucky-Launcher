//! 「停止」与「收场结论」的落点（M388）：从 `update/agent.rs` 拆出来守 400 行硬线。
//!
//! 两件事放在一起的理由：它们共用同一个问题——**这一轮怎么收场、由谁说出来**。句子在这里按
//! 当前语言成形；内核只报事实（超时 N 秒 / 被取消）。`drain_agent` 每拍抽队列时落地超时结论，
//! `stop_agent` 由「停止」键触发。

use std::time::{Duration, Instant};

use unlucky_i18n::{Catalog, Key as TextKey};

use crate::app::diag::diag;
use crate::shell::agent;

use crate::app::state::presenter::transport::Conclusion;
use crate::app::update::App;

/// AI 助手的三个收场词汇（M388）：超时 / 取消的结论句子在这里成形。
///
/// 为什么句子不写在内核或 worker 里：那两处没有 `Catalog`（presenter 与 agent 都不许依赖 i18n，
/// 写死的中文会出现在英文界面上——M161 立的口径）。内核只报**事实**（超时 N 秒 / 被取消），
/// 句子在这里按当前语言现取词。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// 用户按了「停止」。
    Stopped,
    /// 端点沉默到时限（秒数来自内核，见 `Conclusion::Timeout`）。
    Timeout(u64),
}

impl Outcome {
    /// 这一档的结论正文。
    pub(crate) fn notice(self, catalog: Catalog) -> String {
        match self {
            Self::Stopped => catalog.text(TextKey::AgentStopped).to_string(),
            Self::Timeout(secs) => {
                let secs = secs.to_string();
                catalog.render(TextKey::AgentTimeout, &[("secs", secs.as_str())])
            }
        }
    }
}

impl App {
    /// 抽干 presenter 的事件队列，**经既有 Message 通道**回流（见模块文档末节）。
    ///
    /// M388：内核报上来的**收场事实**（超时）在这里落地——先让这一轮的事件按序 apply（下标、
    /// 流式收敛都靠它们），**然后**才压上结论那条系统条目。顺序反了，迟到的正文会冒在结论下面。
    pub(crate) fn drain_agent(&mut self) {
        let events = self.agent_host.drain();
        if !events.is_empty() {
            let _ = self.on_agent(agent::Message::Events(events));
        }
        let catalog = Catalog::of(self.lang());
        let Some(conclusion) = self.agent_host.take_conclusion() else {
            return;
        };
        let outcome = match conclusion {
            Conclusion::Timeout { secs } => Outcome::Timeout(secs),
        };
        diag!("[agent] 本轮收场：{outcome:?}");
        self.agent_host.conclude(outcome.notice(catalog));
        self.sync_agent_presenter();
        self.view_dirty = true;
        self.refresh_view();
    }

    /// 「停止」（M388）：**不再是空操作**。
    ///
    /// M384 之前的这条路径是 `dispatch(AppCommand::StopAgent)`，它进的是 `MockBackend` 里那个
    /// `AppCommand::StopAgent => {}` 空臂——真后端（`AgentHost` → `RealTransport` → worker 线程）
    /// 与它毫无关系。现在直接走 presenter：静音映射器 + 置取消旗标 + **当场**收尾。
    ///
    /// 诚实边界：内核能在 await 处被打断（那是**中断**）；内核若正停在工具执行或等确认上，
    /// 本函数只能做到「本轮结果一律丢弃 + 释放输入区」。日志里那两句话分开写，不合并成「已停止」。
    pub(crate) fn stop_agent(&mut self, catalog: Catalog) {
        let interrupted = self.agent_host.stop(Outcome::Stopped.notice(catalog));
        diag!(
            "[agent] 停止：{}",
            if interrupted {
                "已通知内核（await 处析构请求）"
            } else {
                "没有可打断的内核调用 ⇒ 本轮只丢弃"
            }
        );
        self.sync_agent_presenter();
        self.view_dirty = true;
        self.refresh_view();
    }

    /// **取证夹具**（`ULL_AGENT_FIXTURE=timeout|retry`）：启动期把事件抽干，直到这一轮收场。
    ///
    /// 为什么在启动期阻塞：产品二进制只在第 3 帧出图（`CAPTURE_AT_FRAME`），而超时要等好几秒
    /// ——不在这里等，图上就永远只有「正在等」的空会话。它替代的是**等待**，不是逻辑：抽干的
    /// 每一条事件、判超时的每一条判据都是产品路径（`AgentHost::drain` → `on_agent`）。
    pub(crate) fn await_agent(&mut self, limit_ms: u64) {
        let deadline = Instant::now() + Duration::from_millis(limit_ms);
        while Instant::now() < deadline {
            self.drain_agent();
            if !self.agent_host.rail().busy {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        diag!("[agent] 夹具等待 {limit_ms} ms 仍在忙（这一轮没收敛）");
    }

    /// **取证夹具**：固定等一段时间，期间持续抽干事件（正文才能一段段上屏）。
    pub(crate) fn awake_for(&mut self, ms: u64) {
        let deadline = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < deadline {
            self.drain_agent();
            std::thread::sleep(Duration::from_millis(50));
        }
        self.drain_agent();
    }
}
