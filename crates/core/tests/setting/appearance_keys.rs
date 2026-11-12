//! 「外观（高级）」键簇（`setting::appearance` + `keys::launcher_schema` 的 appearance.* 段）。
//!
//! 守三条设计约束（09-ui-layout §111）：
//! 1. 一切高级特性**默认关闭**（出厂 Metro 纯色）——默认值即出厂态；
//! 2. 全部走 D6 schema 且 **AI 可写**（白名单内，可被 agent 调节）；
//! 3. 值域有硬边界（滑条夹取），坏值/手改配置不崩（回落默认）。

use serde_json::json;
use unlucky_core::setting::appearance::{
    hex_to_rgb, normalize_hex, relative_luminance, Appearance, BgFill, DEFAULT_ACCENT,
    PANEL_ALPHA_MIN,
};
use unlucky_core::setting::document::SettingDocument;
use unlucky_core::setting::keys::{launcher_schema, launcher_version};
use unlucky_core::setting::spec::ValueKind;

fn doc(raw: serde_json::Value) -> SettingDocument {
    SettingDocument::load(&launcher_schema(), &raw).0
}

#[test]
fn factory_defaults_are_all_advanced_features_off() {
    let appearance = Appearance::from_store(&unlucky_core::setting::store::SettingStore::load(
        std::env::temp_dir().join(format!("ull-appearance-none-{}.json", std::process::id())),
        launcher_schema(),
    ));
    assert_eq!(appearance.theme_mode, "auto", "D17：出厂跟随系统");
    assert!(!appearance.bg_image_enabled, "背景图默认关");
    assert!(!appearance.bg_image_active());
    assert_eq!(appearance.bg_fill, BgFill::Cover);
    assert_eq!(appearance.panel_alpha, 1.0, "出厂完全不透明（Metro 纯色）");
    assert!(!appearance.glass, "玻璃拟态默认关");
    assert_eq!(appearance.accent_color, "", "空串 = 未自定义");
    assert_eq!(appearance.accent_or_default(), DEFAULT_ACCENT);
}

#[test]
fn reading_a_configured_document_reflects_each_key() {
    let document = doc(json!({
        "appearance": {
            "theme_mode": "dark",
            "bg_image_enabled": true,
            "bg_image": "D:/wall/paper.png",
            "bg_fill": "tile",
            "bg_dim": 0.6,
            "panel_alpha": 0.8,
            "glass": true,
            "accent_color": "#ff5722",
        }
    }));
    let appearance = Appearance::read(&document);
    assert_eq!(appearance.theme_mode, "dark");
    assert!(appearance.bg_image_active());
    assert_eq!(appearance.bg_image, "D:/wall/paper.png");
    assert_eq!(appearance.bg_fill, BgFill::Tile);
    assert!((appearance.bg_dim - 0.6).abs() < 1e-9);
    assert!((appearance.panel_alpha - 0.8).abs() < 1e-9);
    assert!(appearance.glass);
    assert_eq!(appearance.accent_color, "#FF5722", "读入即规范化大写");
    assert_eq!(appearance.accent_or_default(), "#FF5722");
}

#[test]
fn bad_values_fall_back_instead_of_panicking() {
    // 手改坏的配置：枚举拼错 / 开关类型错 / accent 不是 #RRGGBB。
    let document = doc(json!({
        "appearance": {
            "bg_fill": "stretch",
            "bg_image_enabled": "yes",
            "accent_color": "rebeccapurple",
        }
    }));
    let appearance = Appearance::read(&document);
    assert_eq!(appearance.bg_fill, BgFill::Cover);
    assert!(!appearance.bg_image_enabled);
    assert_eq!(appearance.accent_color, "");
    assert_eq!(appearance.accent_or_default(), DEFAULT_ACCENT);
}

#[test]
fn panel_alpha_is_clamped_to_the_contrast_floor() {
    // schema 下限 0.4；读侧再夹一次（挡手改文件绕过 schema 的情况）。
    let document = doc(json!({ "appearance": { "panel_alpha": 0.05 } }));
    assert!((Appearance::read(&document).panel_alpha - PANEL_ALPHA_MIN).abs() < 1e-9);
}

#[test]
fn appearance_keys_are_all_ai_writable_and_registered() {
    let schema = launcher_schema();
    // 主题三态是 D17 已定案的最轻量键（silent/Low），其余高级键留审计痕迹（audited/Medium）。
    let audited_keys = [
        "appearance.bg_image_enabled",
        "appearance.bg_image",
        "appearance.bg_fill",
        "appearance.bg_dim",
        "appearance.panel_alpha",
        "appearance.glass",
        "appearance.accent_color",
        // M91（D36/D37/D38）的外观下层三键，同样 audited。
        "appearance.window_opacity",
        "appearance.titlebar_mode",
        "appearance.palette_source",
    ];
    for key in audited_keys {
        let spec = schema.spec(key).unwrap_or_else(|| panic!("{key} 必须登记"));
        assert!(spec.ai.writable, "{key} 必须在 AI 可写白名单内");
        // 纯视觉、可一键复原 → 不要求弹窗确认（High 级留给凭据/数据根/代理）。
        assert!(
            !spec.ai.sensitivity.requires_confirmation(),
            "{key} 不该要确认"
        );
        assert_eq!(
            spec.ai.sensitivity.as_str(),
            "medium",
            "{key} 要留审计痕迹（AI 改过什么可回看）"
        );
    }
    // 主题三态仍是 silent（D17 已定案的最轻量键）。
    assert_eq!(
        schema
            .spec("appearance.theme_mode")
            .unwrap()
            .ai
            .sensitivity
            .as_str(),
        "low"
    );
}

#[test]
fn schema_registration_shape_is_right() {
    assert_eq!(
        launcher_version().minor,
        8,
        "1.8.0 登记 M197 的 download.gh_proxy（1.7.0 是 M168 的 ui.font_cdn）"
    );
    let schema = launcher_schema();
    assert_eq!(
        schema.spec("appearance.bg_fill").unwrap().kind,
        ValueKind::Enum(&["cover", "contain", "tile"])
    );
    assert!(matches!(
        schema.spec("appearance.panel_alpha").unwrap().kind,
        ValueKind::Float { min, max } if (min - PANEL_ALPHA_MIN).abs() < 1e-9 && (max - 1.0).abs() < 1e-9
    ));
    assert_eq!(
        schema.spec("appearance.accent_color").unwrap().kind,
        ValueKind::Text { max_len: 9 },
        "#RRGGBB 最多 7 字符，留 9 容错带井号与空白"
    );
    // 默认值 = 出厂态
    assert_eq!(
        schema.spec("appearance.panel_alpha").unwrap().default,
        serde_json::Value::from(1.0)
    );
    assert_eq!(
        schema.spec("appearance.glass").unwrap().default,
        serde_json::Value::from(false)
    );
}

#[test]
fn write_round_trips_through_the_store() {
    let temp =
        std::env::temp_dir().join(format!("ull-appearance-{}-{}", std::process::id(), line!()));
    std::fs::create_dir_all(temp.join("config")).expect("临时目录");
    let path = temp.join("config/launcher.json");
    let _ = std::fs::remove_file(&path);

    let mut store = unlucky_core::setting::store::SettingStore::load(&path, launcher_schema());
    let mut appearance = Appearance::from_store(&store);
    appearance.theme_mode = "dark".to_owned();
    appearance.panel_alpha = 0.8;
    appearance.bg_image_enabled = true;
    appearance.bg_image = "D:/wall/p.png".to_owned();
    appearance.accent_color = "#0f6cbd".to_owned();
    appearance.write(&mut store).expect("写外观键");
    store.save().expect("落盘");

    let reloaded = Appearance::from_store(&unlucky_core::setting::store::SettingStore::load(
        &path,
        launcher_schema(),
    ));
    assert_eq!(reloaded.theme_mode, "dark");
    assert!((reloaded.panel_alpha - 0.8).abs() < 1e-9);
    assert!(reloaded.bg_image_active());
    assert_eq!(reloaded.accent_color, "#0F6CBD");

    std::fs::remove_dir_all(&temp).ok();
}

#[test]
fn hex_helpers_validate_and_convert() {
    assert_eq!(normalize_hex(" 0f6cbd "), Some("#0F6CBD".to_owned()));
    assert_eq!(normalize_hex("#0F6CBD"), Some("#0F6CBD".to_owned()));
    assert_eq!(normalize_hex("#0F6CB"), None);
    assert_eq!(normalize_hex("#GGGGGG"), None);
    assert_eq!(hex_to_rgb("#0078D4"), Some((0x00, 0x78, 0xD4)));
    // WCAG 相对亮度：纯黑 0、纯白 1、Metro 蓝居中偏暗。
    assert!(relative_luminance("#000000").unwrap().abs() < 1e-9);
    assert!((relative_luminance("#FFFFFF").unwrap() - 1.0).abs() < 1e-9);
    let metro = relative_luminance(DEFAULT_ACCENT).unwrap();
    assert!(metro > 0.05 && metro < 0.3, "Metro 蓝亮度 {metro}");
}
