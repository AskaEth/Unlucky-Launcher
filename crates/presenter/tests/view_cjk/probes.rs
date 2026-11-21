//! N2 守门（`view_cjk.rs`）的**探针与夹具**：从 `view_cjk.rs` 拆出来（M392）。
//!
//! 为什么要拆：白名单在 M392 从 3 条 `Diagnosis.*` 降到 1 条具名透传之后，探针要覆盖的新字段
//! 变多（`cause_key` / `severity` / `groups` / `system_text`），`view_cjk.rs` 一路长到 465 行，
//! 撞上了全仓 400 行红线（`.github/workflows/guards.yml` 的 `file-size` job 按 `git ls-files` 判）。
//!
//! 拆的缝是「**造探针**」和「**断言**」：本文件只造（类型、判据、夹具），`view_cjk.rs` 只断言。
//! 判据 `is_cjk` 留在 `view_cjk.rs`（它有一条自己的边界用例），本文件用 `super::is_cjk`。

use std::collections::BTreeMap;

use unlucky_presenter::dto::{
    Diagnosis, DiagnosisReport, DiagnosisSeverity, LaunchFailureStage, LaunchFailureView, LogLevel,
    LogLine, LogOutcome, LogSession, NoticeLevel, NoticeView, VersionKind, VersionSummary,
};
use unlucky_presenter::toast::{Toast, ToastKind};

use super::is_cjk;

/// 一条上屏字符串的探针。
pub(super) struct Probe {
    /// 具名字段（`<类型>.<字段>`），白名单按它匹配。
    field: &'static str,
    /// 实际要检查的文本（拿走所有权：夹具在函数里造，探针要活得比它久）。
    text: String,
}

/// 判定：返回所有「含 CJK 且不在白名单里」的字段（`(字段, 首个 CJK 字符)`）。
///
/// 白名单作为参数传进来是**故意的**：红样本用例要能传一份空白名单，证明判据本身会红。
pub(super) fn violations(probes: &[Probe], allowed: &[&str]) -> Vec<(&'static str, char)> {
    let mut found = Vec::new();
    for probe in probes {
        if allowed.contains(&probe.field) {
            continue;
        }
        if let Some(ch) = probe.text.chars().find(|ch| is_cjk(*ch)) {
            found.push((probe.field, ch));
        }
    }
    found
}

/// 一条 `DiagnosisReport` 的探针（**穷尽解构**：`Diagnosis` 加字段时这里编译不过 ⇒ 逼一次判断）。
pub(super) fn diagnosis_probes(report: &DiagnosisReport) -> Vec<Probe> {
    let DiagnosisReport { session, diagnosis } = report;
    let Diagnosis {
        rule,
        cause_key,
        severity,
        confidence: _,
        groups,
        system_text,
        fix,
    } = diagnosis;
    let mut probes = vec![
        Probe {
            field: "DiagnosisReport.session",
            text: session.to_string(),
        },
        Probe {
            field: "Diagnosis.rule",
            text: rule.to_string(),
        },
        Probe {
            field: "Diagnosis.cause_key",
            text: cause_key.to_string(),
        },
        Probe {
            field: "Diagnosis.severity",
            text: severity.as_str().to_string(),
        },
        Probe {
            field: "Diagnosis.groups",
            text: groups
                .iter()
                .flat_map(|(key, value)| [key.clone(), value.clone()])
                .collect::<Vec<String>>()
                .join(" "),
        },
    ];
    if let Some(system_text) = system_text {
        probes.push(Probe {
            field: "Diagnosis.system_text",
            text: system_text.to_string(),
        });
    }
    if let Some(fix) = fix {
        probes.push(Probe {
            field: "Diagnosis.fix",
            text: fix.to_string(),
        });
    }
    probes
}

/// 一条**内置结论**（`signal.*`，目录里没有条目）：`system_text` 是 core 拼好的中文句子。
///
/// 故意用真规则的措辞（`signal.silent_failure` 那条），而不是 `"中文"` 这种假样本——
/// 假样本证明不了「今天真的会红」。
pub(super) fn chinese_diagnosis() -> DiagnosisReport {
    DiagnosisReport {
        session: "2026-11-18T10-00-00-abc123".to_string(),
        diagnosis: Diagnosis {
            rule: "signal.silent_failure".to_string(),
            cause_key: "diagnose.cause.signal.silent_failure".to_string(),
            severity: DiagnosisSeverity::Warning,
            confidence: 40,
            groups: BTreeMap::new(),
            system_text: Some(
                "退出码为 0 但疑似静默失败（3 条错误行）：游戏可能刚起来就被 mod 异常终止"
                    .to_string(),
            ),
            fix: Some("diagnostics.report".to_string()),
        },
    }
}

/// 一条**规则结论**（`graphics.*`，目录里有条目）：英文/机器串，`system_text` 也留中文
/// —— 它只是 core 的原话备份，界面在目录命中时**不画**它。
pub(super) fn rule_diagnosis() -> DiagnosisReport {
    DiagnosisReport {
        session: "2026-11-18T10-05-00-def456".to_string(),
        diagnosis: Diagnosis {
            rule: "integrity.unsatisfied_link".to_string(),
            cause_key: "diagnose.cause.integrity.unsatisfied_link".to_string(),
            severity: DiagnosisSeverity::Critical,
            confidence: 85,
            groups: BTreeMap::from([("name".to_string(), "lwjgl_opengl.dll".to_string())]),
            system_text: Some("找不到原生库（natives）：lwjgl_opengl.dll 没解压出来".to_string()),
            fix: Some("component.reinstall".to_string()),
        },
    }
}

/// 全绿的夹具：每个被守字段都是英文/机器串（白名单之外的字段一律不许有 CJK）。
pub(super) fn clean_probes() -> Vec<Probe> {
    let launch = LaunchFailureView::new(LaunchFailureStage::Spawn, "spawn failed: os error 2");
    let LaunchFailureView { stage: _, detail } = &launch;
    let session = LogSession {
        id: "2026-11-18T10-00-00-abc123".to_string(),
        instance: "survival".to_string(),
        when: "11-18 10:00".to_string(),
        outcome: LogOutcome::Ok,
        duration_secs: 42,
    };
    let LogSession {
        id,
        instance,
        when,
        outcome: _,
        duration_secs: _,
    } = &session;
    let line = LogLine {
        text: "[10:00:01] [main/INFO]: Loading Minecraft 1.20.4".to_string(),
        level: LogLevel::Info,
    };
    let LogLine { text, level: _ } = &line;
    let version = VersionSummary {
        id: "1.20.4".to_string(),
        kind: VersionKind::Release,
        released: "2023-12-07".to_string(),
        installed: false,
    };
    let VersionSummary {
        id: version_id,
        kind: _,
        released,
        installed: _,
    } = &version;
    let toast = Toast {
        id: 1,
        kind: ToastKind::Info,
        text: "Installing Fabric 0.16.5".to_string(),
        remaining_ms: 4_000,
    };
    let Toast {
        id: _,
        kind: _,
        text: toast_text,
        remaining_ms: _,
    } = &toast;
    let notice = NoticeView {
        id: "isolation-switched".to_string(),
        level: NoticeLevel::Warn,
        title: "Isolation mode changed".to_string(),
        body: "This instance now runs in its own game directory.".to_string(),
    };
    let NoticeView {
        id: notice_id,
        level: _,
        title,
        body,
    } = &notice;

    vec![
        Probe {
            field: "LaunchFailureView.detail",
            text: detail.to_string(),
        },
        Probe {
            field: "LogSession.id",
            text: id.to_string(),
        },
        Probe {
            field: "LogSession.instance",
            text: instance.to_string(),
        },
        Probe {
            field: "LogSession.when",
            text: when.to_string(),
        },
        Probe {
            field: "LogLine.text",
            text: text.to_string(),
        },
        Probe {
            field: "VersionSummary.id",
            text: version_id.to_string(),
        },
        Probe {
            field: "VersionSummary.released",
            text: released.to_string(),
        },
        Probe {
            field: "Toast.text",
            text: toast_text.to_string(),
        },
        Probe {
            field: "NoticeView.id",
            text: notice_id.to_string(),
        },
        Probe {
            field: "NoticeView.title",
            text: title.to_string(),
        },
        Probe {
            field: "NoticeView.body",
            text: body.to_string(),
        },
    ]
}
