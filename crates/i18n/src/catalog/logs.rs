//! 日志页文案（M95）。
//!
//! 日志页的**界面文案**（标题 / 按钮 / 分组名 / 空态）。诊断结论的正文不在这里：
//! 那是规则表 `rules.d/*.yaml` 的输出（数据），见 `pages/logs/diagnostic.rs` 的模块文档。
//!
//! `LogsStatus` 是**模板**：中英差别落在分隔符上（中文用全角竖线——`·` 是外来符号，
//! 中文正文里偏挤；这也让「中文目录必须与英文不同」这条守卫成立，不是为过测试硬造差异）。
//!
//! 域文件的形状见 [`crate::catalog`] 的「域装配」一节。

macro_rules! logs_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // --- logs page (M95) ---
            LogsSubtitleIdle => ("Nothing is running right now", "当前没有运行中的实例"),
            LogsStatus => ("{instance} · {when} · {outcome}", "{instance}｜{when}｜{outcome}"),
            LogsOpenFolder => ("Open folder", "打开目录"),
            LogsClear => ("Clear", "清空"),
            LogsOutputTitle => ("OUTPUT", "输出"),
            LogsDiagnosisTitle => ("DIAGNOSIS", "诊断"),
            LogsConfidenceHigh => ("HIGH CONFIDENCE", "高置信度"),
            LogsConfidenceMedium => ("MEDIUM CONFIDENCE", "中置信度"),
            LogsConfidenceLow => ("LOW CONFIDENCE", "低置信度"),
            LogsSubtitleLive => ("Running now — logs stream as they arrive", "正在运行 — 日志实时滚动"),
            LogsConfidence => ("Confidence {percent}%", "置信度 {percent}%"),
            LogsSuggestedFix => ("Suggested fix", "建议动作"),
            LogsSummaryLabel => ("One-line summary", "一行摘要"),
            LogsCopySummary => ("Copy summary", "复制摘要"),
            LogsEmptyTitle => ("No output yet", "还没有输出"),
            LogsEmptyBody => (
                "Logs stream here while the launcher or the game is running. Pick an instance and start it to see the first lines.",
                "启动器或游戏运行时，日志会实时出现在这里。选择一个实例并启动，就能看到第一批日志。"
            ),
            LogsEmptyAction => ("Choose an instance", "选择实例"),
            // --- 启动失败弹窗（M187）---
            CrashDialogTitleFailed => ("Launch failed", "启动失败"),
            CrashDialogTitleCrashed => ("The game crashed", "游戏崩溃"),
            CrashDialogExitCode => ("Exit code {code}", "退出码 {code}"),
            CrashDialogStartedThenDied => ("The game process started, then exited abnormally.", "游戏进程起来了，随后异常退出。"),
            CrashDialogSilentFailure => ("The game exited with code 0 but never really started.", "游戏以退出码 0 结束，但并没有真正起来。"),
            CrashDialogStageReason => ("It failed before the game could start: {stage}", "游戏还没起来就失败了：{stage}"),
            CrashDialogCrashReportFound => ("The game wrote a crash report — the details are below.", "游戏写出了崩溃报告，详见下方说明。"),
            CrashDialogNoGameLog => ("No crash report this time: the game stopped before it could write one. The last lines the launcher captured are below.", "这次没有生成崩溃报告：游戏还没来得及写就停了。下面是启动器捕获的最后若干行。"),
            CrashDialogNoLogAtAll => ("No crash report and no captured output. Most likely: missing Java, not enough memory, wrong launch arguments, or missing libraries.", "既没有崩溃报告，也没有捕获到输出。最可能的原因：缺少 Java、内存不足、启动参数错误、或库文件缺失。"),
            CrashDialogShowDetail => ("Show crash details", "展开崩溃说明"),
            CrashDialogHideDetail => ("Hide crash details", "收起崩溃说明"),
            CrashDialogLauncherLogs => ("Launcher logs", "启动器日志"),
            CrashDialogSessionLog => ("This session", "本次会话"),
            CrashDialogGameLog => ("Game latest.log", "游戏 latest.log"),
            CrashDialogCrashReports => ("Crash reports", "崩溃报告目录"),
            CrashDialogCapturedTail => ("CAPTURED OUTPUT (last {count} lines)", "捕获输出（最后 {count} 行）"),
            CrashDialogExport => ("Export report bundle", "导出诊断包"),
            CrashDialogOpenFolder => ("Open log folder", "打开日志文件夹"),
            CrashDialogCopyPaths => ("Copy file paths", "复制文件路径"),
            CrashDialogClose => ("Close", "关闭"),
            CrashDialogCopied => ("File paths copied to the clipboard", "文件路径已复制到剪贴板"),
            CrashDialogExportDone => ("Exported {bytes} bytes to {path}", "已导出 {bytes} 字节到 {path}"),
            CrashDialogExportFailed => ("Export failed: {error}", "导出失败：{error}"),
            CrashDialogExportTruncated => ("{count} file(s) were too large and are truncated (head + tail kept).", "有 {count} 个文件太大，已截断（保留头 + 尾）。"),
            CrashDialogOpenFailed => ("Could not open the folder: {error}", "打不开文件夹：{error}"),
        } }
    };
}

pub(crate) use logs_entries;
