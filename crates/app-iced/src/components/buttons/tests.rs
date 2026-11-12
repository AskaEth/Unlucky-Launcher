//! 按钮家族的单测（几何值 / 色彩角色 / 样式闭包）。
//!
//! 这里测的是「函数与样式结构」；**像素级**的几何（40 高、20 圆角、24 图标盒）
//! 由 `gallery/probe.rs` 对着真截图量——M68 的教训是「函数对、图错」，两者缺一不可。

use iced::widget::button;
use iced::{Background, Shadow};

use unlucky_design::color::SchemeFlavor;
use unlucky_design::elevation::Elevation;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;

use super::super::interaction::{self, Base, Interaction};
use super::assemble::StyleSpec;
use super::{style_closure, Geometry, IconGeometry, IconVariant, Variant};
use crate::theme::{Families, Ui, STROKE_PX};

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

fn ui(flavor: SchemeFlavor) -> Ui {
    Ui::with_platform(
        flavor,
        unlucky_design::typography::Platform::Windows,
        FAMILIES,
    )
}

fn light() -> Ui {
    ui(SchemeFlavor::Light)
}

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Pressed(&'static str),
}

#[test]
fn every_variant_builds_and_disabled_has_no_message() {
    let ui = light();
    let _ = super::filled(&ui, super::Button::new("Filled", Msg::Pressed("f")));
    let _ = super::tonal(&ui, super::Button::new("Tonal", Msg::Pressed("t")));
    let _ = super::elevated(&ui, super::Button::new("Elevated", Msg::Pressed("e")));
    let _ = super::outlined(&ui, super::Button::new("Outlined", Msg::Pressed("o")));
    let _ = super::text(&ui, super::Button::new("Text", Msg::Pressed("x")));
    let _ = super::filled::<Msg>(&ui, super::Button::disabled("Disabled"));
    let _ = super::filled(
        &ui,
        super::Button::new("Icons", Msg::Pressed("i"))
            .leading(IconKind::Plus)
            .trailing(IconKind::ChevronRight),
    );
}

/// 12b 1.1~1.6 + 2.1~2.2：几何逐值钉住（每个值都要能指回令牌）。
#[test]
fn geometry_matches_the_reference_values() {
    let ui = light();
    let geometry = Geometry::of(&ui, Variant::Filled, false);
    assert_eq!(geometry.height, 40.0, "12b 1.1 可见高 40");
    assert_eq!(geometry.height, ui.metrics().control_height);
    assert_eq!(
        ui.radius(unlucky_design::shape::ShapeScale::Full, geometry.height)
            .top_left,
        20.0,
        "12b 1.2 full 圆角 = 高/2"
    );
    // M229：内距取规格卡的 `ControlSize::pad_x`（S 档 = 20），不再是 12b 的 24。
    assert_eq!(geometry.padding.left, 20.0, "S 档 pad_x 20");
    assert_eq!(geometry.padding.right, 20.0, "S 档 pad_x 20");
    assert_eq!(geometry.padding.top, 0.0);
    assert_eq!(geometry.gap, 8.0, "12b 1.6 图标↔文字 8");
    assert_eq!(geometry.gap, ui.space(Space::X2));
    assert_eq!(geometry.stroke, 0.0, "实心按钮无描边");

    let with_icon = Geometry::of(&ui, Variant::Filled, true);
    assert_eq!(
        with_icon.padding.left, 16.0,
        "带图标左侧再收 4（20 − 4）：与 12b 1.4 的 16 恰好同值"
    );
    assert_eq!(with_icon.padding.right, 20.0, "带图标右内边距不动");

    let outlined = Geometry::of(&ui, Variant::Outlined, false);
    assert_eq!(outlined.stroke, STROKE_PX, "rubric G7 全站 1px");
    assert_eq!(outlined.stroke, 1.0);

    // 图标 18 是**令牌值**（`IconSize::Button`，AAR `ButtonDefaults.IconSize`）；
    // 12b 1.6 的 18 与它同源。**别按 `ButtonSmallTokens.IconSize = 20` 改**：那是零消费者的死令牌。
    assert_eq!(
        geometry.icon, 18.0,
        "IconSize::Button（官方 ButtonDefaults.IconSize）"
    );
    assert_eq!(
        geometry.icon,
        unlucky_design::icons::IconSize::Button.px(),
        "组件侧必须只从令牌取值（不许写字面量）"
    );
    assert_eq!(geometry.height % 4.0, 0.0, "可见高必须在 4px 栅格上（D1）");
}

#[test]
fn icon_button_geometry_matches_the_reference() {
    let ui = light();
    let geometry = IconGeometry::of(&ui);
    assert_eq!((geometry.side, geometry.icon), (40.0, 24.0), "12b 2.1/2.2");
    assert_eq!(
        ui.radius(unlucky_design::shape::ShapeScale::Full, geometry.side)
            .top_left,
        20.0,
        "40×40 的 full 圆角 = 20"
    );
    assert_eq!(geometry.padding, 8.0, "12b 2.2 默认边距 8");
    // 命中区 = 容器 40 + 四周各 4（12b 2.8）；按钮自身不撑高
    assert_eq!(
        geometry.side + 2.0 * ui.space(Space::X1),
        ui.metrics().touch_target,
        "40 + 8 = 48 命中下限"
    );
}

mod base;
mod icon;
mod roles;

/// 视觉 reviewer P2-3：同一控件里不许混用填充字形与描边字形（线宽语汇会打架）。
///
/// gallery 的样例图标（前置/后置）必须同为 `IconPaint::Stroke`；允许实心字形，但只能作为
/// 「主操作」单独出场的图标（例如播放键），不能与描边图标同处一颗按钮。
#[test]
fn sample_icons_never_mix_fill_and_stroke() {
    use unlucky_design::icons::IconPaint;

    let sample = [IconKind::Download, IconKind::ChevronRight];
    for kind in sample {
        assert_eq!(
            kind.icon().paint,
            IconPaint::Stroke,
            "{} 是填充字形，不能和描边图标混在同一颗按钮里",
            kind.name()
        );
    }
    // 已知的实心字形（留档，下次有人拿它们当按钮图标时这条会提醒）
    assert_eq!(IconKind::Play.icon().paint, IconPaint::Fill);
}

#[test]
fn five_variants_have_distinct_colour_roles() {
    let mut seen = Vec::new();
    for variant in Variant::ALL {
        let roles = variant.roles();
        assert!(
            !seen.contains(&roles),
            "{variant:?} 与别的变体色彩角色重复了"
        );
        seen.push(roles);
    }

    // 12b 1.9 逐条角色名核对
    assert_eq!(
        Variant::Filled.roles(),
        (
            Some(unlucky_design::color::Role::Primary),
            unlucky_design::color::Role::OnPrimary,
            None
        )
    );
    assert_eq!(
        Variant::Tonal.roles(),
        (
            Some(unlucky_design::color::Role::SecondaryContainer),
            unlucky_design::color::Role::OnSecondaryContainer,
            None
        )
    );
    assert_eq!(
        Variant::Elevated.roles(),
        (
            Some(unlucky_design::color::Role::SurfaceContainerLow),
            unlucky_design::color::Role::Primary,
            None
        )
    );
    assert_eq!(
        Variant::Outlined.roles(),
        (
            None,
            unlucky_design::color::Role::OnSurfaceVariant,
            Some(unlucky_design::color::Role::Outline)
        ),
        "描边取 Outline（M248：非文字元素角色；OutlineVariant 是装饰分隔线的）"
    );
    assert_eq!(
        Variant::Text.roles(),
        (None, unlucky_design::color::Role::OnSurfaceVariant, None)
    );

    for variant in Variant::ALL {
        let expected = if variant == Variant::Elevated {
            Elevation::Level1
        } else {
            Elevation::Level0
        };
        assert_eq!(variant.elevation(), expected, "{variant:?} 高程");
    }
}

#[test]
fn icon_button_variants_match_the_reference_roles() {
    use unlucky_design::color::Role;
    assert_eq!(
        IconVariant::Standard.roles(),
        (None, Role::OnSurfaceVariant, None)
    );
    assert_eq!(
        IconVariant::Filled.roles(),
        (Some(Role::Primary), Role::OnPrimary, None)
    );
    assert_eq!(
        IconVariant::Tonal.roles(),
        (
            Some(Role::SecondaryContainer),
            Role::OnSecondaryContainer,
            None
        )
    );

    let ui = light();
    for variant in IconVariant::ALL {
        let _ = super::icon::<Msg>(&ui, variant, IconKind::Settings, None, None);
        let _ = super::icon(
            &ui,
            variant,
            IconKind::Settings,
            Some(Msg::Pressed("i")),
            None,
        );
        // 五态取证开关都能出元素（gallery 矩阵依赖它）
        for state in Interaction::ALL {
            let _ = super::icon(
                &ui,
                variant,
                IconKind::Settings,
                Some(Msg::Pressed("i")),
                Some(state),
            );
        }
    }
}

fn spec_of(variant: Variant, forced: Option<Interaction>) -> StyleSpec {
    let (fill, content, border) = variant.roles();
    StyleSpec {
        fill,
        content,
        border,
        shape: variant.shape(),
        base: Base::CANVAS,
        stroke: Geometry::of(&light(), variant, false).stroke,
        radius_side: Geometry::of(&light(), variant, false).height,
        forced,
    }
}

/// **渲染层**：样式闭包吐给 iced 的圆角/描边/阴影必须是令牌推导值。
#[test]
fn outlined_style_draws_hairline_border_and_full_radius() {
    let ui = light();
    let style = style_closure(&ui, spec_of(Variant::Outlined, None));

    let idle = style(&ui.theme(), button::Status::Active);
    assert_eq!(idle.background, None, "Outlined 静息容器必须透明");
    assert_eq!(idle.border.width, STROKE_PX, "1dp 描边");
    assert_eq!(idle.border.radius.top_left, 20.0, "full 圆角");
    assert_eq!(
        idle.border.color,
        ui.color(unlucky_design::color::Role::Outline)
    );
    assert_eq!(idle.shadow, Shadow::default(), "L0 不许有阴影");

    // 透明容器也要看得见 hover：状态层叠到表面色上
    let hovered = style(&ui.theme(), button::Status::Hovered);
    assert_ne!(hovered.background, None, "透明容器的 hover 必须有可见变化");
    assert_ne!(hovered.background, idle.background);

    let disabled = style(&ui.theme(), button::Status::Disabled);
    let (container, content) = interaction::disabled_button_colors(&ui, Base::CANVAS);
    assert_eq!(disabled.text_color, content, "禁用内容 = 令牌 38%");
    assert_eq!(
        disabled.border.color, container,
        "禁用描边 = 按钮族的令牌 10%（OutlinedButtonTokens.DisabledContainerOpacity）"
    );
}

#[test]
fn filled_button_state_layers_are_visible_and_ordered() {
    let ui = light();
    let style = style_closure(&ui, spec_of(Variant::Filled, None));

    let idle = style(&ui.theme(), button::Status::Active);
    let hovered = style(&ui.theme(), button::Status::Hovered);
    let pressed = style(&ui.theme(), button::Status::Pressed);

    assert_eq!(
        idle.background,
        Some(Background::Color(
            ui.color(unlucky_design::color::Role::Primary)
        ))
    );
    assert_ne!(hovered.background, idle.background, "hover 8%");
    assert_ne!(
        pressed.background, hovered.background,
        "pressed 10% ≠ hover 8%"
    );
    assert_eq!(
        hovered.text_color,
        ui.color(unlucky_design::color::Role::OnPrimary),
        "12b 2.4/4.7：状态层不换内容色"
    );
    assert_eq!(idle.border.width, 0.0, "实心按钮无描边");
}

#[test]
fn forced_state_overrides_the_runtime_status() {
    let ui = light();
    let style = style_closure(&ui, spec_of(Variant::Tonal, Some(Interaction::Hovered)));
    // 运行期说 Active，但取证要 hover → 必须画 hover
    assert_eq!(
        style(&ui.theme(), button::Status::Active).background,
        style(&ui.theme(), button::Status::Hovered).background
    );
    assert_ne!(
        style(&ui.theme(), button::Status::Active).background,
        Some(Background::Color(
            ui.color(unlucky_design::color::Role::SecondaryContainer)
        ))
    );
}

#[test]
fn elevated_uses_a_tone_not_a_shadow() {
    let ui = light();
    let style = style_closure(&ui, spec_of(Variant::Elevated, None));
    let idle = style(&ui.theme(), button::Status::Active);
    assert_eq!(idle.shadow, Shadow::default(), "rubric §7：L1 不画阴影");
    assert_eq!(
        idle.background,
        Some(Background::Color(
            ui.color(unlucky_design::color::Role::SurfaceContainerLow)
        )),
        "L1 由 SurfaceContainerLow 的色调差表达"
    );
    assert_eq!(
        idle.text_color,
        ui.color(unlucky_design::color::Role::Primary)
    );
}
