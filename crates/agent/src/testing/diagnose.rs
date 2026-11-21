//! 内存诊断端口（`DiagnosePort` 的测试替身）。

use crate::port::{
    CrashDiagnosis, DiagnosePort, DiagnosisCandidate, ExitSummary, FixAction, FixActionKind,
    PortResult,
};

/// 内存诊断端口（固定返回一份规则结论）。
#[derive(Debug, Clone)]
pub struct MockDiagnose {
    report: Option<CrashDiagnosis>,
}

impl Default for MockDiagnose {
    fn default() -> Self {
        Self {
            report: Some(Self::forge_missing_library()),
        }
    }
}

impl MockDiagnose {
    /// 构造使用默认结论的端口。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 让端口对所有会话返回「会话不存在」。
    #[must_use]
    pub fn unavailable() -> Self {
        Self { report: None }
    }

    /// 默认结论：Forge 缺库（对应 analyse/02 的高发场景）。
    #[must_use]
    pub fn forge_missing_library() -> CrashDiagnosis {
        CrashDiagnosis {
            session_id: String::new(),
            source: "rules".to_string(),
            needs_more_info: false,
            candidates: vec![DiagnosisCandidate {
                id: "integrity.forge_missing_library".to_string(),
                severity: "error".to_string(),
                category: "integrity".to_string(),
                confidence: 0.86,
                message_key: "diagnose.cause.integrity.forge_missing_library".to_string(),
                message: "Forge 启动缺少 bootstraplauncher 库，通常是版本组件未下载完整"
                    .to_string(),
                detail: "在实例上重装 Forge 组件（或先用官方启动器补全一次再回来）".to_string(),
                evidence: vec![
                    "java.lang.NoClassDefFoundError: cpw/mods/bootstraplauncher/BootstrapLauncher"
                        .to_string(),
                ],
                fix_actions: vec![FixAction {
                    kind: FixActionKind::Reinstall,
                    tool: "component.reinstall".to_string(),
                    label: "重装该版本的 Forge 组件".to_string(),
                    args: [("component".to_string(), "forge".to_string())]
                        .into_iter()
                        .collect(),
                }],
            }],
            secondary: Vec::new(),
            evidence: vec!["Description: Mod loading failure".to_string()],
            exit: ExitSummary {
                code: Some(1),
                exit_type: Some("application_error".to_string()),
                silent_failure: false,
            },
        }
    }
}

impl DiagnosePort for MockDiagnose {
    fn diagnose(&self, session_id: &str) -> PortResult<CrashDiagnosis> {
        self.report.clone().map_or_else(
            || Err(format!("会话 `{session_id}` 不存在")),
            |report| {
                Ok(CrashDiagnosis {
                    session_id: session_id.to_string(),
                    ..report
                })
            },
        )
    }
}
