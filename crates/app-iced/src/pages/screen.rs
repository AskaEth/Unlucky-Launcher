//! 页面枚举 `Screen` 与它的三张对照表（版面 / 导航项 / 路由；M101 r2 从 `pages/mod.rs` **原样搬出**）。
//!
//! 为什么拆：合并 M100/M101 之后 `pages/mod.rs` 是 397 行，只剩 3 行余量。纯搬移，路径靠
//! `pages/mod.rs` 的 `pub use screen::Screen;` 保持不变（`pages::Screen`）。

use unlucky_presenter::router::{NavSection, Route};

use super::layout::PaneKit;

/// 四个页面。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// 实例（list-detail）。
    Instances,
    /// **版本管理**（原下载页；单列页，但正文自己切三栏：类型 / 主版本 / 列表）。
    Download,
    /// 内容库（M109）：整合包 / Mod / 资源包 / 光影。
    Content,
    /// 设置（supporting-pane）。
    Settings,
    /// 日志（单列，两行页头）。
    Logs,
}

impl Screen {
    /// 全部页面（截图矩阵按这个顺序铺）。
    pub const ALL: [Screen; 5] = [
        Screen::Instances,
        Screen::Download,
        Screen::Content,
        Screen::Settings,
        Screen::Logs,
    ];

    /// 稳定 slug（命令行参数与文件名用）。
    pub const fn slug(self) -> &'static str {
        match self {
            Screen::Instances => "instances",
            Screen::Download => "download",
            Screen::Content => "content",
            Screen::Settings => "settings",
            Screen::Logs => "logs",
        }
    }

    /// 反向解析（`--screen=instances`）。
    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|screen| screen.slug() == slug)
    }

    /// 页面声明的**最宽**版面；实际用哪个由 [`layout::resolve`](super::layout) 按窗口宽度决定。
    pub const fn kit(self) -> PaneKit {
        match self {
            Screen::Instances => PaneKit::ListDetail,
            Screen::Settings => PaneKit::Supporting,
            // 内容库自带三栏（`pages/content/layout.rs`），版面基座仍是单列：
            // 它的三栏是「页内栏」而不是 layout 模块那三种整页版面。
            Screen::Download | Screen::Content | Screen::Logs => PaneKit::Single,
        }
    }

    /// 路由落在哪一页上（`None` = 这一页还没有实现，由 app 层决定回退）。
    ///
    /// `Route::Detail(_)` 仍属实例页（窄窗时是「整页切换」的那一屏）。
    pub fn of(route: &Route) -> Option<Self> {
        Some(match route {
            Route::Instances | Route::Detail(_) => Screen::Instances,
            Route::Download => Screen::Download,
            Route::Content => Screen::Content,
            Route::Logs => Screen::Logs,
            Route::Settings => Screen::Settings,
            // 账户页（M114）不是本枚举四页之一：它由 `pages::account` 自己的树画
            // （路由在 `app/view.rs` 里先被拦下），harness 的单页矩阵也不含它。
            Route::Accounts => return None,
            Route::Agent => return None,
        })
    }

    /// 该页高亮的导航项（与 `Route::nav` 同源，但这里是「按页问」的口径）。
    pub const fn nav(self) -> NavSection {
        match self {
            Screen::Instances => NavSection::Instances,
            // M109（B1 裁定）：内容库与版本管理共用同一条导航轨项
            Screen::Download | Screen::Content => NavSection::Download,
            Screen::Logs => NavSection::Logs,
            Screen::Settings => NavSection::Settings,
        }
    }
}
