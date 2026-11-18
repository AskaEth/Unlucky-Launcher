//! `prefs` 的单测（拆文件守 400 行上限；结构体与 re-export 一律留在 `prefs.rs`）。

use super::{
    keys, AppearanceBase, InstanceView, PaletteSource, ThemeMode, TileSize, TitlebarMode, UiPrefs,
    WindowCorner, WINDOW_ALPHA_MIN,
};

#[test]
fn defaults_match_d16() {
    let prefs = UiPrefs::default();
    assert_eq!(prefs.tile_size, TileSize::Medium);
    assert_eq!(prefs.tile_size.columns(), 4);
    assert_eq!(prefs.instance_view, InstanceView::Tiles);
    assert_eq!(prefs.theme, ThemeMode::Auto);
    assert_eq!(prefs.theme.as_str(), "auto");
    assert!(!prefs.nav_collapsed);
    assert!(!prefs.ai_collapsed);
}

#[test]
fn tile_size_columns_follow_q24() {
    assert_eq!(TileSize::Small.columns(), 6);
    assert_eq!(TileSize::Medium.columns(), 4);
    assert_eq!(TileSize::Large.columns(), 3);
}

#[test]
fn theme_mode_round_trips_through_key_values() {
    for mode in [ThemeMode::Auto, ThemeMode::Light, ThemeMode::Dark] {
        assert_eq!(ThemeMode::from_key_value(mode.as_str()), mode);
    }
    // 坏值兜底
    assert_eq!(ThemeMode::from_key_value("solarized"), ThemeMode::Auto);
}

#[test]
fn pref_keys_are_unique() {
    let mut keys = keys::ALL.to_vec();
    let total = keys.len();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), total);
}

fn base_of(raw: serde_json::Value) -> AppearanceBase {
    let schema = unlucky_core::setting::keys::launcher_schema();
    let (doc, _) = unlucky_core::setting::document::SettingDocument::load(&schema, &raw);
    AppearanceBase::from_document(&doc)
}

#[test]
fn appearance_base_missing_keys_use_factory() {
    let base = base_of(serde_json::json!({}));
    assert_eq!(
        base,
        AppearanceBase::factory(),
        "缺键 → 出厂态（不透明 / auto / preset）"
    );
    assert!(!base.is_transparent(), "出厂不透明 → 不下发窗口 α");
}

#[test]
fn appearance_base_clamps_out_of_range_opacity() {
    for raw in [0.0, 0.02, -1.0] {
        let base = base_of(serde_json::json!({ "appearance": { "window_opacity": raw } }));
        assert_eq!(
            base.window_opacity, WINDOW_ALPHA_MIN as f32,
            "raw={raw} 应夹到下限"
        );
    }
    for (raw, want) in [(0.6, 0.6), (3.0, 1.0)] {
        let base = base_of(serde_json::json!({ "appearance": { "window_opacity": raw } }));
        assert_eq!(base.window_opacity, want, "raw={raw}");
    }
}

#[test]
fn appearance_base_illegal_enums_fall_back_and_legal_ones_read_through() {
    let bad = base_of(serde_json::json!({
        "appearance": { "titlebar_mode": "allways", "palette_source": "desktop" }
    }));
    assert_eq!(bad.titlebar_mode, TitlebarMode::Auto, "非法 → auto");
    assert_eq!(bad.palette_source, PaletteSource::Preset, "非法 → preset");
    // 合法值必须照读：否则「回落」测试会因为实现恒回默认而假绿。
    let good = base_of(serde_json::json!({
        "appearance": { "titlebar_mode": "never", "palette_source": "wallpaper" }
    }));
    assert_eq!(good.titlebar_mode, TitlebarMode::Never);
    assert_eq!(good.palette_source, PaletteSource::Wallpaper);
}

/// **M116**：`appearance.window_corner` 的读侧——缺键 = 跟随系统、写 `square` 读回 `square`。
///
/// 抓得住：`AppearanceBase::from_document` 漏搬这一格（那会让设置页永远显示「跟随系统」，
/// 用户点了直角重启后又变回去）。
/// 抓不住：这台机器能不能真圆（`app-iced` 的 `platform::plan_corner` 管）。
#[test]
fn window_corner_round_trips_through_the_document() {
    assert_eq!(
        base_of(serde_json::json!({})).window_corner,
        WindowCorner::System,
        "缺键 → 出厂跟随系统"
    );
    let square = base_of(serde_json::json!({ "appearance": { "window_corner": "square" } }));
    assert_eq!(square.window_corner, WindowCorner::Square);
    // 坏值：core 侧落到 system（并留诊断），这里只钉 UI 侧看到的值。
    let broken = base_of(serde_json::json!({ "appearance": { "window_corner": "rounded" } }));
    assert_eq!(broken.window_corner, WindowCorner::System);
}
