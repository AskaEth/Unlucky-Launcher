//! 角色语义 + 「可主题化」断言（M248：Elevated 底 / Outlined 描边）。
//!
//! **判据不是「等于某个 hex」**：品牌色板本来就会被换（D38 的取色方案会整表接管 49 个角色），
//! 钉死色值等于把「配色可自定义」这条路焊上。所以这里钉两件互相补位的事：
//!
//! 1. [`role_table_is_named_roles_not_colours`]：`Variant::roles()` 吐的是**角色名**；Elevated 的
//!    容器还必须由**高程令牌**推导（`Elevation::Level1.surface_role()`），不许在组件里手抄一遍；
//! 2. [`styles_follow_the_scheme_across_boards_and_custom_accents`]：同一变体在 Ink / Silver /
//!    自定义种子方案（`Scheme::from_seed`）三套方案下，`button::Style` 里解出的颜色**分别等于
//!    各自方案的该角色色值**，且三者互不相同。把「取角色」改成「取死值（`Role::Surface`）」
//!    或「写死某套方案」都会被这条打红。
//!
//! 抓不住什么：
//! - **令牌表里两个角色同值**时，换角色的变异只有第 1 条会红；
//! - **像素**：角色写对了、图上却是别的颜色（样式闭包被别处覆盖）——那是 `gallery/probe` 与出图的事。

use iced::widget::button;
use iced::Background;

use unlucky_design::color::{BrandBoard, Role, Scheme, SchemeFlavor};
use unlucky_design::elevation::Elevation;
use unlucky_design::typography::Platform;

use super::super::{style_closure, IconVariant, Variant};
use super::spec_of;
use crate::theme::{Ui, STROKE_PX};

/// 三套配色来源：两块出厂品牌板 + 一套 `Scheme::from_seed` 自定义种子方案。
///
/// 第三套是关键：品牌板是固化数据，**换不了**；只有「同一份组件代码吃进另一套方案就换色」
/// 才叫可自定义（D38 的取色链就是这么接管 49 个角色的）。种子用 design 里的
/// [`DEFAULT_SEED`]（取色链默认的天蓝）——它是令牌，不是本文件写下的色值字面量
/// （`tests/no_hardcoded_tokens.rs` 拦 `#RRGGBB`，这条门禁是对的）。
fn themes() -> Vec<(&'static str, Ui)> {
    vec![
        (
            "ink",
            Ui::with_board(BrandBoard::Ink, Platform::Windows, super::FAMILIES),
        ),
        (
            "silver",
            Ui::with_board(BrandBoard::Silver, Platform::Windows, super::FAMILIES),
        ),
        (
            "custom",
            Ui::from_scheme(
                Scheme::from_seed(SchemeFlavor::Dark, unlucky_design::color::DEFAULT_SEED),
                super::FAMILIES,
            ),
        ),
    ]
}

#[test]
fn role_table_is_named_roles_not_colours() {
    // ---- P2：Elevated 的容器是**高程令牌的推论**，不是一个手抄的角色名 ----------
    assert_eq!(Variant::Elevated.elevation(), Elevation::Level1, "L1");
    assert_eq!(
        Variant::Elevated.roles().0,
        Some(Elevation::Level1.surface_role()),
        "Elevated 的底必须由高程令牌推导（L1 → surface_role = SurfaceContainerLow），\
         组件里不许再写一遍这个角色名"
    );
    assert_ne!(
        Elevation::Level1.surface_role(),
        Role::Surface,
        "L1 的容器面必须比画布高一档（否则 Elevated 与画布同色）"
    );

    // ---- P3：1dp 边界取 `Outline`；`OutlineVariant` 是装饰分隔线的角色 ----------
    assert_eq!(
        Variant::Outlined.roles(),
        (None, Role::OnSurfaceVariant, Some(Role::Outline)),
        "Outlined 的 1dp 描边取 Outline（`design/color/mod.rs` 的对比度纪律：\
         Outline 是「非文字元素」角色、目标 3:1；OutlineVariant 是纯装饰分隔线）"
    );
    // 同族一致性：图标按钮那一式一直是 Outline，此前文字按钮是 OutlineVariant ⇒ 全站两套描边
    assert_eq!(
        IconVariant::Outlined.roles().2,
        Some(Role::Outline),
        "图标按钮 Outlined 与文字按钮 Outlined 必须是同一个描边角色"
    );
}

/// 非文字元素的对比度下限 3:1（WCAG 1.4.11；`design/color/mod.rs` 的对比度纪律同一口径）。
///
/// 这条是 P3 的**理由**而不只是判据：`OutlineVariant` 在深/浅两块板上都到不了 3:1，
/// 拿它画按钮边界等于「看得见形状、量不出边界」。别把下限写死成某个角色的实测值——
/// 换品牌板或换取色方案时，比例是**算出来的**，不是抄出来的。
#[test]
fn outlined_boundary_meets_the_non_text_contrast_bar() {
    for (name, ui) in themes() {
        let scheme = ui.scheme();
        let boundary = scheme.contrast(Role::Outline, Role::Surface);
        assert!(
            boundary >= 3.0,
            "{name}: Outline/Surface = {boundary:.2}，非文字元素下限是 3:1"
        );
        // 报告用：装饰角色差多少（`--nocapture` 可见），不改判据
        eprintln!(
            "[{name}] Outline/Surface={boundary:.2} · OutlineVariant/Surface={:.2}",
            scheme.contrast(Role::OutlineVariant, Role::Surface)
        );
    }
}

#[test]
fn styles_follow_the_scheme_across_boards_and_custom_accents() {
    let elevated = spec_of(Variant::Elevated, Some(super::Interaction::Enabled));
    let outlined = spec_of(Variant::Outlined, Some(super::Interaction::Enabled));

    let mut elevated_fills = Vec::new();
    let mut outlined_strokes = Vec::new();

    for (name, ui) in themes() {
        let style = style_closure(&ui, elevated);
        let idle = style(&ui.theme(), button::Status::Active);
        let expected = ui.color(Elevation::Level1.surface_role());
        assert_eq!(
            idle.background,
            Some(Background::Color(expected)),
            "{name}: Elevated 的底 = 本方案的 L1 容器面"
        );
        assert_ne!(
            idle.background,
            Some(Background::Color(ui.color(Role::Surface))),
            "{name}: Elevated 退化成画布色了（取死值 Surface 的变异会打红这条）"
        );
        assert_eq!(
            idle.text_color,
            ui.color(Role::Primary),
            "{name}: Elevated 标签 = Primary"
        );

        let style = style_closure(&ui, outlined);
        let idle = style(&ui.theme(), button::Status::Active);
        assert_eq!(
            idle.border.color,
            ui.color(Role::Outline),
            "{name}: Outlined 描边 = Outline"
        );
        assert_eq!(idle.border.width, STROKE_PX, "{name}: 描边 1dp");
        assert_eq!(
            idle.text_color,
            ui.color(Role::OnSurfaceVariant),
            "{name}: Outlined 标签 = OnSurfaceVariant"
        );

        elevated_fills.push(expected);
        outlined_strokes.push(idle.border.color);
    }

    // 三套方案两两不同 ⇒「跟着配色来源走」这件事有实据（不是三套恰好同色）
    for (index, (name, _)) in themes().iter().enumerate() {
        for other in index + 1..elevated_fills.len() {
            assert_ne!(
                elevated_fills[index], elevated_fills[other],
                "{name} 与第 {other} 套方案的 Elevated 底同色，可主题化没有实据"
            );
            assert_ne!(
                outlined_strokes[index], outlined_strokes[other],
                "{name} 与第 {other} 套方案的 Outlined 描边同色"
            );
        }
    }
}

/// 基线条：Ink（深）与 Silver（浅）两块**出厂**板必须给出不同的 Elevated 底与 Outlined 描边。
///
/// 与上一条分开写：上一条的三套方案里有一套是自定义的，这条只钉出厂外观——
/// 哪怕有人把 `themes()` 改成只跑自定义方案，出厂这块的回归也还在。
#[test]
fn ink_and_silver_resolve_the_same_roles_to_different_colours() {
    let ink = Ui::with_board(BrandBoard::Ink, Platform::Windows, super::FAMILIES);
    let silver = Ui::with_board(BrandBoard::Silver, Platform::Windows, super::FAMILIES);
    for role in [
        Elevation::Level1.surface_role(),
        Role::Outline,
        Role::OnSurfaceVariant,
    ] {
        assert_ne!(
            ink.color(role),
            silver.color(role),
            "{role:?} 在 Ink 与 Silver 上同色，说明它没跟着色板走"
        );
    }
    // 浅色板的 L1 容器必须比画布暗、深色板必须比画布亮（层级方向不能反）
    assert!(silver.color(Role::Surface).r > silver.color(Role::SurfaceContainerLow).r);
    assert!(ink.color(Role::Surface).r < ink.color(Role::SurfaceContainerLow).r);
}
