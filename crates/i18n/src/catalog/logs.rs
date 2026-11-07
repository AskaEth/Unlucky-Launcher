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
        } }
    };
}

pub(crate) use logs_entries;
