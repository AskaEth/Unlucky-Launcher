//! 设置页 General 分组**真控件**的文案（M106 `I18NFOLLOW`）：语言选择器 + 其余三行。
//!
//! ## 与 `catalog/settings/general.rs` 的分工
//!
//! `catalog/settings/general.rs`（M99 建的）只有两条**标题**（[`SettingsLanguage`] /
//! [`SettingsLanguageHint`]），是设置页五域装配的一部分；本文件是 M106 接真控件时新增的
//! **选择器文案**：三档选项名、选中后的「界面当前使用」回显，以及 General 分组里另外三行
//! （主题 / 启动页 / 关闭按钮）。
//!
//! 为什么要分两个文件：`catalog.rs` 的装配链是**追加**式的，新域挂在链尾就不会动到既有
//! `Key` 的顺序（= `catalog/golden.rs` 那张逐字节金表）。两个文件同属 General 分组，读代码时
//! 按「标题 vs 选项」区分即可；真要合并也是一个文件搬十行的活，不急着在接线的 mission 里做。
//!
//! ## 选项名为什么不是「语言自称」
//!
//! 常见做法是三档都写 `简体中文` / `English` 这样的自称。这里没有：`catalog/tests.rs` 有一条
//! `chinese_catalog_is_actually_chinese`（英文表与中文表逐 key 必须不同），自称会让「中文表里
//! 写着 English」被判成漏译。所以选项名**跟着界面语言走**：中文界面「跟随系统 / 简体中文 / 英文」，
//! 英文界面 "Follow system / Chinese (Simplified) / English"——这也是多数启动器的做法。
//!
//! 域文件的形状见 [`crate::catalog`] 的「域装配」一节。

macro_rules! general_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // ── General 分组（语言选择器 + 其余三行）
            SettingsLanguageFollowSystem => ("Follow system", "跟随系统"),
            SettingsLanguageZhCn => ("Chinese (Simplified)", "简体中文"),
            SettingsLanguageEn => ("English", "英文"),
            SettingsLanguageEffective => ("Interface right now: {language}", "界面当前使用：{language}"),
            SettingsTheme => ("Theme", "主题"),
            SettingsThemeFollowSystem => ("Follow system", "跟随系统"),
            // M229：主题三态改成「Ink / Silver / 跟随系统」——前两档就是两块品牌色板
            // （Ink = 深色主方案 / Silver = 浅色主方案），落盘仍是 `appearance.theme_mode` 的
            // `dark` / `light` / `auto` 三个值，不新增配置键。
            SettingsThemeInk => ("Ink", "Ink（深色）"),
            SettingsThemeSilver => ("Silver", "Silver（浅色）"),
            SettingsThemeHint => (
                "Ink is the dark scheme, Silver the light one. Follow system picks between them as Windows switches.",
                "Ink 是深色主方案，Silver 是浅色主方案；跟随系统会随 Windows 的深浅在这两块之间切换。"
            ),
            SettingsStartupPage => ("Startup page", "启动页"),
            SettingsStartupInstances => ("Instances", "实例"),
            SettingsCloseButton => ("Close button", "关闭按钮"),
            SettingsCloseToTray => ("Exit to tray", "退出到托盘"),
            // 设置页页头（M106）：标题复用 `NavSettings`，这三条是副标题与两个动作。
            SettingsSubtitle => ("Launcher behaviour, Java runtimes and download sources", "启动器行为、Java 运行时与下载源"),
            SettingsResetSection => ("Reset section", "重置本组"),
            SettingsSave => ("Save", "保存"),
            // 第五条导航项（M106 给导航轨补的最后一条）。放在本域而不是 `catalog/nav.rs`：
            // 金表按**位置**逐字节比，插进 nav 域会把既有 4 条导航行整体平移一位。
            NavAi => ("Assistant", "助手"),
            // 设置页左列的分组名（M106）。`Group.name` 是**id**（测试按它认组），显示名走这里。
            SettingsGroupGeneral => ("General", "通用"),
            SettingsGroupAppearance => ("Appearance", "外观"),
            SettingsGroupJava => ("Java", "Java 运行时"),
            SettingsGroupDownloads => ("Downloads", "下载"),
            // `Route::Agent` 的回退卡正文（M106：`app/view.rs` 里最后一条硬编码英文）
            AgentFallbackBody => ("The assistant page is not built yet. Chat lives in the docked panel once the AI mission lands; this screen is the M68 skeleton fallback.", "助手页还没做。AI 接线落地后对话会出现在右侧停靠面板；这一屏是 M68 的骨架回退。"),
        } }
    };
}

pub(super) use general_entries;
