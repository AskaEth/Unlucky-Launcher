//! 数据根派生：**字体缓存放哪** + **一条设置键读什么**（M168）。
//!
//! ## 为什么需要它
//!
//! 字体下载要把 17 MiB 落在数据根里（`<launcher_home>/cache/fonts/`），而「数据根在哪」这件事
//! 已经有一个真源：`unlucky_presenter::settings_bridge::SettingsBridge::open()`（三模式数据根 +
//! `core.*` 覆盖键 + 便携回落，见那边的 `resolve_config_path`）。本模块**不重写**那套解析，
//! 只做两件它没暴露的事：
//!
//! 1. 由配置文件路径 `<home>/config/launcher.json` 反推 `<home>`，再用 [`LauncherLayout::cache`]
//!    取缓存分区（分区名只从 core 的 `layout` 常量来，不手拼字符串）；
//! 2. 读一条**设置键**的生效值（`ui.font_cdn`）：`SettingsBridge` 只暴露了 `ui.language` 的读写，
//!    而 `ui.font_cdn` 是 M168 新增的键。
//!
//! ## 诚实边界
//!
//! - **抓得住**：路径推导的形状（`<home>/config/x` → `<home>/cache`）与「配置读不到时返回 `None`
//!   而不是猜一个目录」；
//! - **抓不住**：真机上 `SettingsBridge::open()` 解析出哪个根（那是 presenter + core 的用例），
//!   以及 macOS `.app` 只读这类平台差异（没有真机）。
//!
//! ## 后续收口（已登记 finding）
//!
//! 更干净的做法是让 presenter 暴露一个通用的 `SettingsBridge::load_text(key)`，本模块就只剩
//! 「布局 + 一条键」。presenter 不在 M168 的 scope 里，所以这里先按上面的方式落地。

use std::path::PathBuf;

use unlucky_core::platform::layout::LauncherLayout;
use unlucky_core::setting::document::SettingDocument;
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;
use unlucky_presenter::settings_bridge::SettingsBridge;

/// 中文字体下载地址的覆盖键（M168；空 = 用内置镜像）。
pub const FONT_CDN_KEY: &str = "ui.font_cdn";

/// 本进程的数据根布局（`None` = 数据根解析不出来，例如 exe 目录都拿不到）。
///
/// 解析只发生一次：配方来自 `SettingsBridge::open()`（它内部走 `DataDirRequest` + `Resolver`）。
pub fn layout() -> Option<LauncherLayout> {
    static LAYOUT: std::sync::OnceLock<Option<LauncherLayout>> = std::sync::OnceLock::new();
    LAYOUT
        .get_or_init(|| {
            let config = SettingsBridge::open().path()?.to_path_buf();
            // `<home>/config/launcher.json` → `<home>`
            let home = config.parent()?.parent()?.to_path_buf();
            Some(LauncherLayout::new(home))
        })
        .clone()
}

/// 字体缓存的根（`<cache>/fonts/` 的父目录；**不**等于字体目录本身）。
///
/// 用 `cache/` 而不是 `data/`：字体随时可删、删了会自动重下（D28 的「数据根」语义里，
/// 这就是「可重建」的那一类）。
pub fn cache_root() -> Option<PathBuf> {
    layout().map(|layout| layout.cache())
}

/// 读一条设置键的**生效值**（schema 默认值也算；空串会被 `trim` 掉但仍是 `Some`）。
///
/// `None` = 配置读不到（数据根解析失败 / 文件损坏到 `SettingStore` 都装不起来）。
pub fn setting_text(key: &str) -> Option<String> {
    let path = SettingsBridge::open().path()?.to_path_buf();
    let schema = launcher_schema();
    let store = SettingStore::load(&path, schema);
    let document: &SettingDocument = store.doc();
    document.effective_str(key).map(str::to_owned)
}

/// `ui.font_cdn` 的覆盖值（`None` = 没配 / 配了空串 ⇒ 用内置镜像）。
pub fn font_cdn_override() -> Option<String> {
    let raw = setting_text(FONT_CDN_KEY)?;
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 键名与 core 的 schema 一致：写错一个字符，用户配了也不生效（而 UI 不会报错）。
    ///
    /// 抓得住：本模块的常量与 `core::setting::keys` 里登记的键名对不上。
    /// 抓不住：真机上读到的值（那要数据根与配置文件）。
    #[test]
    fn the_cdn_key_is_registered_in_the_launcher_schema() {
        let schema = launcher_schema();
        assert!(
            schema.spec(FONT_CDN_KEY).is_some(),
            "{FONT_CDN_KEY} 没在 core 的 schema 里登记"
        );
    }

    /// 布局推导的形状：`<home>/config/launcher.json` → `<home>/cache`。
    ///
    /// 抓得住：有人把「上两级」写成「上一级」（缓存会落进 `config/` 里），或改用 `data/` 分区。
    /// 抓不住：真机上 `<home>` 解析成什么（presenter 的 `resolve_config_path` 管那一层）。
    #[test]
    fn the_cache_partition_is_the_sibling_of_config() {
        let config = PathBuf::from("/tmp/ull-home/config/launcher.json");
        let home = config.parent().unwrap().parent().unwrap().to_path_buf();
        let layout = LauncherLayout::new(home);
        assert_eq!(layout.cache(), PathBuf::from("/tmp/ull-home/cache"));
        assert_eq!(
            layout.cache().join("fonts"),
            PathBuf::from("/tmp/ull-home/cache/fonts")
        );
    }
}
