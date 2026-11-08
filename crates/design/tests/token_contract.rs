//! 令牌契约测试：从**消费方视角**（只用公开 API）钉住 M61 承诺的那些性质。
//!
//! 这份测试的作用是「冻结」：`IconKind`/`Role` 的次序与个数、type scale 的取值、形状/高程阶梯、
//! 描边的 SVG 语义换算。后续 iced 页面线靠它保证不会因为一次「顺手改动」而悄悄换观感。

use unlucky_design::color::{Role, Scheme, SchemeFlavor, TonalPalette};
use unlucky_design::component_map::Component;
use unlucky_design::elevation::{Elevation, MaterialLayer};
use unlucky_design::icons::{
    stroke_width_for, visual_stroke_px, IconKind, IconPaint, IconSize, VIEWBOX,
};
use unlucky_design::motion::{
    DurationToken, Easing, MotionScale, SPRING_EXPRESSIVE_SPATIAL_DEFAULT,
};
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::{LayoutMetrics, Space};
use unlucky_design::typography::{FontFamily, Platform, TypeRole, Weight};
use unlucky_design::DesignTokens;

#[test]
fn color_role_table_is_frozen() {
    assert_eq!(Role::ALL.len(), 49, "M3 基础角色集共 49 个");
    assert_eq!(Role::ALL[0], Role::Primary);
    assert_eq!(Role::ALL[16], Role::Surface);
    assert_eq!(Role::ALL[48], Role::OnTertiaryFixedVariant);
    // 每个角色都能从两套 scheme 里取到值，且前景角色都带 on- 语义。
    let light = Scheme::baseline(SchemeFlavor::Light);
    let dark = Scheme::baseline(SchemeFlavor::Dark);
    for role in Role::ALL {
        assert!(
            light.get(role).is_opaque(),
            "{} 应当是不透明色",
            role.name()
        );
        assert!(dark.get(role).is_opaque());
        if role.name().starts_with("on-") {
            assert!(role.is_foreground(), "{} 应被判为前景色", role.name());
        }
    }
    assert!(light.get(Role::Primary).is_opaque());
}

#[test]
fn tone_mapping_is_the_m3_table() {
    // 抽查几行关键映射（light / dark），防止有人改 tone 表时静默换观感。
    assert_eq!(Scheme::tone_of(Role::Primary, SchemeFlavor::Light), 40.0);
    assert_eq!(Scheme::tone_of(Role::Surface, SchemeFlavor::Light), 98.0);
    assert_eq!(Scheme::tone_of(Role::Surface, SchemeFlavor::Dark), 6.0);
    assert_eq!(
        Scheme::tone_of(Role::OnPrimaryContainer, SchemeFlavor::Dark),
        90.0
    );
    assert_eq!(Scheme::tone_of(Role::Outline, SchemeFlavor::Light), 50.0);
}

#[test]
fn type_scale_values_are_the_m3_baseline() {
    assert_eq!(TypeRole::ALL.len(), 15);
    let title = TypeRole::TitleLarge.style();
    assert_eq!((title.size_px, title.line_height_px), (22.0, 28.0));
    let body = TypeRole::BodyMedium.style();
    assert_eq!((body.size_px, body.line_height_px), (14.0, 20.0));
    assert_eq!(body.weight, Weight::Regular);
    let label = TypeRole::LabelLarge.style();
    assert_eq!(label.weight, Weight::Medium);
    assert_eq!(TypeRole::BodySmall.style().size_px, 12.0);
    // 强调档只动字重。
    assert_eq!(TypeRole::TitleLarge.emphasized().size_px, 22.0);
    assert!(TypeRole::TitleMedium.emphasized().weight.value() > Weight::Medium.value());
}

#[test]
fn font_stacks_never_embed_font_files() {
    for platform in [Platform::Windows, Platform::MacOs, Platform::Linux] {
        for family in [FontFamily::Display, FontFamily::Text, FontFamily::Mono] {
            let stack = platform.font_stack(family);
            assert!(!stack.is_empty());
            for face in stack {
                let lower = face.family.to_ascii_lowercase();
                for banned in [".ttf", ".otf", ".ttc", "woff", "file"] {
                    assert!(
                        !lower.contains(banned),
                        "{} 的字体栈出现文件引用 {}（只允许族名，禁止内嵌受限字体）",
                        platform.name(),
                        face.family
                    );
                }
            }
        }
    }
}

#[test]
fn cjk_weight_gap_is_registered() {
    // 中文 700（Expressive 强调档要用的）在三平台都要能找到原生落点，否则就是合成加粗降级。
    use unlucky_design::typography::{Weight, WeightMatch};
    // Windows：Segoe UI Variable 覆盖 700（原生）；CJK 落点是雅黑 700（原生，雅黑有 Bold）。
    let win = Platform::Windows.resolve_weight(FontFamily::Text, Weight::Bold);
    assert_eq!(win.quality, WeightMatch::Native);
    assert!(Platform::Windows.cjk_face().supports(Weight::Bold));
    // macOS：苹方只到 600 → CJK 的 700 只能合成/换族，这条 gap 已在文档登记。
    assert!(!Platform::MacOs.cjk_face().supports(Weight::Bold));
    // Linux：Noto Sans CJK 可变字体全覆盖。
    for weight in [Weight::Regular, Weight::Medium, Weight::Bold] {
        assert!(Platform::Linux.cjk_face().supports(weight));
    }
}

#[test]
fn icon_kind_is_frozen_and_append_only() {
    assert_eq!(IconKind::ALL.len(), 28, "M61 冻结 28 个图标");
    // 迁移来的 24 个必须保持原次序（序号即持久化 id）。
    assert_eq!(IconKind::ALL[0], IconKind::Instances);
    assert_eq!(IconKind::ALL[4], IconKind::Ai);
    assert_eq!(IconKind::ALL[20], IconKind::More);
    assert_eq!(IconKind::ALL[23], IconKind::User);
    // 新增的 4 个追加在末尾。
    assert_eq!(IconKind::ALL[24], IconKind::Refresh);
    assert_eq!(IconKind::ALL[27], IconKind::Trash);
    for kind in IconKind::ALL {
        assert!(kind.icon().path.starts_with('M'));
        assert_eq!(kind.icon().kind, kind);
    }
}

#[test]
fn icon_stroke_follows_svg_semantics_not_slint() {
    // 派生式（**这条测试钉的就是它**）：`stroke = 1.5 × 24 / render_px`，1.5 是视觉线宽、24 是 viewBox。
    //
    // 两个锚点，一个不随本次收敛变化、一个随令牌取值：
    // - 20px（`IconSize::Nav`，本轮没动）→ 1.5 × 24 / 20 = **1.8**
    // - 18px（`IconSize::Button`，M86 由 16 收敛到官方 18）→ 1.5 × 24 / 18 = **2.0**
    //
    // ⚠️ **这条只保证「烘焙路径」**（`crates/app-iced/src/icons.rs::view_at`：按盒尺寸烘焙 SVG，
    // 于是一个 viewBox 单位的 stroke 落到盒子后正好是 1.5 视觉像素）。
    // 它**不**保证「24 烘焙 + 布局缩到 18」那条路径——那条会得到 `1.5 × 18/24 = 1.125px`，
    // 正是复审 r1 P2-2 实测到的 1.04~1.25px 缺陷。所以组件侧凡是盒 ≠ `RENDER_PX`(24) 的调用点
    // 都必须走 `view_at`（见 `icons.rs::tests::view_at_bakes_the_requested_size` 与
    // `crates/app-iced/src/components/gate.rs` 的 Q9 扫描）。
    //
    // 之所以写死字面量而不写成 `stroke_width_for(IconSize::Nav.px(), 1.5)`：后者等于拿实现算实现，
    // 派生式一旦被改坏（例如把 ×24 写成 ×20）它照样绿。写死才会红。
    // Slint 口径（绝对 1.5px，不乘 viewBox）在这两档都会得到 1.5 —— 那正是这条要防的错。
    let nav_width = IconKind::Settings.icon().stroke_width(IconSize::Nav.px());
    assert!(
        (nav_width - 1.8).abs() < 1.0e-6,
        "20px 渲染应取 1.8 个 viewBox 单位（锚点，不随 IconSize 收敛变化）"
    );
    let button_width = IconKind::Settings
        .icon()
        .stroke_width(IconSize::Button.px());
    assert!(
        (button_width - 2.0).abs() < 1.0e-6,
        "18px 渲染应取 2.0 个 viewBox 单位（M86：Button 由 16→18，旧期望 2.25 是 16px 的派生值）"
    );
    // 视觉宽度的往返一致性（这条是「别照抄 Slint 的 1.5」的守门测试）。
    for size in IconSize::ALL {
        let w = stroke_width_for(size.px(), 1.5);
        assert!((visual_stroke_px(size.px(), w) - 1.5).abs() < 1.0e-6);
    }
    assert!((VIEWBOX - 24.0).abs() < f32::EPSILON);
    // 实心图标不描边。
    assert_eq!(IconKind::Play.icon().paint, IconPaint::Fill);
    assert_eq!(IconKind::Play.icon().stroke_width(24.0), 0.0);
}

#[test]
fn shape_and_elevation_ladders() {
    assert_eq!(ShapeScale::ExtraSmall.dp(), 4.0);
    assert_eq!(ShapeScale::Medium.dp(), 12.0);
    assert_eq!(ShapeScale::ExtraLarge.dp(), 28.0);
    assert_eq!(ShapeScale::Full.radius_px(40.0), 20.0);
    assert!(ShapeScale::ExtraLargeIncreased.is_expressive());

    let dp: Vec<f32> = Elevation::ALL.iter().map(|e| e.dp()).collect();
    assert_eq!(dp, vec![0.0, 1.0, 3.0, 6.0, 8.0, 12.0]);
    assert_eq!(
        MaterialLayer::Card.surface_role(),
        Role::SurfaceContainerLow
    );
}

#[test]
fn motion_tokens_are_usable_as_data() {
    assert_eq!(DurationToken::Medium2.ms(), 300);
    assert!((Easing::Standard.sample(0.0)).abs() < 1.0e-6);
    assert!((Easing::Standard.sample(1.0) - 1.0).abs() < 1.0e-6);
    let settle = SPRING_EXPRESSIVE_SPATIAL_DEFAULT.settle_ms(0.001);
    assert!(settle > 0.0 && settle < 2000.0);
    assert_eq!(MotionScale::NONE.ms(DurationToken::Long4), 0.0);
}

#[test]
fn spacing_and_layout_metrics_are_shared() {
    assert_eq!(Space::X4.dp(), 16.0);
    let metrics = LayoutMetrics::DESKTOP;
    assert_eq!(metrics.header_height(false), 56.0);
    assert_eq!(metrics.header_height(true), 84.0);
}

#[test]
fn component_map_covers_the_page_skeleton() {
    // M69（G9）在末尾追加 Checkbox/Radio/Search/NavigationDrawer/Divider：20 → 25。
    assert_eq!(Component::ALL.len(), 25);
    for component in Component::ALL {
        let spec = component.spec();
        assert_eq!(spec.component, component);
        assert!(spec.text.style().size_px >= 11.0);
    }
    assert_eq!(Component::PrimaryButton.spec().shape, ShapeScale::Full);
    // 浮层必须高于卡片层。
    assert!(Component::Dialog.spec().elevation > Component::OutlinedCard.spec().elevation);
    // 末尾追加的 5 个不许改动既有次序。
    assert_eq!(Component::ALL[0], Component::TopAppBar);
    assert_eq!(Component::ALL[19], Component::EmptyState);
}

#[test]
fn design_tokens_entry_point() {
    let light = DesignTokens::baseline(SchemeFlavor::Light);
    let dark = DesignTokens::baseline(SchemeFlavor::Dark);
    assert_eq!(light.flavor, SchemeFlavor::Light);
    assert!(light.color(Role::Surface).relative_luminance() > 0.8);
    assert!(dark.color(Role::Surface).relative_luminance() < 0.05);
    assert!(!light.motion_scale.is_off());

    let accent = DesignTokens::with_accent(
        SchemeFlavor::Light,
        unlucky_design::Color::from_hex("#006A5C").unwrap(),
    );
    assert_ne!(accent.color(Role::Primary), light.color(Role::Primary));
    assert_eq!(accent.color(Role::Surface), light.color(Role::Surface));
}

#[test]
fn tonal_palette_hits_requested_tones() {
    let palette = TonalPalette::new(150.0, 36.0);
    for tone in [20.0_f64, 40.0, 60.0, 80.0] {
        let color = palette.tone(tone);
        assert!(
            (f64::from(color.lstar()) - tone).abs() <= 1.0,
            "tone {tone} 实际 {}",
            color.lstar()
        );
    }
}
