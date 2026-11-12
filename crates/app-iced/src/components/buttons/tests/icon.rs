//! 图标按钮的单测（M229 复审 P1-1 从 `buttons/tests.rs` 拆出，守 400 行硬线）。
//!
//! 拆出来的只有「四式图标按钮的描边」这一组：`icon()` 曾经把 `StyleSpec.stroke` 写死 `0.0`，
//! 于是 `Outlined` 与 `Plain` 逐像素相同——判据在这里，渲染级的那一半在 `gallery/probe/rows.rs`。

use super::*;

/// **M229 复审 P1-1 的回归门**：四种图标按钮里**只有 `Outlined` 有描边**。
///
/// 抓得住：`icon()` 里把 `stroke` 写死成 0（那样 Outlined 与 Plain 逐像素相同——正是复审量到的现场）。
/// 抓不住：描边颜色对不对（那是 `IconVariant::roles` 的事，上面已有变体表测试）。
///
/// 渲染级的第二道在 `components/gallery/probe.rs`（`buttons` 段的 sheet 上数 Outlined 行的环像素、
/// 并断言 Plain 行没有环），这一条是纯函数的、不依赖出图。
#[test]
fn only_the_outlined_icon_variant_draws_a_ring() {
    use super::super::{icon_stroke, icon_style_spec};

    assert_eq!(
        icon_stroke(IconVariant::Outlined),
        crate::theme::STROKE_PX,
        "Outlined 必须画 1px 环（否则与 Plain 逐像素相同）"
    );
    assert_eq!(
        icon_stroke(IconVariant::Outlined),
        1.0,
        "全站描边 1px（rubric G7）"
    );
    for plain in [IconVariant::Plain, IconVariant::Filled, IconVariant::Tonal] {
        assert_eq!(icon_stroke(plain), 0.0, "{plain:?} 不该有描边");
    }
    // 与角色表同源：有描边角色 ⇔ 有环。
    for variant in IconVariant::ALL {
        assert_eq!(
            icon_stroke(variant) > 0.0,
            variant.roles().2.is_some(),
            "{variant:?} 的描边与角色表不一致"
        );
        // **接线断言**：`icon()` 真正塞进 `style_closure` 的那个结构体（`icon_style_spec`）。
        // 只测 `icon_stroke()` 抓不住「调用点写死 0」——第一版就是这样漏的（变异测试证明过）。
        let spec = icon_style_spec(variant, 40.0, None, Base::CANVAS);
        assert_eq!(
            spec.stroke,
            icon_stroke(variant),
            "{variant:?}：icon() 用的描边宽度与角色表不一致（调用点被写死了？）"
        );
        assert_eq!(
            spec.border.is_some(),
            icon_stroke(variant) > 0.0,
            "{variant:?}：描边角色与线宽必须同生同灭"
        );
    }
}
