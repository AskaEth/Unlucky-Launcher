//! field 的单测（从 `field.rs` 拆出：单文件 ≤400 行，AGENTS.md §3-1）。

use super::*;
use unlucky_design::color::SchemeFlavor;
use unlucky_design::typography::Platform;

use crate::theme::Families;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

fn ui(flavor: SchemeFlavor) -> Ui {
    Ui::with_platform(flavor, Platform::Windows, FAMILIES)
}

fn build(field: Field<'_>, flavor: SchemeFlavor) -> Element<'_, u8> {
    let ui = Box::leak(Box::new(ui(flavor)));
    text_field(ui, field, |_| 0_u8)
}

#[test]
fn every_variant_and_state_builds() {
    for variant in FieldVariant::ALL {
        for state in Interaction::ALL {
            for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
                let field = Field::new("实例一")
                    .variant(variant)
                    .label("名称")
                    .placeholder("请输入")
                    .supporting("辅助说明")
                    .leading(IconKind::Folder)
                    .trailing(IconKind::Close)
                    .state(state);
                let _: Element<'_, u8> = build(field, flavor);
            }
        }
    }
}

/// 9.4：聚焦 2dp `Primary`、hover 1dp `OnSurface`、静息 Outlined 1dp `Outline`（Filled 用 OnSurfaceVariant）。
#[test]
fn outline_colors_and_widths_follow_the_state() {
    let ui = ui(SchemeFlavor::Light);
    let rest = palette(&ui, FieldVariant::Outlined, Interaction::Enabled, false);
    assert_eq!(rest.line, ui.color(Role::Outline));
    assert_eq!(rest.stroke, 1.0);
    assert_eq!(rest.fill, None, "Outlined 无容器色");

    let hover = palette(&ui, FieldVariant::Outlined, Interaction::Hovered, false);
    assert_eq!(hover.line, ui.color(Role::OnSurface));
    assert_eq!(hover.stroke, 1.0);

    let focus = palette(&ui, FieldVariant::Outlined, Interaction::Focused, false);
    assert_eq!(focus.line, ui.color(Role::Primary));
    assert_eq!(focus.stroke, 2.0, "FocusOutlineWidth = 2dp");
    assert_eq!(focus.label, ui.color(Role::Primary), "FocusLabelColor");

    // A1 没有 Pressed token：按 hover 处理（D11），不是漏做
    assert_eq!(
        palette(&ui, FieldVariant::Outlined, Interaction::Pressed, false),
        hover
    );
}

/// 9.7：错误态线/标签/辅助全 `Error`，线宽 2dp；错误优先于聚焦。
#[test]
fn error_beats_focus_and_paints_everything_error() {
    let ui = ui(SchemeFlavor::Light);
    let error = palette(&ui, FieldVariant::Outlined, Interaction::Focused, true);
    assert_eq!(error.line, ui.color(Role::Error));
    assert_eq!(error.label, ui.color(Role::Error));
    assert_eq!(error.supporting, ui.color(Role::Error));
    assert_eq!(error.stroke, 2.0, "错误线宽 2dp");
    assert_eq!(error.input, ui.color(Role::OnSurface), "ErrorInputColor");
}

/// 9.8：禁用内容 38%、Outlined 描边 12%、Filled 容器 12%（官方 4%，令牌缺口 → D10）。
#[test]
fn disabled_uses_the_token_alphas() {
    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let ui = ui(flavor);
        let container = interaction::disabled_colors(&ui, Role::Surface).0;

        let filled = palette(&ui, FieldVariant::Filled, Interaction::Disabled, false);
        assert_eq!(filled.fill, Some(container));
        assert_eq!(
            filled.input,
            interaction::disabled_content_over_role(&ui, Role::OnSurface, true)
        );

        let outlined = palette(&ui, FieldVariant::Outlined, Interaction::Disabled, false);
        assert_eq!(outlined.fill, None);
        assert_eq!(outlined.line, container, "DisabledOutlineOpacity = 0.12");
        assert_eq!(
            outlined.input,
            interaction::disabled_content_over_role(&ui, Role::OnSurface, false)
        );
        assert_ne!(outlined.input, ui.color(Role::OnSurface), "禁用不许满档");
    }
}

/// Filled 有容器色、Outlined 没有；两者圆角都取 `Component::TextField`（4）。
#[test]
fn filled_carries_the_container_role() {
    let ui = ui(SchemeFlavor::Light);
    assert_eq!(
        palette(&ui, FieldVariant::Filled, Interaction::Enabled, false).fill,
        Some(ui.color(Role::SurfaceContainerHighest)),
        "FilledTextFieldTokens.ContainerColor"
    );
    assert_eq!(FieldGeometry::of(&ui, true, None).shape.dp(), 4.0);
}
