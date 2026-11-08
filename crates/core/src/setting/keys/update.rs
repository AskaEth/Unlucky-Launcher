//! 自更新两键（M161：从更新域自己的 `cache/update/prefs.json` 迁进主配置）。
//!
//! 从 `keys.rs` 拆出（那边贴着 400 行硬线）：这两个键连同它们的允许值与理由一起搬，
//! 主表只留一行 `.with(update::register(..))`。
//!
//! 为什么必须进 schema：`ull config get update.auto_check` 读的是**这一份**文件；域内文件对
//! CLI 不可见（用户在 GUI 里关掉自动检查，CLI 永远报默认值 `true`）。存量迁移（幂等 + 收据）
//! 在 [`crate::setting::update_prefs`]。

use crate::setting::spec::{AiPolicy, SettingSpec};

/// `update.channel` 允许值。
///
/// D50：先只发 stable。**加值必须与宿主层同步**——`presenter::host::update::Runtime` 现在真的
/// 读这个键（`UpdateView::channel` 与检查用的渠道同源），但 core 的 `Job::check/apply` 仍按
/// 传入的渠道准入，所以只有清单里真有那个渠道的发布时才该加值。现在写死一个值：用户
/// `config set update.channel beta` 会被 schema **当面拒绝**，而不是写进文件后被静默忽略
/// （那是更坏的谎）。
pub const UPDATE_CHANNELS: &[&str] = &["stable"];

/// 两个自更新键的完整登记项（主表一行 `.with_all(specs())`）。
#[must_use]
pub fn specs() -> Vec<SettingSpec> {
    vec![
        SettingSpec::bool(
            "update.auto_check",
            true,
            AiPolicy::audited(),
            "启动时静默检查更新（关掉之后启动一次网络都不发；发现新版本才弹提示条）",
        ),
        SettingSpec::enumeration(
            "update.channel",
            "stable",
            UPDATE_CHANNELS,
            AiPolicy::audited(),
            "更新渠道（D50 先只发 stable；清单里没有的渠道不许写进文件）",
        ),
    ]
}
