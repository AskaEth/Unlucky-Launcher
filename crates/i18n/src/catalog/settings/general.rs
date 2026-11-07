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
        } }
    };
}

pub(crate) use settings_general_entries;
