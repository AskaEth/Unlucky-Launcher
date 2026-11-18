//! 禁用态**合成底**的接线判据（M234；从 `buttons/tests.rs` 拆出，守 400 行硬线）。
//!
//! 判据走真装配路径（`Button::base` → `text_style_spec` → `style_closure`），
//! 不是「自己拼一个 `StyleSpec` 再喂进去」——后者证明不了接线（M229 复审 P1-1 的教训）。

use iced::widget::button;
use iced::Background;

use unlucky_design::color::Role;
use unlucky_design::color::SchemeFlavor;

use super::super::Button;
use super::{light, style_closure, Variant};
use crate::app::components::interaction::Base;
use crate::app::components::surfaces;
use crate::theme::to_iced;

/// **M234 的接线判据**：禁用按钮的容器色必须在**它被告诉的那层底**上合成。
///
/// 走的是**真装配路径**：`Button::base(..)` → `text_style_spec`（接线）→ `style_closure`
/// （求值）。所以这条红了只有两种原因：① `assemble` 没把 `spec.base` 传下去；
/// ② `disabled_button_colors` 内部又写死了画布色。
///
/// 抓得住：卡片/面板里的禁用按钮叠错底（M223 的「CR 1.05、形丢失」就是这么来的）。
/// 抓不住：调用点**忘了**调 `.base(..)`（编译期不报错，默认 `Base::CANVAS`）——那一半由
/// `gallery` 的像素 probe（`ULL_GALLERY_SECTION=disabledbase`）与卡片场景守。
#[test]
fn a_disabled_button_composites_over_the_base_it_is_given() {
    use unlucky_design::color::BrandBoard;

    let ui = light();
    let card = surfaces::elevated::BASE;

    let spec = super::super::assemble::text_style_spec(
        &ui,
        Variant::Filled,
        &Button::<u8>::disabled("卡片里"),
    );
    assert_eq!(
        spec.base,
        Base::CANVAS,
        "不调 `.base(..)` 时是画布底（默认值，语义写在 `Base::CANVAS` 的文档里）"
    );

    let card_spec = super::super::assemble::text_style_spec(
        &ui,
        Variant::Filled,
        &Button::<u8>::disabled("卡片里").base(card),
    );
    assert_eq!(
        card_spec.base, card,
        "接线：`Button::base` 必须原样进 `StyleSpec`"
    );

    let card_style = style_closure(&ui, card_spec)(&ui.theme(), button::Status::Disabled);
    let canvas_style = style_closure(&ui, spec)(&ui.theme(), button::Status::Disabled);

    // 期望值独立算（写死 0.10 + 直接取色板角色），不调被测入口。
    let scheme = BrandBoard::for_flavor(SchemeFlavor::Light).scheme();
    let expected_card = to_iced(
        scheme
            .get(Role::OnSurface)
            .with_alpha(0.10)
            .over(scheme.get(Role::SurfaceContainer)),
    );
    assert_eq!(
        card_style.background,
        Some(Background::Color(expected_card)),
        "卡片里的禁用 Filled 按钮容器 = on-surface@10% 叠 **SurfaceContainer**"
    );
    assert_ne!(
        card_style.background, canvas_style.background,
        "卡片底与画布底的合成结果必须不同（叠在画布上就是 M234 要修的那条）"
    );
}
