//! UI 偏好/路由枚举的双向映射（Slint 枚举 ↔ 状态层枚举）。

use crate::dto::{self, VersionCategory};
use crate::prefs;
use crate::router;
use crate::{BgFillKind, DownloadCategory, InstanceView, RouteKind, ThemeMode, TileSize};

/// 路由 → 导航高亮/页面选择。
pub fn route(route: &router::Route) -> RouteKind {
    match route {
        router::Route::Instances => RouteKind::Instances,
        router::Route::Download => RouteKind::Download,
        router::Route::Logs => RouteKind::Logs,
        router::Route::Settings => RouteKind::Settings,
        router::Route::Detail(_) => RouteKind::Detail,
        router::Route::Agent => RouteKind::Agent,
    }
}

/// Slint 侧导航点击 → 路由（详情页保持当前实例，不在导航里触发）。
pub fn route_from(kind: RouteKind) -> Option<router::Route> {
    match kind {
        RouteKind::Instances => Some(router::Route::Instances),
        RouteKind::Download => Some(router::Route::Download),
        RouteKind::Logs => Some(router::Route::Logs),
        RouteKind::Settings => Some(router::Route::Settings),
        RouteKind::Agent => Some(router::Route::Agent),
        RouteKind::Detail => None,
    }
}

/// 磁贴尺寸。
pub fn tile_size(size: prefs::TileSize) -> TileSize {
    match size {
        prefs::TileSize::Small => TileSize::Small,
        prefs::TileSize::Medium => TileSize::Medium,
        prefs::TileSize::Large => TileSize::Large,
    }
}

/// 视图模式。
pub fn instance_view(view: prefs::InstanceView) -> InstanceView {
    match view {
        prefs::InstanceView::Tiles => InstanceView::Tiles,
        prefs::InstanceView::List => InstanceView::List,
    }
}

/// 主题三态。
pub fn theme_mode(mode: prefs::ThemeMode) -> ThemeMode {
    match mode {
        prefs::ThemeMode::Auto => ThemeMode::Auto,
        prefs::ThemeMode::Light => ThemeMode::Light,
        prefs::ThemeMode::Dark => ThemeMode::Dark,
    }
}

/// Slint 主题三态 → 偏好枚举。
pub fn theme_mode_from(mode: ThemeMode) -> prefs::ThemeMode {
    match mode {
        ThemeMode::Auto => prefs::ThemeMode::Auto,
        ThemeMode::Light => prefs::ThemeMode::Light,
        ThemeMode::Dark => prefs::ThemeMode::Dark,
    }
}

/// Slint 磁贴尺寸 → 偏好枚举。
pub fn tile_size_from(size: TileSize) -> prefs::TileSize {
    match size {
        TileSize::Small => prefs::TileSize::Small,
        TileSize::Medium => prefs::TileSize::Medium,
        TileSize::Large => prefs::TileSize::Large,
    }
}

/// Slint 视图模式 → 偏好枚举。
pub fn instance_view_from(view: InstanceView) -> prefs::InstanceView {
    match view {
        InstanceView::Tiles => prefs::InstanceView::Tiles,
        InstanceView::List => prefs::InstanceView::List,
    }
}

/// 下载分类 → core 侧分类枚举。
pub fn version_category(category: DownloadCategory) -> VersionCategory {
    match category {
        DownloadCategory::Release => VersionCategory::Release,
        DownloadCategory::Snapshot => VersionCategory::Snapshot,
        DownloadCategory::AncientClassic => VersionCategory::Ancient(dto::AncientKind::Classic),
        DownloadCategory::AncientAlpha => VersionCategory::Ancient(dto::AncientKind::OldAlpha),
        DownloadCategory::AncientBeta => VersionCategory::Ancient(dto::AncientKind::OldBeta),
        DownloadCategory::AncientUnlisted => VersionCategory::Ancient(dto::AncientKind::Unlisted),
        DownloadCategory::Modpack => VersionCategory::Modpack,
        DownloadCategory::Mod => VersionCategory::Mod,
    }
}

/// 下载分类（偏好 → Slint）——供启动时回填。
pub fn category_from(category: VersionCategory) -> DownloadCategory {
    match category {
        VersionCategory::Release => DownloadCategory::Release,
        VersionCategory::Snapshot => DownloadCategory::Snapshot,
        VersionCategory::Ancient(dto::AncientKind::Classic) => DownloadCategory::AncientClassic,
        VersionCategory::Ancient(dto::AncientKind::OldAlpha) => DownloadCategory::AncientAlpha,
        VersionCategory::Ancient(dto::AncientKind::OldBeta) => DownloadCategory::AncientBeta,
        VersionCategory::Ancient(dto::AncientKind::Unlisted) => DownloadCategory::AncientUnlisted,
        VersionCategory::Modpack => DownloadCategory::Modpack,
        VersionCategory::Mod => DownloadCategory::Mod,
    }
}

/// 背景图填充模式（core 侧枚举 → Slint）。
pub fn bg_fill(fill: unlucky_core::setting::appearance::BgFill) -> BgFillKind {
    match fill {
        unlucky_core::setting::appearance::BgFill::Cover => BgFillKind::Cover,
        unlucky_core::setting::appearance::BgFill::Contain => BgFillKind::Contain,
        unlucky_core::setting::appearance::BgFill::Tile => BgFillKind::Tile,
    }
}

/// Slint 填充模式 → core 侧枚举。
pub fn bg_fill_from(fill: BgFillKind) -> unlucky_core::setting::appearance::BgFill {
    match fill {
        BgFillKind::Cover => unlucky_core::setting::appearance::BgFill::Cover,
        BgFillKind::Contain => unlucky_core::setting::appearance::BgFill::Contain,
        BgFillKind::Tile => unlucky_core::setting::appearance::BgFill::Tile,
    }
}

/// 强调色十六进制串 → Slint 颜色；空串/非法值给透明（= 用出厂 accent）。
pub fn accent_color(hex: &str) -> slint::Color {
    match unlucky_core::setting::appearance::hex_to_rgb(hex) {
        Some((r, g, b)) => slint::Color::from_rgb_u8(r, g, b),
        None => slint::Color::from_argb_u8(0, 0, 0, 0),
    }
}
