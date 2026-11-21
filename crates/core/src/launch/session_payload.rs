//! 会话事件 payload 构造（schema 见 `DevDocs/SubAgentsTalk/launch-接口约定.md` §3）。
//!
//! 拆出来是为了让 [`crate::launch::session::SessionWriter`] 只关心“写文件 + seq + 索引”，
//! 字段定义集中在一处，评审时一眼能对完整张表。

use std::path::Path;

use serde_json::{json, Value};

use crate::launch::args_table::AssembledCommand;
use crate::launch::error::AssemblyError;
use crate::launch::exit_analysis::ExitAnalysis;
use crate::launch::log_level::Level;
use crate::launch::natives::NativesReport;
use crate::launch::redact::Redactor;
use crate::launch::session::SessionMeta;
use crate::launch::window_detect::Detection;

/// `session_start`。
pub(crate) fn session_start(meta: &SessionMeta, schema: u8) -> Value {
    json!({
        "instance_id": meta.instance_id,
        "instance_name": meta.instance_name,
        "game_version": meta.game_version,
        "launcher": {
            "name": meta.launcher_name,
            "version": meta.launcher_version,
        },
        "os": {
            "name": meta.os_name,
            "arch": meta.os_arch,
            "bits": meta.os_bits,
        },
        "schema": schema,
    })
}

/// `cmd_built`：完整 argv（脱敏）+ classpath 摘要 + 组装告警。
pub(crate) fn cmd_built(command: &AssembledCommand, redactor: &Redactor) -> Value {
    let argv = redactor.scrub_all(&command.argv());
    let program = redactor
        .scrub(&command.program.display().to_string())
        .into_owned();
    json!({
        "program": program,
        "argv": argv,
        "main_class": command.main_class,
        "classpath_entries": command.classpath.len(),
        "classpath_missing": Vec::<String>::new(),
        "memory": {
            "xmx_mb": command.memory.max_mb,
            "xms_mb": command.memory.min_mb,
        },
        "encoding": command.encoding,
        "warnings": command.warnings.iter().map(ToString::to_string).collect::<Vec<_>>(),
    })
}

/// `assembly_failed`：启动前失败的结构化事实（结论由 M15 下）。
pub(crate) fn assembly_failed(error: &AssemblyError) -> Value {
    let (code, components) = match error {
        AssemblyError::MissingMainClass => ("missing_main_class", Vec::new()),
        AssemblyError::EmptyClasspath => ("empty_classpath", Vec::new()),
        AssemblyError::EmptyJavaPath => ("empty_java_path", Vec::new()),
        AssemblyError::MalformedArgument { .. } => ("malformed_argument", Vec::new()),
        AssemblyError::NativesExtraction { .. } => ("natives_extraction", Vec::new()),
        AssemblyError::MissingComponents(items) => (
            "missing_components",
            items
                .iter()
                .map(|item| {
                    json!({
                        "name": item.name,
                        "path": item.path.as_ref().map(|path| path.display().to_string()),
                    })
                })
                .collect::<Vec<_>>(),
        ),
    };
    json!({
        "error_code": code,
        "message": error.to_string(),
        "missing_components": components,
    })
}

/// `proc_start`。
pub(crate) fn proc_start(pid: u32, started_at: &str) -> Value {
    json!({ "pid": pid, "started_at": started_at })
}

/// `natives_extracted`：解压事实（`files`/`skipped`/`custom` 是既有约定字段，
/// 其余为 v1 追加字段，读取方按「未知字段忽略」处理）。
pub(crate) fn natives_extracted(report: &NativesReport, redactor: &Redactor) -> Value {
    json!({
        "dir": redactor.scrub(&report.dir.display().to_string()).into_owned(),
        "files": report.files,
        "skipped": report.skipped,
        "custom": report.custom,
        "jars": report.jars,
        "ignored": report.ignored,
        "up_to_date": report.up_to_date,
        "era": report.era.as_str(),
    })
}

/// `killed`。
pub(crate) fn killed(by: &str, signal: Option<&str>) -> Value {
    json!({ "by": by, "signal": signal })
}

/// `stdout` / `stderr` 行事件。
pub(crate) fn log_line(text: &str, level: Level) -> Value {
    json!({ "line": text, "level": level.as_str() })
}

/// `window_detected`。
pub(crate) fn window_detected(detection: &Detection, redactor: &Redactor) -> Value {
    json!({
        "heuristic": detection.heuristic,
        "took_ms": detection.took_ms,
        "evidence": redactor.scrub(&detection.evidence),
    })
}

/// `exit`。
pub(crate) fn exit(analysis: &ExitAnalysis) -> Value {
    json!({
        "code": analysis.code,
        "exit_type": analysis.exit_type.as_str(),
        "uptime_ms": analysis.uptime_ms,
        "error_lines": analysis.error_lines,
        "crash_keywords": analysis.crash_keywords,
        "jvm_keywords": analysis.jvm_keywords,
        "silent_failure": analysis.silent_failure.is_some(),
        "windows_status": analysis.windows_status,
        "needs_diagnosis": analysis.needs_diagnosis,
        "summary": analysis.summary(),
    })
}

/// `crash_found`：`path` 是权威字段，`excerpt` 仅供人读。
pub(crate) fn crash_found(
    source: &str,
    path: &Path,
    excerpt: Option<&str>,
    redactor: &Redactor,
) -> Value {
    json!({
        "source": source,
        "path": path.display().to_string(),
        "excerpt": excerpt.map(|text| redactor.scrub(text).into_owned()),
    })
}

/// `session_end`。
pub(crate) fn session_end(reason: &str, duration_ms: u64) -> Value {
    json!({ "reason": reason, "duration_ms": duration_ms })
}
