//! 「当前值回显」那一格的可读性（M353 P1-1 的判据）。
//!
//! 起因：M351 的像素实测，`Role::OnPrimaryFixedVariant` 当卡片正文色在**深色档只有 1.80:1**
//! （浅色档 10.87:1）——同一枚令牌两档差 6 倍，深色下几乎看不见。根因是固定角色明暗同值
//! tone 30，它只该配 `PrimaryFixedDim`（tone 80）用（`design::palette::wcag::TEXT_PAIRS`）。
//!
//! | 断言 | 抓得住 | 抓不住 |
//! |---|---|---|
//! | [`the_value_echo_clears_aa_in_both_flavors`] | 把 [`VALUE_ECHO`] 换回任何在卡片底上掉到 4.5:1 以下的令牌（含原来那枚固定角色） | 字重 / 字号是否够大（这两格是 `LabelLarge`，本来就得 4.5）；真图上的抗锯齿边缘 |
//! | [`the_old_fixed_role_is_the_one_that_failed`] | 设计层「固定角色明暗同值」这条性质被改掉时，本条会红——它钉的是那条**因果** | 谁把这枚令牌又搬回这两行（`language.rs` / `theme.rs` 都只写 [`VALUE_ECHO`]，所以换色只有一处） |
//!
//! 为什么住在 `settings/tests/`：`pages/tests/base_wiring.rs` 的接线表按「每个非测试文件里
//! `BASE.role()` 出现几次」记账，而这里只是**量**卡片底、不是画状态层；放在用例文件里才不会
//! 逼着那张表去登记一个不存在的接线（`scan.rs` 模块文档第 3 条：用例文件不参加产品判据）。

use unlucky_design::color::{Color as TokenColor, Role, SchemeFlavor};
use unlucky_design::palette::wcag::AA_TEXT;
use unlucky_design::typography::Platform;

use crate::pages::frame;
use crate::theme::{Families, Ui};

use super::super::general::VALUE_ECHO;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

/// iced 的 sRGB 分量 → 令牌色（[`crate::theme::to_iced`] 的逆；来回一趟无误差）。
fn token(color: iced::Color) -> TokenColor {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    TokenColor::rgba(q(color.r), q(color.g), q(color.b), q(color.a))
}

/// 某枚角色压在卡片底（`frame::BASE`，= `SurfaceContainerLow`）上的对比度。
///
/// `Ui::with_platform` 走的是**出厂色板**（Ink / Silver），也就是产品与出图真正画的那一套；
/// 所以这里的数字与 `DevDocs/artifacts/darkfix/` 的像素实测应当一致（实测见 M353 报告第 2 节）。
fn on_card(flavor: SchemeFlavor, role: Role) -> f32 {
    let ui = Ui::with_platform(flavor, Platform::Windows, FAMILIES);
    token(ui.color(role)).contrast_ratio(token(ui.color(frame::BASE.role())))
}

#[test]
fn the_value_echo_clears_aa_in_both_flavors() {
    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let ratio = on_card(flavor, VALUE_ECHO);
        assert!(
            ratio >= AA_TEXT,
            "{flavor:?} 档的当前值回显只有 {ratio:.2}:1（要 ≥ {AA_TEXT}）——\
             这一格在深色档曾经是 1.80:1，改回固定角色就是这次事故"
        );
    }
}

#[test]
fn the_old_fixed_role_is_the_one_that_failed() {
    let dark = on_card(SchemeFlavor::Dark, Role::OnPrimaryFixedVariant);
    let light = on_card(SchemeFlavor::Light, Role::OnPrimaryFixedVariant);
    assert!(
        dark < AA_TEXT && light >= AA_TEXT,
        "固定角色应当是「浅色档没事、深色档掉线」的那一枚（实测 {light:.2} / {dark:.2}）；\
         两边都过或都不过说明设计层的固定角色语义变了，得重新挑角色"
    );
}
