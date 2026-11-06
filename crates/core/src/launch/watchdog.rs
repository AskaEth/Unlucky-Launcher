//! 看门狗：给 pump 的 join 加超时。
//!
//! HMCL 的 `ExitWaiter` 在 `waitFor()` 之后无条件 `join` 两条泵线程
//! （`analyse/02` §6.1、§8.5）：若子进程自己不关闭 stdout（例如它又拉起了不退出的
//! 子进程），`onExit` 永远不回调，UI 卡在“正在启动”。这里把“等日志落盘”改成
//! 有预算的等待——超时就带着已收集到的日志继续做退出分析，绝不无限等待。

use std::time::Duration;

use crate::launch::pump::{PumpCompletion, PumpGroup, Stream};

/// 看门狗配置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchdogConfig {
    /// 进程退出后等待日志泵结束的预算。
    pub pump_timeout: Duration,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            pump_timeout: Duration::from_secs(10),
        }
    }
}

/// 看门狗判定。
#[derive(Debug, Clone)]
pub enum PumpVerdict {
    /// 两条流都正常结束。
    Complete(PumpCompletion),
    /// 超时：仍有流未结束（附带是哪几条）。
    Stalled {
        /// 等待结果（含已完成流与行数）。
        completion: PumpCompletion,
        /// 卡住的流。
        streams: Vec<Stream>,
        /// 等待了多久。
        waited: Duration,
    },
}

impl PumpVerdict {
    /// 是否一切正常。
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Complete(_))
    }

    /// 等待结果（两种情形都有）。
    pub fn completion(&self) -> &PumpCompletion {
        match self {
            Self::Complete(completion) => completion,
            Self::Stalled { completion, .. } => completion,
        }
    }
}

/// 用配置里的预算等待日志泵结束。
pub fn wait_for_pumps(group: &mut PumpGroup, config: &WatchdogConfig) -> PumpVerdict {
    let completion = group.wait(config.pump_timeout);
    if completion.is_complete() {
        PumpVerdict::Complete(completion)
    } else {
        let streams = completion.stalled.clone();
        PumpVerdict::Stalled {
            completion,
            streams,
            waited: config.pump_timeout,
        }
    }
}
