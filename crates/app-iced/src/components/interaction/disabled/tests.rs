//! 禁用态两族 α 与合成色的单测（M229 复审 F1 后从 `disabled.rs` 纯搬移，守 400 行硬线）。
//!
//! 搬移之后**内容有改动**（不是一行未改）：既有两条「α 族」测试的期望值从 `Scheme::baseline`
//! 改成**品牌板**（F1 的修法就是换色源），并新增 `disabled_colors_follow_the_brand_board_not_the_tone_baseline`
//! 作为 F1 的回归门——它与两条族测试放在一起，读的人一眼能看到「族」与「色源」两件事。

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

/// 两族的 α 必须都钉住（只钉一条，另一族被合并时会静默错 1/255）。
#[test]
fn both_disabled_alpha_classes_are_pinned_to_the_tokens() {
    // 绑定成局部变量再过断言：直接比两个常量会被 clippy 判成「常量断言」。
    let button_alpha = DISABLED_CONTAINER_ALPHA;
    let surface_alpha = DISABLED_SURFACE_ALPHA;
    let content_alpha = DISABLED_CONTENT_ALPHA;
    // 按钮容器：官方 *ButtonTokens.DisabledContainerOpacity = 0.1f
    assert!(
        (button_alpha - 0.10).abs() < f32::EPSILON,
        "按钮禁用容器 α 必须是 0.10（官方 DisabledContainerOpacity）；\
         0.12 是面状控件那一类，别混用"
    );
    // 面状控件：chip 描边 / slider 未激活轨 / switch 轨道 = 0.12f
    assert!(
        (surface_alpha - 0.12).abs() < f32::EPSILON,
        "面状控件禁用 α 必须是 0.12（官方 FlatDisabledOutlineOpacity / \
         DisabledInactiveTrackOpacity / DisabledTrackOpacity）"
    );
    assert!((content_alpha - 0.38).abs() < f32::EPSILON);
    assert!(
        surface_alpha > button_alpha,
        "两族必须可区分：面状控件比按钮容器更实（{surface_alpha} > {button_alpha}）"
    );
}

/// **面状控件族**的那一对颜色 = 0.12 预混合（chip / switch / slider / TextField 描边）。
///
/// ⚠️ 期望值**写死 0.12 / 0.38**，不从 `DISABLED_SURFACE_ALPHA` 取：
/// 从被测常量求期望是**自指断言**——常量一改，expected 跟着改，永远相等（复审 r1 P2-1 实测：
/// 把 `DISABLED_SURFACE_ALPHA` 改成 0.10，旧版这条照样绿）。现在常量改坏必红。
#[test]
fn surface_family_disabled_colors_are_the_12_percent_class() {
    const EXPECTED_SURFACE_ALPHA: f32 = 0.12;
    const EXPECTED_CONTENT_ALPHA: f32 = 0.38;
    let actual_alpha = DISABLED_SURFACE_ALPHA;
    assert_eq!(
        actual_alpha, EXPECTED_SURFACE_ALPHA,
        "面状控件族 α 必须是官方 0.12f；改常量会让下面每一条合成色断言同时红"
    );

    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let ui = ui(flavor);
        // 期望值按**品牌色板**（= `Ui` 注入的那一份），不是 tone 派生的 baseline。
        let scheme = unlucky_design::color::BrandBoard::for_flavor(flavor).scheme();
        let base = scheme.get(Role::Surface);
        let expected_container = scheme
            .get(Role::OnSurface)
            .with_alpha(EXPECTED_SURFACE_ALPHA)
            .over(base);

        let (surface, surface_content) = disabled_colors(&ui, Base::CANVAS);
        assert_eq!(
            surface,
            to_iced(expected_container),
            "{flavor:?}：面状控件容器 = on-surface@12% 叠表面（期望值硬编码，不取自常量）"
        );
        assert_eq!(
            surface_content,
            to_iced(
                scheme
                    .get(Role::OnSurfaceVariant)
                    .with_alpha(EXPECTED_CONTENT_ALPHA)
                    .over(expected_container)
            ),
            "{flavor:?}：面状控件内容仍是 38%，只是叠的底不同"
        );
        assert_ne!(surface, ui.color(Role::Surface), "12% 叠上去必须能看出来");
        assert_eq!(surface.a, 1.0, "禁用容器也是实色（CPU/GPU 一致）");
        assert_eq!(surface_content.a, 1.0, "禁用内容也是实色");
        // 内容叠在容器上 vs 叠在表面上，是两回事（probe 量的是前者）
        assert_ne!(
            surface_content,
            to_iced(
                scheme
                    .get(Role::OnSurfaceVariant)
                    .with_alpha(EXPECTED_CONTENT_ALPHA)
                    .over(base)
            ),
            "{flavor:?}：内容必须叠在容器上"
        );
        // 跨族对照：面状族的合成色必须与按钮族不同（两族一旦合流，这条红）
        let (button, _) = disabled_button_colors(&ui, Base::CANVAS);
        assert_ne!(
            surface, button,
            "{flavor:?}：面状族 12% 与按钮族 10% 必须落在不同像素值上"
        );
    }
}

/// **按钮家族**的那一对颜色 = 0.10 预混合，且与面状控件族**必须落在不同像素值上**。
///
/// 期望值同样写死（0.10 / 0.38），理由见上一条：自指断言抓不住常量被改坏。
#[test]
fn button_family_disabled_colors_are_the_10_percent_class() {
    const EXPECTED_BUTTON_ALPHA: f32 = 0.10;
    const EXPECTED_CONTENT_ALPHA: f32 = 0.38;
    let actual_alpha = DISABLED_CONTAINER_ALPHA;
    assert_eq!(
        actual_alpha, EXPECTED_BUTTON_ALPHA,
        "按钮族 α 必须是官方 0.1f（*ButtonTokens.DisabledContainerOpacity）"
    );

    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let ui = ui(flavor);
        // 期望值按**品牌色板**（= `Ui` 注入的那一份），不是 tone 派生的 baseline。
        let scheme = unlucky_design::color::BrandBoard::for_flavor(flavor).scheme();
        let base = scheme.get(Role::Surface);
        let expected_container = scheme
            .get(Role::OnSurface)
            .with_alpha(EXPECTED_BUTTON_ALPHA)
            .over(base);

        let (button, button_content) = disabled_button_colors(&ui, Base::CANVAS);
        assert_eq!(
            button,
            to_iced(expected_container),
            "{flavor:?}：按钮容器 = on-surface@10% 叠表面（官方 DisabledContainerOpacity = 0.1f）"
        );
        assert_eq!(
            button_content,
            to_iced(
                scheme
                    .get(Role::OnSurfaceVariant)
                    .with_alpha(EXPECTED_CONTENT_ALPHA)
                    .over(expected_container)
            ),
        );

        let (surface, _) = disabled_colors(&ui, Base::CANVAS);
        assert_ne!(
            button, surface,
            "{flavor:?}：10% 与 12% 必须落在不同像素值上（合成一个数就是这次要防的回归）"
        );

        // 内容色同理：按钮 38% 叠 10% 底 ≠ 面状 38% 叠 12% 底
        assert_ne!(
            button_content,
            disabled_content_over_role(&ui, Base::CANVAS, Role::OnSurfaceVariant, true),
            "{flavor:?}：两族的「有容器」内容色不能同源"
        );
        assert_eq!(
            disabled_button_content(&ui, Base::CANVAS, Role::OnSurfaceVariant, true),
            button_content,
            "{flavor:?}：按钮内容入口必须与按钮颜色入口同源"
        );
        // 无容器时两族完全一致（都叠 surface）——差异只在「有容器」那一支
        assert_eq!(
            disabled_button_content(&ui, Base::CANVAS, Role::OnSurfaceVariant, false),
            disabled_content_over_role(&ui, Base::CANVAS, Role::OnSurfaceVariant, false),
            "{flavor:?}：没有容器时不该有族差异"
        );
    }
}

/// **M229 复审 F1 的回归门**：禁用态的合成必须跟着**品牌色板**走，不许偷偷回到 tone 派生的 baseline。
///
/// Ink 板的中性色是绿系（`on-surface` 与 `surface` 的 G 通道最高），baseline 是蓝系；所以只要
/// 「禁用容器」在 Ink 下是绿系（`g >= b`）且与 baseline 算出来的值**不相等**，就说明它走的是板。
///
/// 抓得住：有人把三处 `Scheme::baseline(ui.flavor())` 写回去（那时本测试立刻红，且报出偏蓝的像素）。
/// 抓不住：色板本身改了色相（那是 `design` 的 `anchors_pin_the_spec` 的事）。
#[test]
fn disabled_colors_follow_the_brand_board_not_the_tone_baseline() {
    use unlucky_design::color::BrandBoard;

    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let ui = ui(flavor);
        let board = BrandBoard::for_flavor(flavor).scheme();
        let baseline = Scheme::baseline(flavor);

        // ① 面状控件族的容器（chip 描边 / slider 未激活轨 / switch 轨道）
        let (surface, _surface_content) = disabled_colors(&ui, Base::CANVAS);
        let expect_surface = to_iced(
            board
                .get(Role::OnSurface)
                .with_alpha(DISABLED_SURFACE_ALPHA)
                .over(board.get(Role::Surface)),
        );
        assert_eq!(
            surface, expect_surface,
            "{flavor:?}：禁用容器必须按色板合成"
        );

        let baseline_surface = to_iced(
            baseline
                .get(Role::OnSurface)
                .with_alpha(DISABLED_SURFACE_ALPHA)
                .over(baseline.get(Role::Surface)),
        );
        assert_ne!(
            surface, baseline_surface,
            "{flavor:?}：禁用容器退回了 baseline（这就是复审 F1 的蓝系像素）"
        );

        // ② 按钮族的容器
        let (button, _) = disabled_button_colors(&ui, Base::CANVAS);
        assert_ne!(button, surface, "{flavor:?}：两族 α 仍要可区分");

        // ③ 内容色（两个入口）
        assert_eq!(
            disabled_content_over_role(&ui, Base::CANVAS, Role::OnSurfaceVariant, true),
            to_iced(
                board
                    .get(Role::OnSurfaceVariant)
                    .with_alpha(DISABLED_CONTENT_ALPHA)
                    .over(
                        board
                            .get(Role::OnSurface)
                            .with_alpha(DISABLED_SURFACE_ALPHA)
                            .over(board.get(Role::Surface))
                    )
            ),
            "{flavor:?}：面状控件族内容色要按色板合成"
        );
        assert_eq!(
            disabled_button_content(&ui, Base::CANVAS, Role::OnSurfaceVariant, true),
            to_iced(
                board
                    .get(Role::OnSurfaceVariant)
                    .with_alpha(DISABLED_CONTENT_ALPHA)
                    .over(
                        board
                            .get(Role::OnSurface)
                            .with_alpha(DISABLED_CONTAINER_ALPHA)
                            .over(board.get(Role::Surface))
                    )
            ),
            "{flavor:?}：按钮族内容色要按色板合成"
        );

        // ④ **色相判据**（像素级的自证）：Ink 板的禁用容器是绿系（G 最高），baseline 是蓝系（B 最高）。
        //    Ink 板：`on-surface` 与 `surface` 的 G 通道都高于 B 通道（两个中性角色都是绿黑），
        //    合成后仍是 G ≥ B；baseline 深色则是 B > G（天蓝中性）。浅色板上两者都是近中性，
        //    所以只在深色档下判色相。
        if matches!(flavor, SchemeFlavor::Dark) {
            let (r, g, b) = channels(surface);
            assert!(
                g >= b,
                "{flavor:?}：禁用容器 {r}/{g}/{b} 仍是蓝系（G < B），说明没走 Ink 板"
            );
            let (_, bg, bb) = channels(baseline_surface);
            assert!(bb > bg, "对照组不合格：baseline 深色的禁用容器应当是蓝系");
        }
    }
}

/// **M234 的主判据**：禁用态的合成必须用**给的底**，不是画布色。
///
/// 卡片（`SurfaceContainer`）与面板（`SurfaceContainerLow`）里各放一个禁用控件时，
/// 10%（按钮族）/ 12%（面状族）那一层要叠在**卡/面板的底**上；叠在 `Surface` 上就是 M223
/// 记的「CR 1.05、形丢失」。
///
/// 抓得住：① 入口把 `base` 丢了（内部又写死 `Role::Surface`）；② 入口把两个族的 α 合成一个数；
/// ③ 调用点给了画布底（这条测试只证「给了卡片底 → 算出卡片色」，调用点是否真的给了由
/// `buttons::tests::a_disabled_button_composites_over_the_base_it_is_given` 与 gallery 的像素
/// probe 分别守）。
///
/// 抓不住：① 容器画的是**合成色**（半透明面板 α、叠过状态层的底）——`Base` 只装一个 `Role`；
/// ② 卡/面板组件**画底时**用的角色与它导出的 `BASE` 不一致（`BASE` 的值本身由
/// `surfaces::elevated::tests::the_card_base_constant_is_the_role_the_card_paints` 钉住，
/// 「card() 真的用了它」与「页面里的调用点传了它对不对」由 `gallery::probe` 的 disabledbase
/// 段守）；③ GPU(wgpu) 档（本测试是纯值，两档同源同值）。
#[test]
fn disabled_paint_composites_over_the_base_it_is_given() {
    use unlucky_design::color::BrandBoard;
    use unlucky_design::color::Role;

    const BUTTON_ALPHA: f32 = 0.10;
    const SURFACE_ALPHA: f32 = 0.12;
    const CONTENT_ALPHA: f32 = 0.38;

    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let ui = ui(flavor);
        let scheme = BrandBoard::for_flavor(flavor).scheme();
        let card = Base::of(Role::SurfaceContainer);
        let panel = Base::of(Role::SurfaceContainerLow);

        // 期望值一律**独立算**（写死 α + 直接取色板角色），不调被测常量。
        let over = |base: Role, alpha: f32| {
            to_iced(
                scheme
                    .get(Role::OnSurface)
                    .with_alpha(alpha)
                    .over(scheme.get(base)),
            )
        };

        // ① 按钮族 10%：卡片底 / 面板底 / 画布底三档互不相等
        let (card_button, card_button_ink) = disabled_button_colors(&ui, card);
        let (panel_button, _) = disabled_button_colors(&ui, panel);
        let (canvas_button, canvas_button_ink) = disabled_button_colors(&ui, Base::CANVAS);
        assert_eq!(
            card_button,
            over(Role::SurfaceContainer, BUTTON_ALPHA),
            "{flavor:?}：卡片里的禁用按钮容器 = on-surface@10% 叠**卡片底**"
        );
        assert_eq!(
            panel_button,
            over(Role::SurfaceContainerLow, BUTTON_ALPHA),
            "{flavor:?}：面板里的禁用按钮容器 = on-surface@10% 叠**面板底**"
        );
        assert_ne!(card_button, canvas_button, "{flavor:?}：卡片底 ≠ 画布底");
        assert_ne!(panel_button, canvas_button, "{flavor:?}：面板底 ≠ 画布底");
        assert_ne!(card_button, panel_button, "{flavor:?}：卡片底 ≠ 面板底");

        // ② 面状控件族 12%（字段容器 / chip 容器 / switch 轨道）
        let (card_surface, _) = disabled_colors(&ui, card);
        let (panel_surface, _) = disabled_colors(&ui, panel);
        let (canvas_surface, _) = disabled_colors(&ui, Base::CANVAS);
        assert_eq!(
            card_surface,
            over(Role::SurfaceContainer, SURFACE_ALPHA),
            "{flavor:?}：卡片里的禁用字段容器 = on-surface@12% 叠**卡片底**"
        );
        assert_eq!(
            panel_surface,
            over(Role::SurfaceContainerLow, SURFACE_ALPHA),
            "{flavor:?}：面板里的禁用字段容器 = on-surface@12% 叠**面板底**"
        );
        assert_ne!(card_surface, canvas_surface);
        assert_ne!(panel_surface, canvas_surface);

        // ③ 内容色也跟着底走（38% 叠在容器上 / 直接叠在底上，两支都要换底）
        assert_eq!(
            disabled_button_content(&ui, card, Role::OnSurfaceVariant, true),
            to_iced(
                scheme
                    .get(Role::OnSurfaceVariant)
                    .with_alpha(CONTENT_ALPHA)
                    .over(
                        scheme
                            .get(Role::OnSurface)
                            .with_alpha(BUTTON_ALPHA)
                            .over(scheme.get(Role::SurfaceContainer))
                    )
            ),
            "{flavor:?}：有容器的内容色 = 38% 叠(10% 叠卡片底)"
        );
        assert_ne!(
            card_button_ink, canvas_button_ink,
            "{flavor:?}：内容叠的容器换了底"
        );
        assert_ne!(
            disabled_content_over_role(&ui, card, Role::OnSurface, false),
            disabled_content_over_role(&ui, Base::CANVAS, Role::OnSurface, false),
            "{flavor:?}：无容器时 38% 也必须叠在**给的底**上"
        );
    }
}

/// iced 颜色 → 8bit 通道（像素值，报告里的证据就用它）。
fn channels(color: Color) -> (u8, u8, u8) {
    (
        (color.r * 255.0).round() as u8,
        (color.g * 255.0).round() as u8,
        (color.b * 255.0).round() as u8,
    )
}
