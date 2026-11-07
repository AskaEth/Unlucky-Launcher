//! 启动期的取证环境变量解析（`ULL_START_ROUTE`）。
//!
//! 从 `app/update.rs` 原样搬出（那个文件在 M105/M106/M109/M113 之后贴着 400 行硬线）：
//! **纯函数、无状态**，只是取值表长到了七条。

use unlucky_presenter::router::Route;

/// 取证开关 `ULL_START_ROUTE` 的取值解析（`settings` / `instances` / `download` / `content` / `logs`）。
///
/// 为什么是环境变量：`crates/app-iced/src/cli.rs` 不在 M93 的 scope，而外观矩阵必须在**设置页**上
/// 出图。语义与 `ULL_APPEARANCE_*` / `ULL_TICK_TRACE` 一致：只影响这一次进程。
pub(super) fn env_start_route() -> Option<Route> {
    match std::env::var("ULL_START_ROUTE").ok()?.trim() {
        "instances" => Some(Route::Instances),
        "download" => Some(Route::Download),
        "logs" => Some(Route::Logs),
        "settings" => Some(Route::Settings),
        // M104：AI 独立页也要能直达（三态矩阵的 Detached 那张）
        "agent" => Some(Route::Agent),
        // M109：内容库（截图矩阵要能直达；页面内的四条侧栏不在路由里）
        "content" => Some(Route::Content),
        // M114：账户页的 D30 矩阵要能直达（`cli.rs` 不在本 mission 的 scope）
        "accounts" => Some(Route::Accounts),
        _ => None,
    }
}
