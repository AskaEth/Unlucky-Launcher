//! 外观三键（整窗透明度 / 标题栏三态 / 配色来源）的读侧契约。
//!
//! 与 `mod.rs` 拆开只为单文件 ≤400 行的纪律（D36/D37/D38 的键仍是
//! `setting::appearance::*`），测试属于同一个模块。

use serde_json::json;

use super::{
    diagnostics, Appearance, PaletteSource, TitlebarMode, WindowCorner, PALETTE_SOURCES,
    TITLEBAR_MODES, WINDOW_ALPHA_MIN, WINDOW_CORNERS,
};
use crate::setting::document::SettingDocument;
use crate::setting::keys::launcher_schema;
use crate::setting::schema::{SchemaId, SchemaRef, SchemaVersion};
use crate::setting::store::SettingStore;

fn doc(raw: serde_json::Value) -> SettingDocument {
    SettingDocument::load(&launcher_schema(), &raw).0
}

#[test]
fn missing_keys_fall_back_to_factory_values() {
    let document = doc(json!({}));
    let appearance = Appearance::read(&document);
    assert_eq!(appearance.window_opacity, 1.0, "D36：出厂完全不透明");
    assert_eq!(
        appearance.titlebar_mode,
        TitlebarMode::Auto,
        "D37：默认 auto"
    );
    assert_eq!(
        appearance.palette_source,
        PaletteSource::Preset,
        "D38：默认内置色卡（莫奈取色关闭的出厂档）"
    );
    assert!(
        diagnostics(&document).is_empty(),
        "缺键 = 取默认，不该产生回落诊断"
    );
}

#[test]
fn window_opacity_is_clamped_to_the_d36_floor() {
    let cases = [
        (0.0, WINDOW_ALPHA_MIN),
        (0.02, WINDOW_ALPHA_MIN),
        (WINDOW_ALPHA_MIN, WINDOW_ALPHA_MIN),
        (0.5, 0.5),
        (1.0, 1.0),
        (4.0, 1.0),
    ];
    for (raw, want) in cases {
        let appearance = Appearance::read(&doc(json!({ "appearance": { "window_opacity": raw } })));
        assert!(
            (appearance.window_opacity - want).abs() < 1e-9,
            "window_opacity={raw} 应读出 {want}，实际 {}",
            appearance.window_opacity
        );
    }
}

#[test]
fn out_of_range_opacity_is_clamped_not_rejected() {
    // 手改文件绕过 schema：load 只记一条 problem（不失败、不 panic），读侧再夹一次。
    let (document, problems) = SettingDocument::load(
        &launcher_schema(),
        &json!({ "appearance": { "window_opacity": 0.02 } }),
    );
    assert!(
        !problems.is_empty(),
        "越界值必须留下痕迹（LoadProblem），否则「钳位」变成静默改用户的数"
    );
    assert!((Appearance::read(&document).window_opacity - WINDOW_ALPHA_MIN).abs() < 1e-9);

    // 类型错（字符串）也不算错误：回落默认 1.0。
    let appearance = Appearance::read(&doc(json!({ "appearance": { "window_opacity": "half" } })));
    assert_eq!(appearance.window_opacity, 1.0);
}

#[test]
fn illegal_enum_falls_back_and_leaves_a_readable_diagnostic() {
    let document = doc(json!({
        "appearance": { "titlebar_mode": "allways", "palette_source": "desktop" }
    }));
    let appearance = Appearance::read(&document);
    assert_eq!(appearance.titlebar_mode, TitlebarMode::Auto);
    assert_eq!(appearance.palette_source, PaletteSource::Preset);

    let found = diagnostics(&document);
    assert_eq!(found.len(), 2, "两个非法枚举各一条诊断：{found:?}");
    assert_eq!(found[0].key, "appearance.titlebar_mode");
    assert_eq!(found[0].raw, "allways");
    assert_eq!(found[0].fallback, "auto");
    assert!(
        found[0].message.contains("allways") && found[0].message.contains("auto"),
        "诊断要同时说清「读到了什么」和「生效的是什么」：{}",
        found[0].message
    );
    assert_eq!(found[1].key, "appearance.palette_source");
    assert_eq!(found[1].raw, "desktop");
    assert_eq!(found[1].fallback, "preset");
}

#[test]
fn value_tables_are_self_consistent() {
    assert_eq!(TITLEBAR_MODES, ["auto", "always", "never"], "D37 三态");
    // 这里只钉 **core 自己的字面量**（改这三个字符串要过这一关）。
    // 与 `unlucky_design::palette::Level::code()` 的一致性 core 守不了——core 不依赖 design
    // （分层纪律），本 crate 的测试永远看不到 design 的代码。那条一致性由
    // `crates/app-iced/src/platform/mod.rs::palette_source_codes_match_design_level_codes`
    // 守（那个 crate 同时依赖 design 与 presenter，是唯一能对照的地方）。
    assert_eq!(
        PALETTE_SOURCES,
        ["background-image", "wallpaper", "preset"],
        "D38 三级（与 design 的 Level::code() 同名）"
    );
    for source in [
        PaletteSource::BackgroundImage,
        PaletteSource::Wallpaper,
        PaletteSource::Preset,
    ] {
        assert_eq!(PaletteSource::from_key_value(source.as_str()), source);
        assert!(PALETTE_SOURCES.contains(&source.as_str()));
    }
    for mode in [
        TitlebarMode::Auto,
        TitlebarMode::Always,
        TitlebarMode::Never,
    ] {
        assert_eq!(TitlebarMode::from_key_value(mode.as_str()), mode);
        assert!(TITLEBAR_MODES.contains(&mode.as_str()));
    }
}

#[test]
fn older_document_loads_and_write_back_keeps_existing_keys() {
    let temp = std::env::temp_dir().join(format!(
        "ull-appearance-base-{}-{}",
        std::process::id(),
        line!()
    ));
    std::fs::create_dir_all(temp.join("config")).expect("临时目录");
    let path = temp.join("config/launcher.json");
    let _ = std::fs::remove_file(&path);

    // 1.3.0 的老文件：没有任何新键，但已有外观键要原样保住。
    let old = json!({
        "$schema": SchemaRef::new(SchemaId::LAUNCHER, SchemaVersion::new(1, 3, 0)).url(),
        "appearance": { "theme_mode": "dark", "panel_alpha": 0.8, "glass": true }
    });
    std::fs::write(&path, serde_json::to_vec_pretty(&old).expect("序列化")).expect("写老文件");

    let mut store = SettingStore::load(&path, launcher_schema());
    let before = Appearance::read(store.doc());
    assert_eq!(before.theme_mode, "dark", "老文件的已有键照读");
    assert!((before.panel_alpha - 0.8).abs() < 1e-9);
    assert!(before.glass);

    let mut appearance = before;
    appearance.window_opacity = 0.9;
    appearance.titlebar_mode = TitlebarMode::Always;
    appearance.write(&mut store).expect("写新键");
    store.save().expect("落盘");

    let reloaded = Appearance::from_store(&SettingStore::load(&path, launcher_schema()));
    assert!((reloaded.window_opacity - 0.9).abs() < 1e-9);
    assert_eq!(reloaded.titlebar_mode, TitlebarMode::Always);
    assert_eq!(
        reloaded.palette_source,
        PaletteSource::Preset,
        "未写的键仍是默认"
    );
    assert_eq!(reloaded.theme_mode, "dark", "写回不许丢已有键");
    assert!((reloaded.panel_alpha - 0.8).abs() < 1e-9);
    assert!(reloaded.glass);

    std::fs::remove_dir_all(&temp).ok();
}

/// **M116**：圆角键的三条读侧语义——缺键取默认、合法值往返、坏值落回 `system` 并**留诊断**。
///
/// 抓得住：把 `from_key_value` 的兜底写成 `Square`（那会让「配置写坏了」变成「用户想要直角」）、
/// 或者漏掉 `diagnostics` 里那一条（静默回落）。
/// 抓不住：这台机器能不能真圆（那是 UI 侧 `platform::plan_corner` 的事）。
#[test]
fn window_corner_falls_back_to_follow_system_and_says_so() {
    let appearance = Appearance::read(&doc(json!({})));
    assert_eq!(
        appearance.window_corner,
        WindowCorner::System,
        "缺键 = 出厂 = 跟随系统"
    );
    assert!(
        diagnostics(&doc(json!({}))).is_empty(),
        "缺键不是回落，不该有诊断"
    );

    for corner in [WindowCorner::System, WindowCorner::Square] {
        assert_eq!(WindowCorner::from_key_value(corner.as_str()), corner);
        assert!(WINDOW_CORNERS.contains(&corner.as_str()));
        let read = Appearance::read(&doc(
            json!({ "appearance": { "window_corner": corner.as_str() } }),
        ));
        assert_eq!(read.window_corner, corner);
    }

    // 坏值：不报错、不崩，落到 system，但**必须**留下可读诊断。
    let bad = doc(json!({ "appearance": { "window_corner": "rounded" } }));
    assert_eq!(Appearance::read(&bad).window_corner, WindowCorner::System);
    let notes = diagnostics(&bad);
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert_eq!(notes[0].key, "appearance.window_corner");
    assert_eq!(notes[0].fallback, "system");
    assert!(notes[0].message.contains("rounded"), "{}", notes[0].message);
}
