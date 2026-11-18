//! 演示模型（M357 r2 从 `pages/mod.rs` **整块搬出**：那边要守 400 行硬线）。
//!
//! 这是 `pages_demo` example 与页面单测的输入；产品路径走 `app/view.rs` 的 `AppState` 转换。
//! 搬移**一个字没改**，调用点仍是 `pages::demo_model`（`mod.rs` 里 `pub use`）。

use super::model::Model;
use super::screen::Screen;
use super::{content, download, instances, logs, update};

/// 演示模型（`pages_demo` example 与单测用；真页面走 `app/view.rs` 的 `AppState` 转换）。
///
/// 实例 / 下载 / 日志三页带演示数据，设置页仍是占位视觉。
///
/// 高度也是模型的一部分（M161）：内容库在矮窗口下要换版面，出图时 `--height` 必须
/// 真的传进来，否则「矮窗口那几张图」画的是高窗口的版面。
pub fn demo_model(screen: Screen, window_width: f32, window_height: f32) -> Model {
    let mut model = Model::new(screen, window_width, window_height, Some(screen.nav()));
    // 出图的中英取证（M222）：`ULL_DL_LANG=zh` 时整页走中文目录。
    //
    // 为什么需要这一行：页面文案来自 `model.lang`，而 `Model::new` 的 `lang` 写死 `En`、
    // `demo_model` 之前也不碰它——所以 `LC_ALL=zh_CN.UTF-8` 对出图**完全无效**
    // （`i18n::detect_env` 不在这条链上，Windows 上平台源还会压过 env）。M353 起它同时喂设置页
    // （`Language::fixture(model.lang)`）与导航轨，M355 起还喂实例页的元信息卡标签；产品路径走 `app/view.rs`。
    let lang = match std::env::var("ULL_DL_LANG").ok().as_deref().map(str::trim) {
        Some("zh" | "zh_cn" | "zh-CN" | "zh_CN") => unlucky_i18n::Lang::ZhCn,
        _ => unlucky_i18n::Lang::En,
    };
    model.lang = lang;
    match screen {
        Screen::Instances => model.instances = instances::demo(lang),
        Screen::Download => model.download = download::demo(lang.catalog()),
        Screen::Content => model.content = content::demo(content::ContentKind::Mod, model.lang),
        Screen::Logs => model.logs = logs::demo(model.lang),
        // M159：设置页「更新」组的出图夹具（一个可装的新版本 + 一次成功检查）。
        // 只给这一页：其它页的基线图（尤其实例页的启动参数区）不该因为更新提示条而整体下移。
        Screen::Settings => model.update = update::demo(),
    }
    // M357：`ULL_UPDATE_BANNER=1` ⇒ 每一页都带提示条（P1-1 的取证口，见 `update::demo_forced`）。
    model.update = update::demo_forced().unwrap_or(model.update);
    model
}
