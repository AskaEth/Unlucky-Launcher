//! 设置页「General」分组文案：语言（Theme / Startup page / Close button 的 key 还没建）。
//!
//! 域文件的形状见 [`crate::catalog`] 的「域装配」一节。

macro_rules! settings_general_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // ── 设置
            SettingsLanguage => ("Language", "界面语言"),
            SettingsLanguageHint => (
                "Switches the interface language. Chinese text needs a Chinese font.",
                "切换界面语言。显示中文需要中文字体。"
            ),
            // ── 日志文件夹（M188：用户投诉「找不到崩溃日志」的直接落点）
            SettingsLogsFolder => ("Log folder", "日志文件夹"),
            SettingsLogsFolderHint => (
                "What explains a crash lives here: the launcher's own runtime log (launcher.log, rolled over at 1 MiB with one old copy kept) and one jsonl per game session under logs/game/. Sessions from older versions (data/agent/sessions) are still read, not moved.",
                "解释一次崩溃要看的东西都在这：启动器自己的运行日志（launcher.log，满 1 MiB 滚动一次、只留一份旧档）与每次游戏会话的 jsonl（logs/game/）。旧版本留下的会话（data/agent/sessions）仍然读得出来，不会被搬动。"
            ),
            SettingsLogsCopy => ("Copy path", "复制路径"),
            SettingsLogsUnknown => (
                "Data directory not resolved — restart the launcher from its own folder",
                "数据根没解析出来——从启动器所在目录重新启动试试"
            ),
        } }
    };
}

pub(crate) use settings_general_entries;
