//! FAB 的几何断言（四档尺寸/圆角/图标/内距，全部对着规格卡原值）。
//!
//! 抓得住：档位与规格卡错位（例如把 Default 写成 48）、圆角被写成半高、扩展态内距悄悄改成 M3 的 16。
//! 抓不住：图上好不好看（D30），以及 hover/pressed 的状态层（那走 `interaction` 的既有测试）。
//!
//! 配色只钉**一条**（禁用内容色，M234 复审 P2-2）：FAB 没有禁用容器档，所以 38% 必须合在
//! 自己的 `PrimaryContainer` 上，而不是画布色。其余配色仍由 `interaction` 的测试覆盖。

use unlucky_design::color::{Role, SchemeFlavor};
use unlucky_design::inkcat::size::FabSize;
use unlucky_design::typography::Platform;

use super::super::super::interaction::Interaction;
use crate::theme::{to_iced, Families, Ui};

fn ui(flavor: SchemeFlavor) -> Ui {
    Ui::with_platform(
        flavor,
        Platform::Windows,
        Families {
            text: "Segoe UI",
            display: "Segoe UI",
            mono: "Consolas",
        },
    )
}

#[test]
fn the_four_states_are_the_spec_sizes() {
    for (size, width, height, radius) in [
        (FabSize::Small, 40.0, 40.0, 12.0),
        (FabSize::Default, 56.0, 56.0, 16.0),
        (FabSize::Medium, 80.0, 80.0, 20.0),
        (FabSize::Extended, 160.0, 56.0, 16.0),
    ] {
        assert_eq!(size.width_px(), width, "{size:?} 宽");
        assert_eq!(size.height_px(), height, "{size:?} 高");
        assert_eq!(size.radius_px(), radius, "{size:?} 圆角");
    }
    assert_eq!(FabSize::ALL.len(), 4);
}

#[test]
fn only_the_extended_state_carries_a_label_and_its_inset() {
    for size in [FabSize::Small, FabSize::Default, FabSize::Medium] {
        assert_eq!(size.label_px(), 0.0, "{size:?} 不该有标签");
        assert_eq!(size.pad_start_px(), 0.0);
    }
    assert_eq!(FabSize::Extended.label_px(), 15.0);
    assert_eq!(FabSize::Extended.pad_start_px(), 28.0);
    assert_eq!(FabSize::Extended.gap_px(), 8.0);
}

/// 图标随档放大（20 / 26 / 34），扩展态回到 24（它与标签同行，跟着文字尺度走）。
#[test]
fn the_glyph_grows_with_the_smaller_states() {
    assert!(FabSize::Small.icon_px() < FabSize::Default.icon_px());
    assert!(FabSize::Default.icon_px() < FabSize::Medium.icon_px());
    assert_eq!(FabSize::Extended.icon_px(), 24.0);
}

/// 扩展态右内距是常量（不是散在 `Padding` 里的字面量 28），且与起始内距对称。
#[test]
fn the_extended_pad_end_is_a_named_constant() {
    use super::super::fab::EXTENDED_PAD_END;
    use unlucky_design::inkcat::size::FabSize;

    assert_eq!(EXTENDED_PAD_END, 28.0);
    assert_eq!(
        EXTENDED_PAD_END,
        FabSize::Extended.pad_start_px(),
        "左右对称：规格卡只量到起始 28，右侧取同值"
    );
}

/// **M234 复审 P2-2 的判据**：禁用 FAB 的内容色必须合在**自己的容器**上。
///
/// 走的是真实现（`fab::content_color`，`button()` 与样式闭包共用它）。抓得住：底又改回画布
/// （那就是修前那块与 FAB 底无关的灰）、或有人把 `on_container` 打开（会多算一层
/// 10% 的 `OnSurface` 灰）。抓不住：容器本身该不该在禁用时保持满档 `PrimaryContainer`
/// （那是 M3 FAB token 的口径问题，不由本测试裁决）。
#[test]
fn disabled_fab_ink_composites_over_its_own_container() {
    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let ui = ui(flavor);
        let scheme = unlucky_design::color::BrandBoard::for_flavor(flavor).scheme();
        // 期望值独立算：38% 的 OnPrimaryContainer 叠在满档 PrimaryContainer 上
        let expected = to_iced(
            scheme
                .get(Role::OnPrimaryContainer)
                .with_alpha(0.38)
                .over(scheme.get(Role::PrimaryContainer)),
        );
        let actual = super::content_color(&ui, Some(Interaction::Disabled));
        assert_eq!(
            actual, expected,
            "{flavor:?}：禁用 FAB 的内容色 = on-primary-container@38% 叠 PrimaryContainer"
        );

        // 反向：不许等于「叠画布」那一档（修前的错值）
        let over_canvas = to_iced(
            scheme
                .get(Role::OnPrimaryContainer)
                .with_alpha(0.38)
                .over(scheme.get(Role::Surface)),
        );
        assert_ne!(actual, over_canvas, "{flavor:?}：又合到画布上了");

        // 非禁用档不受影响
        assert_eq!(
            super::content_color(&ui, Some(Interaction::Enabled)),
            ui.color(Role::OnPrimaryContainer)
        );
    }
}
