//! 日志页：会话列表、日志行、D8 诊断卡片。

use crate::dto::{LogLevel as DtoLogLevel, LogOutcome as DtoLogOutcome};
use crate::state::AppState;
use crate::{LogLevel, LogOutcome, UiDiagnosis, UiLogLine, UiLogSession};

/// 日志会话列表。
pub fn log_sessions(state: &AppState) -> Vec<UiLogSession> {
    state
        .log_sessions
        .iter()
        .map(|session| UiLogSession {
            id: session.id.clone().into(),
            when: session.when.clone().into(),
            instance: session.instance.clone().into(),
            outcome: match session.outcome {
                DtoLogOutcome::Ok => LogOutcome::Ok,
                DtoLogOutcome::Crashed => LogOutcome::Crashed,
                DtoLogOutcome::SilentFail => LogOutcome::SilentFail,
            },
            duration: duration(session.duration_secs).into(),
        })
        .collect()
}

/// 秒 → `1h03m` / `62s`。
pub fn duration(secs: u64) -> String {
    if secs >= 3_600 {
        format!("{}h{:02}m", secs / 3_600, (secs % 3_600) / 60)
    } else if secs >= 60 {
        format!("{}m{:02}s", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    }
}

/// 当前会话日志行。
pub fn log_lines(state: &AppState) -> Vec<UiLogLine> {
    state
        .log_lines
        .iter()
        .map(|line| UiLogLine {
            text: line.text.clone().into(),
            level: match line.level {
                DtoLogLevel::Debug => LogLevel::Debug,
                DtoLogLevel::Info => LogLevel::Info,
                DtoLogLevel::Warn => LogLevel::Warn,
                DtoLogLevel::Error => LogLevel::Error,
            },
        })
        .collect()
}

/// 诊断卡片内容（无结论时返回默认值 + has=false）。
pub fn diagnosis(state: &AppState) -> (UiDiagnosis, bool) {
    match &state.diagnosis {
        None => (UiDiagnosis::default(), false),
        Some(report) => {
            let diagnosis = &report.diagnosis;
            (
                UiDiagnosis {
                    rule: diagnosis.rule.clone().into(),
                    title: diagnosis.title.clone().into(),
                    confidence: i32::from(diagnosis.confidence),
                    hint: diagnosis.hint.clone().into(),
                    has_fix: diagnosis.fix.is_some(),
                },
                true,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::duration;

    #[test]
    fn duration_is_human_readable() {
        assert_eq!(duration(8), "8s");
        assert_eq!(duration(62), "1m02s");
        assert_eq!(duration(3_780), "1h03m");
    }
}
