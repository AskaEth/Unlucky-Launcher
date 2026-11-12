//! 设置页「Appearance」分组**强调色**一族的文案（M250 ACCENTWIRE）。
//!
//! ## 为什么单开一个域文件，而不是写进 `settings/appearance.rs`
//!
//! `settings_appearance_entries` 在装配链**中间**。往那个域后面追加，会把其后所有 key 在
//! `Key::ALL` 里整体平移 ⇒ 金表（逐 index 逐字节）整片红。本域挂在**链尾**
//! （`font_install_entries` 之后），于是既有顺序一个不动，金表只需在 `tail2::ROWS` 末尾追加
//! （M104 / M106 / M107 / M109 / M114 / M159 / M168 都是这条做法）。
//!
//! key 前缀仍叫 `SettingsAppearanceAccent*`：`catalog/settings/appearance.rs` 的译文守卫按
//! **名字前缀**筛 key，改名会让那两条守卫漏掉这一族。
//!
//! ## 三档失败原因为什么分开写
//!
//! `#12`（位数不足）、`#006A5C00`（位数过多）、`zzzzzz`（非十六进制）是三种不同的手误，
//! 而「格式不对」对用户没有行动性。分开写之后错行能直接说「现在 2 位」「有非十六进制字符 z」。
//!
//! 域文件的形状见 [`crate::catalog`] 的「域装配」一节。

macro_rules! settings_accent_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // ── 强调色（M250：设置页 Appearance 组的第一个控件）
            SettingsAppearanceAccent => ("Accent colour", "强调色"),
            SettingsAppearanceAccentPresets => ("Presets", "预设"),
            SettingsAppearanceAccentFactory => ("Factory", "出厂"),
            SettingsAppearanceAccentHint => (
                "Empty = the factory accent from the brand board. A custom accent replaces primary / secondary / tertiary only; neutral surfaces and error colours do not follow it.",
                "空 = 出厂品牌板的强调色。自定义强调色只替换 primary / secondary / tertiary 三族；中性表面与错误色不随强调色变。"
            ),
            SettingsAppearanceAccentShort => (
                "Accent needs 6 hex digits (RRGGBB); got {digits}.",
                "强调色要 6 位十六进制（RRGGBB），现在 {digits} 位。"
            ),
            SettingsAppearanceAccentLong => (
                "Accent has {digits} digits; a colour takes exactly 6.",
                "强调色有 {digits} 位，一个颜色只要 6 位。"
            ),
            SettingsAppearanceAccentDigit => (
                "Accent contains a character that is not a hex digit: {ch}",
                "强调色里有非十六进制字符：{ch}"
            ),
        } }
    };
}

pub(crate) use settings_accent_entries;
