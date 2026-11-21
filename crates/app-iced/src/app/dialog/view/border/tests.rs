//! 判据：**面板带 α 时不预合成 rim**；α = 1.0 时与 M444 那一句**逐位相同**。
//!
//! ## 抓得住 / 抓不住
//!
//! | 用例 | 抓得住 | 抓不住 |
//! |---|---|---|
//! | [`a_translucent_panel_is_not_precomposited`] | 降级分支被改回「无条件预合成」（`width` 从 0 变 1 ⇒ 红） | 那块面**看起来**对不对（边是不是真没了、硬边是不是真消失）——只能出图，见 `DevDocs/artifacts/m449/` |
//! | [`an_opaque_panel_still_gets_the_m444_rim`] | α=1.0 路径的接线漂了（与 `panel_border` 的返回值不再逐位相等 ⇒ 红） | 个别像素：`Border` 相等 ⇒ 渲染指令相同，但「渲染器把同一组指令画成同一片像素」本身不由这里举 |
//!
//! 两条一起看才有意义：只有第一条会红的话，「一刀切不画边」也能全绿（卡片永久没边）；
//! 只有第二条会红的话，半透明底上的硬边会漏过去。

use unlucky_design::color::SchemeFlavor;
use unlucky_design::component_map::Component;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::typography::Platform;

use crate::theme::Families;

use super::*;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

fn ui(flavor: SchemeFlavor) -> Ui {
    Ui::with_platform(flavor, Platform::Windows, FAMILIES)
}

/// 卡片真实的圆角档（产品路径取的就是它，别在这里写死一个刻度）。
fn shape() -> ShapeScale {
    Component::Dialog.spec().shape
}

/// **M449 的降级**：面板实际色带 α 时，卡片边界不许预合成 rim。
///
/// 变异原文（改一处）：把 [`card_border`] 的函数体替换成无条件那一句
/// `panel_border(ui, DIALOG_TIER, ui.color(MaterialLayer::Overlay.surface_role()), radius)`
/// （= M444 的原文）⇒ 期望红在本文件第一条 `assert_eq!(got.width, 0.0, …)`。
#[test]
fn a_translucent_panel_is_not_precomposited() {
    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let gpu = ui(flavor).with_panel_alpha(0.55);
        let panel = gpu.layer(MaterialLayer::Overlay);
        assert!(
            panel.a < 1.0,
            "{flavor:?}：面板 α=0.55 必须真的带 α（实际 a={}），否则这条用例什么都没测",
            panel.a
        );

        let got = card_border(&gpu, panel, shape());
        assert_eq!(
            got.width, 0.0,
            "{flavor:?}：α<1 时不许预合成 rim —— 显式降级成只留圆角"
        );
        assert_eq!(
            got.radius,
            gpu.radius_dp(shape()),
            "{flavor:?}：降级只换材质、不换几何（§1.6 的同一口径）"
        );
    }
}

/// **α = 1.0 那一档必须一个像素都不变**：边界仍是 M444 的 S4 rim，且逐位等于原来那一句。
///
/// 结构依据（这条用例断言的就是它）：
/// `Ui::layer(Overlay)` 在面板 α = 1.0 时是 `color(surface_role()).scale_alpha(1.0)`，
/// 而 iced 的 `scale_alpha` 是 `a * factor` ⇒ `a * 1.0 == a` **逐位相同**；
/// 于是 `card_border` 收到的 `panel` 与 M444 硬传的 `ui.color(..surface_role())` 是同一个值，
/// 渲染树（`container::Style { background, border, .. }`）其余字段一个没动。
///
/// 变异原文（改一处）：把 `card_border` 的降级条件写成 `panel.a <= 1.0`
/// ⇒ 期望红在本文件 `assert!(got.width > 0.0, …)`（α=1.0 档被误伤成没边）。
#[test]
fn an_opaque_panel_still_gets_the_m444_rim() {
    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let gpu = ui(flavor);
        let panel = gpu.layer(MaterialLayer::Overlay);

        // 前提：出厂面板 α = 1.0，且 `layer()` 退化成的就是角色色本身（逐位）。
        assert_eq!(panel.a, 1.0, "{flavor:?}：出厂面板 α = 1.0");
        assert_eq!(
            panel,
            gpu.color(MaterialLayer::Overlay.surface_role()),
            "{flavor:?}：α=1.0 时 `layer()` 必须逐位等于角色色（`scale_alpha(1.0)` 是不动点）"
        );

        let got = card_border(&gpu, panel, shape());
        assert!(
            got.width > 0.0,
            "{flavor:?}：α=1.0 仍要画出 S4 rim（否则降级变成一刀切，卡片永久没边）"
        );
        assert_eq!(
            got,
            panel_border(
                &gpu,
                DIALOG_TIER,
                gpu.color(MaterialLayer::Overlay.surface_role()),
                gpu.radius_dp(shape()),
            ),
            "{flavor:?}：α=1.0 路径必须逐位等于 M444 那一句 —— 那一档的渲染树没变"
        );
    }
}
