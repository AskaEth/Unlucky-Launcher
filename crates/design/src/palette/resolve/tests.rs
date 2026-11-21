//! `resolve` 的单测：三级优先序、失败回落、以及「回落落点是用户选的卡」这三类断言。
//!
//! 与 `mod.rs` 拆开只为单文件 ≤400 行；测试仍属于 `palette::resolve::tests`。

use super::*;
use crate::color::Color;

fn solid(color: Color, width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((width * height * 4) as usize);
    for _ in 0..(width * height) {
        out.extend_from_slice(&[color.r, color.g, color.b, color.a]);
    }
    out
}

#[test]
fn level_one_wins_when_background_is_good() {
    let pixels = solid(Color::rgb(0xC0, 0x39, 0x2B), 4, 4);
    let (scheme, provenance) = resolve_source(
        Some(ImageInput::pixels(&pixels, 4, 4)),
        None,
        Some(CardKey::Forest),
        SchemeFlavor::Light,
    );
    assert_eq!(provenance.level, Level::BackgroundImage);
    assert_eq!(provenance.background(), Outcome::Used);
    assert_eq!(provenance.preset(), Outcome::NotReached);
    assert!(provenance.hint_key().is_none());
    assert!(check(&scheme).is_clean());
    assert!(provenance.seed.is_some());
    // 取色方案的表面色温来自当前色卡（森林绿），不是默认卡。
    assert_eq!(provenance.card, CardKey::Forest);
}

#[test]
fn gray_background_falls_back_to_wallpaper_with_reason() {
    // 反证 1：灰白背景图 → 不许凭空上色，必须带着 low-chroma 原因回落。
    let gray = solid(Color::rgb(0xF2, 0xF2, 0xF0), 8, 8);
    let teal = solid(Color::rgb(0x00, 0x79, 0x6B), 8, 8);
    let (_, provenance) = resolve_source(
        Some(ImageInput::pixels(&gray, 8, 8)),
        Some(ImageInput::pixels(&teal, 8, 8)),
        Some(CardKey::Sky),
        SchemeFlavor::Light,
    );
    assert_eq!(provenance.level, Level::Wallpaper);
    match provenance.background() {
        Outcome::Failed(Skip::LowChroma { chroma, .. }) => assert!(chroma < 8.0),
        other => panic!("背景图应当是低彩度失败，实际 {other:?}"),
    }
    assert_eq!(
        provenance.hint_key(),
        Some("palette.hint.background.low-chroma")
    );
    assert!(provenance.did_fall_back());
    assert_eq!(
        provenance.reason_codes(),
        vec![("background-image", "low-chroma")]
    );
}

#[test]
fn undecodable_and_tiny_inputs_are_distinguished_from_absent() {
    let tiny = solid(Color::rgb(0xFF, 0x00, 0x00), 1, 2);
    let (_, provenance) = resolve_source(
        Some(ImageInput::Undecodable),
        Some(ImageInput::pixels(&tiny, 1, 2)),
        None,
        SchemeFlavor::Dark,
    );
    assert_eq!(provenance.level, Level::Preset);
    assert_eq!(provenance.background(), Outcome::Failed(Skip::Undecodable));
    assert_eq!(
        provenance.wallpaper(),
        Outcome::Failed(Skip::TooSmall {
            width: 1,
            height: 2
        })
    );
    assert_eq!(
        provenance.hint_key(),
        Some("palette.hint.background.undecodable")
    );
    // 从没选过卡 → 出厂默认。
    assert_eq!(provenance.card, CardKey::Sky);
}

#[test]
fn nothing_provided_uses_default_card_without_hint() {
    let (scheme, provenance) = resolve_source(None, None, None, SchemeFlavor::Light);
    assert_eq!(provenance.level, Level::Preset);
    assert_eq!(provenance.preset(), Outcome::Used);
    assert!(!provenance.did_fall_back());
    assert!(provenance.adjustments.is_empty());
    assert!(provenance.hint_key().is_none());
    assert_eq!(scheme, DEFAULT_CARD.scheme(SchemeFlavor::Light));
}

#[test]
fn preset_selection_decides_the_fallback_colour() {
    // 反证 2：回落落点是「用户选的那张卡」，不是默认卡。
    let gray = solid(Color::rgb(0x80, 0x80, 0x80), 4, 4);
    let (scheme, provenance) = resolve_source(
        Some(ImageInput::pixels(&gray, 4, 4)),
        None,
        Some(CardKey::Rose),
        SchemeFlavor::Light,
    );
    assert_eq!(provenance.level, Level::Preset);
    assert_eq!(provenance.card, CardKey::Rose);
    assert_eq!(scheme, CardKey::Rose.card().scheme(SchemeFlavor::Light));
}

#[test]
fn background_beats_wallpaper_when_both_work() {
    let bg = solid(Color::rgb(0xB2, 0x6A, 0x00), 8, 8);
    let wp = solid(Color::rgb(0x00, 0x79, 0x6B), 8, 8);
    let (_, provenance) = resolve_source(
        Some(ImageInput::pixels(&bg, 8, 8)),
        Some(ImageInput::pixels(&wp, 8, 8)),
        Some(CardKey::Teal),
        SchemeFlavor::Dark,
    );
    assert_eq!(provenance.level, Level::BackgroundImage);
    assert_eq!(
        provenance.wallpaper(),
        Outcome::NotReached,
        "② 有输入但根本没轮到——不等于「没提供」"
    );
    assert!(!provenance.did_fall_back());
}

#[test]
fn transparent_background_reports_its_own_reason() {
    let transparent = solid(Color::rgba(0x3C, 0x8D, 0xBC, 0), 4, 4);
    let (_, provenance) = resolve_source(
        Some(ImageInput::pixels(&transparent, 4, 4)),
        None,
        None,
        SchemeFlavor::Light,
    );
    assert_eq!(
        provenance.background(),
        Outcome::Failed(Skip::NoOpaquePixels)
    );
    assert_eq!(
        provenance.hint_key(),
        Some("palette.hint.background.no-opaque-pixels")
    );
}

#[test]
fn faint_colour_falls_back_to_the_selected_card() {
    // 复审 p2-3：D40 的门槛是 8，落在 6~8 之间的图必须**回落**，且落点是用户选中的那张卡。
    // 变异反证：`MIN_SEED_CHROMA` 8.0 → 6.0 时，这个种子会通过门槛，
    // `level` 变成 `BackgroundImage`，本条的第一条断言变红。
    let faint = Color::rgb(0x7E, 0x84, 0x89); // CAM16 chroma ≈ 7.23
    let pixels = solid(faint, 16, 16);
    let (scheme, provenance) = resolve_source(
        Some(ImageInput::pixels(&pixels, 16, 16)),
        None,
        Some(CardKey::Amber),
        SchemeFlavor::Light,
    );

    assert_eq!(provenance.level, Level::Preset, "低彩度图必须回落");
    assert_eq!(provenance.card, CardKey::Amber, "落点是用户选中的卡");
    assert_eq!(
        scheme,
        CardKey::Amber.card().scheme(SchemeFlavor::Light),
        "方案必须逐值等于选中卡"
    );
    match provenance.background() {
        Outcome::Failed(Skip::LowChroma { chroma, .. }) => {
            assert!((6.0..8.0).contains(&chroma), "实测彩度 {chroma}");
        }
        other => panic!("应当是低彩度失败，实际 {other:?}"),
    }
}

#[test]
fn a_five_percent_accent_drives_level_one() {
    // 端到端反证（与 `palette::seed::tests::end_to_end_picks_the_five_percent_accent` 配对）：
    // 95% 米白 + 5% 珊瑚的背景图**必须**在一级取色成功，且种子里就是那 5% 的珊瑚。
    // 若 `analyze_rgba` 退化成「不透明像素求算术平均」，这条会变成「回落色卡」而红。
    let beige = Color::rgb(0xF5, 0xF1, 0xEA);
    let coral = Color::rgb(0xFF, 0x6B, 0x5A);
    let mut pixels = Vec::with_capacity(40 * 40 * 4);
    for index in 0..40 * 40 {
        let color = if index < 40 * 40 - 80 { beige } else { coral };
        pixels.extend_from_slice(&[color.r, color.g, color.b, color.a]);
    }

    let (scheme, provenance) = resolve_source(
        Some(ImageInput::pixels(&pixels, 40, 40)),
        None,
        Some(CardKey::Rose),
        SchemeFlavor::Light,
    );
    assert_eq!(provenance.level, Level::BackgroundImage, "{provenance:?}");
    assert_eq!(provenance.background(), Outcome::Used);
    let seed = provenance.seed.expect("一级成功必须带种子");
    assert_eq!(seed.color, coral, "种子应当是 5% 的珊瑚");
    assert!(provenance.hint_key().is_none());
    // 方案来自珊瑚，而不是「回落到用户选的 Rose 卡」——两者 primary 不同。
    assert_ne!(scheme, CardKey::Rose.card().scheme(SchemeFlavor::Light));
    assert!(crate::palette::wcag::check(&scheme).is_clean());
}

#[test]
fn not_reached_is_distinguishable_from_not_provided() {
    // 复审（色彩 r1 p2-2）：`Absent` 曾同时表示「用户没提供」和「更高级成功所以没轮到」，
    // UI 照字面写「色卡：未选择」就会与事实相反（用户明明选了卡，而且它的中性彩度参与了生成）。
    let pixels = solid(Color::rgb(0xC0, 0x39, 0x2B), 4, 4);

    // 一级成功：壁纸与色卡都是「没轮到」，但 card 仍然是用户选的那张。
    let (_, provenance) = resolve_source(
        Some(ImageInput::pixels(&pixels, 4, 4)),
        Some(ImageInput::Undecodable), // 壁纸**提供了**，只是没轮到
        Some(CardKey::Forest),
        SchemeFlavor::Light,
    );
    assert_eq!(provenance.level, Level::BackgroundImage);
    assert_eq!(provenance.wallpaper(), Outcome::NotReached);
    assert_eq!(provenance.preset(), Outcome::NotReached);
    assert_eq!(provenance.card, CardKey::Forest, "没轮到 ≠ 没选择");

    // 真的没提供：背景图 `None` → Absent；壁纸成功 → 色卡 NotReached。
    let (_, provenance) = resolve_source(
        None,
        Some(ImageInput::pixels(&pixels, 4, 4)),
        Some(CardKey::Forest),
        SchemeFlavor::Light,
    );
    assert_eq!(provenance.background(), Outcome::Absent);
    assert_eq!(provenance.preset(), Outcome::NotReached);
    assert_eq!(
        provenance.reason_codes(),
        Vec::<(&'static str, &'static str)>::new(),
        "没提供 / 没轮到都不是「失败」，不该进原因码"
    );
}
