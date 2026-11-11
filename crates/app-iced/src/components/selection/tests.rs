//! 选择类家族的单测：几何取自令牌、角色取自 A1（`#[cfg(test)]` 下的纯值断言）。
//!
//! **这些测试测的是「值与映射」**——它们抓不住「函数对、图错」（M68 的教训），
//! 像素级几何在 `components/gallery/selection.rs` 的 `probe` 里对着真截图量。

use unlucky_design::color::{Role, SchemeFlavor};
use unlucky_design::typography::Platform;

use super::super::interaction::{self, Base, Interaction};

mod chip;
use super::geometry::{ChipGeometry, ControlGeometry, SwitchGeometry};
use super::paint::{self, ChipVariant};
use crate::theme::{Families, Ui};

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

fn ui() -> Ui {
    Ui::with_platform(SchemeFlavor::Light, Platform::Windows, FAMILIES)
}

/// 复审 P2-2：命中区恒 **48×48**——Ø40 状态层之外左右各 4dp 透明内边距（无标签时行宽才不是 40）。
#[test]
fn hit_area_is_48_square_even_without_a_label() {
    let ui = ui();
    for (name, geometry) in [
        ("checkbox", ControlGeometry::checkbox(&ui)),
        ("radio", ControlGeometry::radio(&ui)),
    ] {
        assert_eq!(geometry.hit, 48.0, "{name} 命中区必须是 48");
        assert_eq!(geometry.state_layer, 40.0, "{name} 状态层 Ø40");
        assert_eq!(geometry.hit_padding(), 4.0, "{name} 左右各留 4dp");
        assert_eq!(
            geometry.state_layer + geometry.hit_padding() * 2.0,
            geometry.hit,
            "{name}：可见 40 + 左右各 4 = 命中 48"
        );
    }
}

/// §18.1 / §18.5 / §18.9：Checkbox 的四个参考值全部来自令牌层。
#[test]
fn checkbox_geometry_comes_from_the_token_layer() {
    let ui = ui();
    let g = ControlGeometry::checkbox(&ui);
    // M229：取规格卡的 18×18 / rx4（官方 20×20 / 2dp，登记在 `inkcat` 模块）。
    assert_eq!(g.side, 18.0, "规格卡 rect 18×18");
    assert_eq!(g.radius, 4.0, "规格卡 rx=4（ShapeScale::ExtraSmall）");
    assert_eq!(g.stroke, 2.0, "18.2：未选中描边 2px");
    assert_eq!(g.state_layer, 40.0, "18.5：状态层 Ø40");
    assert_eq!(g.hit, 48.0, "18.9：命中区 48");
    assert_eq!(g.gap, 8.0, "18.9：框↔标签 8（[估] 团队口径）");
    assert_eq!(g.inner_dot, 0.0, "Checkbox 没有内点");
    // 18.4：勾形横向端点跨度 = 0.8−0.2 = 0.6 × 18 = 10.8（比例不变，边长换成规格卡的 18）
    assert!((g.check_span() - 10.8).abs() < 0.001, "18.4：勾横向 10.8dp");
    // 方头外扩：每端 2/√2 ≈ 1.41 → 墨迹跨度 ≈ 14.8（参考值 12 是端点跨度，不是墨迹）
    let bleed = g.square_cap_bleed();
    assert!(
        (bleed - std::f32::consts::SQRT_2).abs() < 0.001,
        "方头外扩 1/√2×线宽"
    );
    assert!(
        (g.check_span() + bleed * 2.0 - 13.63).abs() < 0.05,
        "墨迹跨度 ≈13.63（10.8 + 2×1.41）"
    );
}

/// §19.1 / §19.2：Radio 外圈 **18**（规格卡）、环线 2、**内点 10 不是 12**、真圆。
#[test]
fn radio_inner_dot_is_ten_not_twelve() {
    let ui = ui();
    let g = ControlGeometry::radio(&ui);
    // M229：外圈取规格卡的 18 Ø（官方 20 Ø，登记在 `inkcat` 模块）。
    assert_eq!(g.side, 18.0, "规格卡 r=9 ⇒ 直径 18");
    assert_eq!(g.stroke, 2.0, "19.1：环线 2px");
    assert_eq!(g.radius, 9.0, "19.1：真圆（半径 = 边长/2）");
    assert_eq!(g.inner_dot, 10.0, "19.2：内点可视直径 10");
    assert_ne!(
        g.inner_dot, 12.0,
        "19.2：12 是含描边前的名义值，抄 12 会明显偏大"
    );
    assert_eq!(g.state_layer, 40.0, "19.7：状态层 Ø40");
    assert_eq!(g.hit, 48.0, "19.7：命中区 48");
}

/// §10.1 / §10.2 / §10.6：Switch 的三档滑块与状态层，以及「状态层必然溢出轨道」这条布局不变量。
#[test]
fn switch_geometry_matches_the_tokens() {
    let ui = ui();
    let g = SwitchGeometry::of(&ui);
    assert_eq!(g.track_width, 52.0, "10.1：轨道宽 52");
    assert_eq!(g.track_height, 32.0, "10.1：轨道高 32");
    assert_eq!(g.track_stroke, 2.0, "10.3：未选中态 2px Outline");
    assert_eq!(g.thumb(false, false), 16.0, "10.2：未选中滑块 16");
    assert_eq!(g.thumb(true, false), 24.0, "10.2：选中滑块 24");
    assert_eq!(g.thumb(false, true), 28.0, "10.2：按下滑块 28");
    assert_eq!(g.thumb(true, true), 28.0, "10.2：按下档与选中无关");
    assert_eq!(
        g.icon, 16.0,
        "10.5：滑块内图标 16（官方 SwitchTokens.SelectedIconSize，令牌 IconSize::SwitchThumb）"
    );
    assert_eq!(
        g.icon,
        unlucky_design::icons::IconSize::SwitchThumb.px(),
        "滑块图标必须走 SwitchThumb 档，不许借按钮的 18"
    );
    assert_eq!(g.state_layer, 40.0, "10.6：状态层 Ø40");
    assert_eq!(g.hit, 48.0, "命中区 48");

    // 内距公式在三个档位都成立（32−24=8→4、32−16=16→8、32−28=4→2）
    for thumb in [24.0, 16.0, 28.0] {
        assert!((g.thumb_inset(thumb) * 2.0 + thumb - g.track_height).abs() < 0.001);
    }
    // 状态层必然溢出轨道左右：未选中时滑块靠左、选中时靠右
    assert_eq!(g.bleed(), 4.0, "状态层比轨道高 → 上下各溢出 4");
    assert_eq!(g.draw_width(), 60.0, "绘制区宽 = 52 + 2×4");
    for (selected, thumb) in [(false, 16.0), (true, 24.0), (false, 28.0), (true, 28.0)] {
        let offset = g.layer_offset(selected, thumb);
        assert!(
            offset >= 0.0,
            "状态层左缘不得为负（selected={selected} thumb={thumb} → {offset}）"
        );
        // 圆心与滑块同心：offset + 半径 = 滑块圆心 + 外扩
        let center = offset + g.layer_radius();
        assert!((center - (g.thumb_center(selected, thumb) + g.bleed())).abs() < 0.001);
    }
}

/// §13.1 / §13.3 / §13.4：Chip 高 32、圆角 8、内边距 8、图标 18（官方 `AssistChipTokens.IconSize`，G14 已收口）。
#[test]
fn chip_geometry_matches_the_tokens() {
    let ui = ui();
    let g = ChipGeometry::of(&ui);
    assert_eq!(g.height, 32.0, "13.1：容器高 32");
    assert_eq!(g.radius, 8.0, "13.1：圆角 8（ContainerShape=CornerSmall）");
    assert_eq!(g.pad, 8.0, "13.3：水平内边距 8");
    assert_eq!(g.gap, 8.0, "13.3：元素间距 8");
    // 旧期望 16 记的是 G14 偏离（令牌层当时没有 18 档）；令牌补齐后官方值 18 才是对的
    // ——AAR `AssistChipTokens.IconSize = 18.0d`，12b 13.4 也写 18。这不是「把期望改成实现」，
    // 是偏离被取消。
    assert_eq!(
        g.icon, 18.0,
        "13.4：图标 18（官方 AssistChipTokens.IconSize）"
    );
    assert_eq!(
        g.icon,
        unlucky_design::icons::IconSize::Button.px(),
        "chip 图标与按钮图标同源（令牌 IconSize::Button）"
    );
}

/// §18.2 / A1：未选中 Checkbox 在 hover/focus/pressed 下描边提到 `OnSurface`（不是 `OnSurfaceVariant`）。
#[test]
fn checkbox_unselected_lifts_the_outline_to_on_surface() {
    let ui = ui();
    let g = ControlGeometry::checkbox(&ui);
    let at_rest = paint::checkbox(
        &ui,
        Base::CANVAS,
        false,
        false,
        Interaction::Enabled,
        g.radius,
        g.stroke,
    );
    assert_eq!(
        at_rest.frame.outline,
        Some(ui.color(Role::OnSurfaceVariant))
    );
    for state in [
        Interaction::Hovered,
        Interaction::Focused,
        Interaction::Pressed,
    ] {
        let lifted = paint::checkbox(&ui, Base::CANVAS, false, false, state, g.radius, g.stroke);
        assert_eq!(
            lifted.frame.outline,
            Some(ui.color(Role::OnSurface)),
            "A1：{state:?} 的 UnselectedOutlineColor = OnSurface"
        );
        assert_eq!(lifted.frame.stroke, 2.0, "18.8：描边宽度不随态变");
    }
    // 错误态优先于 hover
    let error = paint::checkbox(
        &ui,
        Base::CANVAS,
        false,
        true,
        Interaction::Hovered,
        g.radius,
        g.stroke,
    );
    assert_eq!(error.frame.outline, Some(ui.color(Role::Error)));
}

/// §18.8：禁用不是「变淡变细」——描边仍 2；选中禁用是 `OnSurface`@38% 容器 + `Surface` 勾。
#[test]
fn checkbox_disabled_keeps_the_stroke_and_uses_the_a1_roles() {
    let ui = ui();
    let g = ControlGeometry::checkbox(&ui);
    let unchecked = paint::checkbox(
        &ui,
        Base::CANVAS,
        false,
        false,
        Interaction::Disabled,
        g.radius,
        g.stroke,
    );
    assert_eq!(unchecked.frame.stroke, 2.0, "18.8：禁用描边仍 2px");
    assert_eq!(unchecked.frame.fill, None, "18.8：未选中禁用内部仍透明");
    assert!(
        unchecked
            .frame
            .outline
            .is_some_and(|color| color != ui.color(Role::OnSurfaceVariant)),
        "18.8：禁用描边是 OnSurface@38%，不是满档角色色"
    );

    let checked = paint::checkbox(
        &ui,
        Base::CANVAS,
        true,
        false,
        Interaction::Disabled,
        g.radius,
        g.stroke,
    );
    assert_eq!(
        checked.glyph,
        ui.color(Role::Surface),
        "A1：SelectedDisabledIconColor = Surface"
    );
    // 复审 P2-3：**选中且禁用**的填充是 OnSurface@38%（`SelectedDisabledContainerOpacity`），
    // 不是普通禁用容器的 12%。两格差一眼就能看出来，所以逐字钉住。
    let ink38 = interaction::disabled_content_over_role(&ui, Base::CANVAS, Role::OnSurface, false);
    let container12 = interaction::disabled_colors(&ui, Base::CANVAS).0;
    assert_eq!(
        checked.frame.fill,
        Some(ink38),
        "A1：SelectedDisabledContainerOpacity = 0.38"
    );
    assert_ne!(
        checked.frame.fill,
        Some(container12),
        "选中禁用不是 12% 容器色"
    );
    assert_eq!(
        unchecked.frame.outline,
        Some(ink38),
        "同一行里未选中禁用描边与选中禁用填充是同一个 38% 叠色"
    );
}

/// §19.4：**选中态 hover/pressed 不换环色**（只有状态层变）。
#[test]
fn radio_keeps_primary_on_hover_when_selected() {
    let ui = ui();
    let g = ControlGeometry::radio(&ui);
    let primary = Some(ui.color(Role::Primary));
    for state in Interaction::ALL {
        if state.is_disabled() {
            continue;
        }
        let paint = paint::radio(&ui, Base::CANVAS, true, state, g.radius, g.stroke);
        assert_eq!(
            paint.ring.outline, primary,
            "19.4：{state:?} 环仍是 Primary"
        );
        assert_eq!(paint.dot, primary, "19.4：{state:?} 点仍是 Primary");
    }
    // 未选中：静息 OnSurfaceVariant → hover/focus/pressed 提到 OnSurface
    let at_rest = paint::radio(
        &ui,
        Base::CANVAS,
        false,
        Interaction::Enabled,
        g.radius,
        g.stroke,
    );
    assert_eq!(at_rest.dot, None, "未选中不画内点");
    assert_eq!(at_rest.ring.outline, Some(ui.color(Role::OnSurfaceVariant)));
    let lifted = paint::radio(
        &ui,
        Base::CANVAS,
        false,
        Interaction::Hovered,
        g.radius,
        g.stroke,
    );
    assert_eq!(lifted.ring.outline, Some(ui.color(Role::OnSurface)));
}

/// §10.4 + A1：选中态 hover 的滑块是 `PrimaryContainer`；禁用选中滑块是 `Surface`（不透明度 1）。
#[test]
fn switch_handle_roles_follow_the_tokens() {
    let ui = ui();
    let at_rest = paint::switch(&ui, Base::CANVAS, true, Interaction::Enabled);
    assert_eq!(at_rest.track.fill, Some(ui.color(Role::Primary)));
    assert_eq!(at_rest.thumb, ui.color(Role::OnPrimary));
    assert_eq!(at_rest.icon, Some(ui.color(Role::OnPrimaryContainer)));

    let hovered = paint::switch(&ui, Base::CANVAS, true, Interaction::Hovered);
    assert_eq!(
        hovered.thumb,
        ui.color(Role::PrimaryContainer),
        "A1：选中 hover 滑块 PrimaryContainer"
    );
    assert_eq!(
        hovered.track.fill,
        Some(ui.color(Role::Primary)),
        "轨道不随 hover 变"
    );

    let off = paint::switch(&ui, Base::CANVAS, false, Interaction::Enabled);
    assert_eq!(
        off.track.outline,
        Some(ui.color(Role::Outline)),
        "10.3：未选中 2px Outline"
    );
    assert_eq!(off.track.stroke, 2.0);
    assert_eq!(
        off.thumb,
        ui.color(Role::Outline),
        "10.4：未选中滑块 Outline"
    );
    assert_eq!(off.icon, None, "未选中态图标与轨道同色（不画）");

    let disabled = paint::switch(&ui, Base::CANVAS, true, Interaction::Disabled);
    assert_eq!(
        disabled.thumb,
        ui.color(Role::Surface),
        "A1：DisabledSelectedHandleColor=Surface"
    );
}
