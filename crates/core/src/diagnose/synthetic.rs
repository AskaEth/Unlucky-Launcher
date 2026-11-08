//! 引擎内置结论：退出码 / hs_err 这类**结构化信号**没有文本签名，塞 YAML 别扭，放在这里。
//!
//! 它们参与和其它规则一样的排序与抑制流程（`priority`/`confidence`/`suppress` 语义相同），
//! 所以「主因是驱动、次生是图形规则」这种关系能自动成立。
//! 由 `engine.rs` 用 `#[path]` 声明，拆分只为守单文件 400 行上限。

use super::{DiagnosisInput, Evidence, Finding};
use crate::diagnose::collectors::{
    ExitSignal, ExitType, SignalOrigin, EXIT_DLL_NOT_FOUND, EXIT_STACK_BUFFER_OVERRUN,
};
use crate::diagnose::hs_err::{HsErrKind, HsErrReport};
use crate::diagnose::rules::{
    AdjustMemory, ChangeGraphics, ChangeJava, ComponentKind, FixAction, GraphicsSetting,
    JavaVendor, Reinstall, RuleCategory, SeekHelp, Severity,
};

/// 所有内置结论。
#[must_use]
pub fn synthetic_findings(input: &DiagnosisInput) -> Vec<Finding> {
    let mut out = Vec::new();
    if let Some(exit) = &input.exit {
        out.extend(exit_findings(exit));
    }
    if let Some(report) = &input.hs_err {
        out.extend(hs_err_findings(report));
    }
    out
}

fn exit_findings(exit: &ExitSignal) -> Vec<Finding> {
    let mut out = Vec::new();
    if exit.is_access_violation() {
        let mut finding = base(
            "signal.access_violation",
            RuleCategory::Graphics,
            Severity::Critical,
            0.55,
            82,
            "diagnose.cause.signal.access_violation",
            &format!(
                "进程以 0xC0000005（访问违例）退出：多半是显卡驱动/渲染栈崩溃，建议先更新显卡驱动或换渲染器（{}）",
                exit.summary()
            ),
        );
        finding.fix_actions = vec![FixAction::ChangeGraphics(ChangeGraphics {
            setting: GraphicsSetting::Driver,
            update_driver: true,
            suggested_value: None,
            note: Some("访问违例优先怀疑显卡驱动".to_owned()),
        })];
        finding.suppress = vec!["graphics.graphics_driver".to_owned()];
        finding.evidence = vec![Evidence::new(SignalOrigin::Launcher, 0, exit.summary())];
        out.push(finding);
    }
    if exit.code == Some(EXIT_DLL_NOT_FOUND) {
        let mut finding = base(
            "signal.dll_not_found",
            RuleCategory::Integrity,
            Severity::Critical,
            0.7,
            85,
            "diagnose.cause.signal.dll_not_found",
            "进程以 0xC0000135（找不到 DLL）退出：缺原生库或 Java 安装损坏，重装 natives/Java 组件",
        );
        finding.fix_actions = vec![FixAction::Reinstall(Reinstall {
            component: ComponentKind::Natives,
            path_hint: None,
            note: Some("找不到 DLL：优先修 natives，其次重装 Java".to_owned()),
        })];
        finding.evidence = vec![Evidence::new(SignalOrigin::Launcher, 0, exit.summary())];
        out.push(finding);
    }
    if exit.code == Some(EXIT_STACK_BUFFER_OVERRUN) {
        let mut finding = base(
            "signal.stack_buffer_overrun",
            RuleCategory::Jvm,
            Severity::Warning,
            0.5,
            60,
            "diagnose.cause.signal.stack_buffer_overrun",
            "进程以 0xC0000409（栈缓冲区溢出）退出：JVM/原生库内部错误，需要 hs_err 或崩溃报告进一步判断",
        );
        finding.fix_actions = vec![FixAction::SeekHelp(SeekHelp {
            export_diagnostics: true,
            needs_more_info: vec!["run 目录下的 hs_err_pid*.log".to_owned()],
            note: None,
        })];
        finding.evidence = vec![Evidence::new(SignalOrigin::Launcher, 0, exit.summary())];
        out.push(finding);
    }
    if exit.exit_type == ExitType::Sigkill {
        let mut finding = base(
            "signal.sigkill",
            RuleCategory::Memory,
            Severity::Warning,
            0.6,
            70,
            "diagnose.cause.signal.sigkill",
            "进程被 SIGKILL 强杀：常见于系统内存耗尽（OOM killer）或外部清理进程，建议降低 -Xmx 或扩大虚拟内存",
        );
        finding.fix_actions = vec![FixAction::AdjustMemory(AdjustMemory {
            increase_system_swap: true,
            note: Some("被强杀：先确认系统内存是否够".to_owned()),
            ..AdjustMemory::default()
        })];
        finding.evidence = vec![Evidence::new(SignalOrigin::Launcher, 0, exit.summary())];
        out.push(finding);
    }
    if exit.exit_type == ExitType::JvmError {
        let mut finding = base(
            "signal.jvm_error",
            RuleCategory::Jvm,
            Severity::Critical,
            0.55,
            72,
            "diagnose.cause.signal.jvm_error",
            "JVM 未能启动（游戏根本没跑起来）：先查 -Xmx 是否超过可用内存、JVM 参数是否非法",
        );
        finding.fix_actions = vec![
            FixAction::AdjustMemory(AdjustMemory {
                increase_system_swap: true,
                note: Some("JVM 初始化失败，优先怀疑 -Xmx 过大".to_owned()),
                ..AdjustMemory::default()
            }),
            FixAction::ChangeJava(ChangeJava {
                vendor: JavaVendor::Hotspot,
                note: Some("换成 HotSpot 的官方 Java 试试".to_owned()),
                ..ChangeJava::default()
            }),
        ];
        finding.evidence = vec![Evidence::new(SignalOrigin::Launcher, 0, exit.summary())];
        out.push(finding);
    }
    if exit.silent_failure {
        let mut finding = base(
            "signal.silent_failure",
            RuleCategory::Other,
            Severity::Warning,
            0.4,
            55,
            "diagnose.cause.signal.silent_failure",
            &format!(
                "退出码为 0 但疑似静默失败（{} 条错误行）：游戏可能刚起来就被 mod 异常终止，需要更长的日志确认",
                exit.error_lines
            ),
        );
        finding.fix_actions = vec![FixAction::SeekHelp(SeekHelp {
            export_diagnostics: true,
            needs_more_info: vec!["把日志行数上限调到 20000 后复现一次".to_owned()],
            note: None,
        })];
        finding.evidence = vec![Evidence::new(SignalOrigin::Launcher, 0, exit.summary())];
        out.push(finding);
    }
    out
}

fn hs_err_findings(report: &HsErrReport) -> Vec<Finding> {
    let mut out = Vec::new();
    match report.kind {
        HsErrKind::AccessViolation => {
            let module = report
                .failed_frame
                .as_ref()
                .and_then(|frame| frame.module.clone())
                .unwrap_or_else(|| "未知模块".to_owned());
            let mut finding = base(
                "hs_err.access_violation",
                RuleCategory::Graphics,
                Severity::Critical,
                0.75,
                88,
                "diagnose.cause.hs_err.access_violation",
                &format!(
                    "JVM 致命错误 {module}（访问违例，失败帧 {}）：原生层崩溃，显卡驱动/渲染栈是首要嫌疑",
                    report
                        .failed_frame
                        .as_ref()
                        .map_or("未知", |frame| frame.raw.as_str())
                ),
            );
            finding.fix_actions = vec![
                FixAction::ChangeGraphics(ChangeGraphics {
                    setting: GraphicsSetting::Driver,
                    update_driver: true,
                    suggested_value: None,
                    note: Some(format!("失败帧落在 {module}")),
                }),
                FixAction::SeekHelp(SeekHelp {
                    export_diagnostics: true,
                    needs_more_info: Vec::new(),
                    note: Some("把 hs_err 一并上报，便于确认驱动版本".to_owned()),
                }),
            ];
            finding.suppress = vec!["graphics.graphics_driver".to_owned()];
            finding.evidence = vec![Evidence::new(SignalOrigin::HsErr, 1, report.summary())];
            out.push(finding);
        }
        HsErrKind::OutOfMemory => {
            let memory = report
                .memory
                .as_ref()
                .map_or_else(String::new, |memory| memory.summary());
            let mut finding = base(
                "hs_err.out_of_memory",
                RuleCategory::Memory,
                Severity::Critical,
                0.85,
                86,
                "diagnose.cause.hs_err.out_of_memory",
                &format!("JVM 因内存不足终止：{memory}，降低 -Xmx 或扩大系统虚拟内存"),
            );
            finding.fix_actions = vec![FixAction::AdjustMemory(AdjustMemory {
                increase_system_swap: true,
                note: Some(memory),
                ..AdjustMemory::default()
            })];
            finding.evidence = vec![Evidence::new(SignalOrigin::HsErr, 1, report.summary())];
            out.push(finding);
        }
        HsErrKind::StackOverflow => {
            let mut finding = base(
                "hs_err.stack_overflow",
                RuleCategory::ModRuntime,
                Severity::Warning,
                0.6,
                62,
                "diagnose.cause.hs_err.stack_overflow",
                "JVM 栈溢出导致的致命错误：通常是某个 mod 的递归/死循环，禁用最近装的 mod 再试",
            );
            finding.fix_actions = vec![FixAction::SeekHelp(SeekHelp {
                export_diagnostics: true,
                needs_more_info: vec!["最近安装/更新的 mod 列表".to_owned()],
                note: None,
            })];
            finding.evidence = vec![Evidence::new(SignalOrigin::HsErr, 1, report.summary())];
            out.push(finding);
        }
        HsErrKind::InternalError => {
            let mut finding = base(
                "hs_err.internal_error",
                RuleCategory::Jvm,
                Severity::Warning,
                0.5,
                62,
                "diagnose.cause.hs_err.internal_error",
                "JVM 内部致命错误：优先换官方 HotSpot Java，其次怀疑原生库（natives）损坏",
            );
            finding.fix_actions = vec![FixAction::ChangeJava(ChangeJava {
                vendor: JavaVendor::Hotspot,
                ..ChangeJava::default()
            })];
            finding.evidence = vec![Evidence::new(SignalOrigin::HsErr, 1, report.summary())];
            out.push(finding);
        }
        HsErrKind::Other => {}
    }
    out
}

fn base(
    rule_id: &str,
    category: RuleCategory,
    severity: Severity,
    confidence: f32,
    priority: u8,
    message_key: &str,
    message: &str,
) -> Finding {
    Finding::skeleton(
        rule_id,
        category,
        severity,
        confidence,
        priority,
        message_key,
        message,
    )
}
