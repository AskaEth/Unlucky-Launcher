//! 设置页「Downloads」分组文案（M201 起有内容：这一组接了真控件）。
//!
//! 加 key 时把条目写进 `$($acc)*` 后面即可（形状见 [`crate::catalog`] 的「域装配」一节）。
//! 本域在装配链里排在 `settings_java_entries` 之后、`settings_ai_entries` 之前，所以金表那几行
//! 也落在 `SettingsJavaReasonInstallRootTooLong` 与 `SettingsGroupAssistant` 之间。
//!
//! ## 只有一条 key 的理由
//!
//! 下载组的**行文案**（`并发数` / `单源尝试次数` / 「原值 9999 已按出厂 8 生效」这些）不在这里：
//! 它们由 presenter 按 schema 键现算（`dto::SettingRowView::label` / `description`），因为那些句子
//! 里带着**生效值**——把数字搬进静态目录只会再长出一份会说谎的文案。这一域只放「页面自己的
//! 按钮词」：提交文本框那一下的「应用」。

macro_rules! settings_download_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // ── 文本行的提交按钮（`pages/settings/download.rs` 的 text 行）
            SettingsDownloadsApply => ("Apply", "应用"),
        } }
    };
}

pub(crate) use settings_download_entries;
