//! 禁用态的颜色口径：**两个族、两套 α**（M86 收口）。
//!
//! ## 为什么不是「一个 12%」
//!
//! 官方禁用 α 分两类（A 级：`material3-android-1.4.0.aar` 的字节码 `<clinit>`）：
//!
//! | 族 | α | 官方 token | 消费者 |
//! |---|---|---|---|
//! | **按钮容器** | **0.10** | `*ButtonTokens.DisabledContainerOpacity`（`Filled` / `Outlined` / `Text` / `Elevated` / `Tonal` / `Baseline`） | `buttons::assemble` |
//! | **面状控件**容器与描边 | **0.12** | `AssistChipTokens.FlatDisabledOutlineOpacity`、`FilterChipTokens.FlatDisabled(SelectedContainer/UnselectedOutline)Opacity`、`SliderTokens.DisabledInactiveTrackOpacity`、`SwitchTokens.DisabledTrackOpacity` | chip / switch / slider / TextField 描边 |
//! | 内容（两族共用） | 0.38 | `Disabled{Icon,LabelText,Input}Opacity` | 全部 |
//!
//! 历史上这里只有一个 12% 常量供两族共用：**按钮那半边一直错、chip/slider/switch 那半边一直对**。
//! 现在按族分开两个入口——取错入口的后果是「差 1/255」，肉眼看不见，只有像素 probe 会红，
//! 所以入口**必须按名字就能选对**。
//!
//! ⚠️ TextField 的 **Filled 容器**不在这两族里：官方
//! `FilledTextFieldTokens.FieldDisabledContainerOpacity = 0.04f`，我们仍在用面状族的值
//! （既有 D10 偏离，未在本轮收口）。`inputs::field` 走 [`disabled_colors`] 的口径，行为未变。
//!
//! ⚠️ **filled-tonal 是另一条已知偏离**：官方
//! `FilledTonalButtonTokens.DisabledContainerOpacity = 0.12f`（`javap -p -constants` 可见它是个
//! **编译期常量**，被内联进 `getDefaultFilledTonalButtonColors` 的 `ldc_w 0.12f`，所以调用点是常量、
//! `getDisabledContainerOpacity()` 不会被调用）；而本模块的 `disabled_button_*` **不区分变体**，
//! tonal 与 filled 共用 0.10 ⇒ tonal 实际画 10%。这不是对齐、是登记过的偏离；要修得让入口知道变体，
//! 本轮（M86）不做。

use iced::Color;

use unlucky_design::color::state::{
    DISABLED_CONTAINER_ALPHA, DISABLED_CONTENT_ALPHA, DISABLED_SURFACE_ALPHA,
};
use unlucky_design::color::{Role, Scheme};

use crate::theme::{to_iced, Ui};

/// 禁用态内容色（**按角色、按有没有容器**求值）——A1 口径的入口（**面状控件族**的底）。
///
/// - `role`：由变体决定（Filled/Outlined/Text/Elevated = `OnSurfaceVariant`；
///   FilledTonal 与三式 IconButton = `OnSurface`，见 `Variant::disabled_content`）；
/// - `on_container = true`：叠在**面状控件族的 12% 容器**（[`DISABLED_SURFACE_ALPHA`]）上
///   （Filled TextField / 有容器控件在 12% 底上求内容时）；
///   `false`：直接叠在 **surface** 上（Text / Outlined / standard IconButton / 描边类控件的内容）。
///
/// **按钮家族不要用这个函数**（按钮容器是 10%）：走 [`disabled_button_content`]。
///
/// 之前这里写死 `OnSurfaceVariant` 且只有一个「底」，导致 FilledTonal 与 IconButton 两族偏浅
/// 15/255（视觉 r3 用像素量的）。
pub fn disabled_content_over_role(ui: &Ui, role: Role, on_container: bool) -> Color {
    let scheme = Scheme::baseline(ui.flavor());
    disabled_content_with_alpha(&scheme, role, on_container, DISABLED_SURFACE_ALPHA)
}

/// 禁用态内容色——**按钮家族**（`on_container`：是否叠在本族 10% 容器上）。
///
/// 与 [`disabled_content_over_role`] 的差别只有「底」的 α：按钮 10%、面状控件 12%。
/// 两个入口分开是 M86 的结论：合成一个数会让一族永远差 1/255，肉眼看不见、只有 probe 会红。
pub fn disabled_button_content(ui: &Ui, role: Role, on_container: bool) -> Color {
    let scheme = Scheme::baseline(ui.flavor());
    disabled_content_with_alpha(&scheme, role, on_container, DISABLED_CONTAINER_ALPHA)
}

/// 两个族共用的内容色求值（「底」的 α 由调用方给）。
fn disabled_content_with_alpha(
    scheme: &Scheme,
    role: Role,
    on_container: bool,
    container_alpha: f32,
) -> Color {
    let base = if on_container {
        scheme
            .get(Role::OnSurface)
            .with_alpha(container_alpha)
            .over(scheme.get(Role::Surface))
    } else {
        scheme.get(Role::Surface)
    };

    to_iced(
        scheme
            .get(role)
            .with_alpha(DISABLED_CONTENT_ALPHA)
            .over(base),
    )
}

/// 禁用态的一对颜色 `(容器, 内容)`——**面状控件族**（12% 容器/描边）。
///
/// ⚠️ 这个名字是历史遗留（当年只有它一个），**语义是「面状控件族」**：
/// - chip 容器与描边（`AssistChipTokens.FlatDisabledOutlineOpacity = 0.12f` 等）
/// - switch 轨道（`SwitchTokens.DisabledTrackOpacity = 0.12f`）
/// - slider 未激活轨（`SliderTokens.DisabledInactiveTrackOpacity = 0.12f`）
/// - TextField 禁用描边（Filled 容器的 4% 是另一条 D10 偏离，不在这里收口）
///
/// **按钮家族用 [`disabled_button_colors`]**（0.10，`*ButtonTokens.DisabledContainerOpacity = 0.1f`）。
///
/// `ui` 不暴露 [`Scheme`]，这里按同一个 flavor 求一次基线方案——调的是
/// `unlucky_design` 的**同一个**函数与同一套角色，不是第二份令牌。
///
/// 顺序必须「先容器后内容」：Compose 也是这么画的，先叠容器再叠 label，
/// 所以内容叠的角色是容器实色而不是底下的 surface。
pub fn disabled_colors(ui: &Ui, surface: Role) -> (Color, Color) {
    disabled_colors_with_alpha(ui, surface, DISABLED_SURFACE_ALPHA)
}

/// 禁用态的一对颜色 `(容器, 内容)`——**按钮家族**（10% 容器）。
///
/// - 容器 = `OnSurface` @ 10%（[`DISABLED_CONTAINER_ALPHA`]）叠在 `surface` 上；
/// - 内容 = **默认** `OnSurfaceVariant` @ 38%；按变体的取值走 [`disabled_button_content`]。
///
/// 除了按钮家族，**没有别的消费者**：面状控件（chip / slider 轨道 / switch 轨道 / 描边）走
/// [`disabled_colors`] 的 12%。
pub fn disabled_button_colors(ui: &Ui, surface: Role) -> (Color, Color) {
    disabled_colors_with_alpha(ui, surface, DISABLED_CONTAINER_ALPHA)
}

/// 两个族共用的求值：容器 = `OnSurface` @ `container_alpha` 叠在 `surface` 上，内容 = 38% 叠在容器上。
fn disabled_colors_with_alpha(ui: &Ui, surface: Role, container_alpha: f32) -> (Color, Color) {
    let scheme = Scheme::baseline(ui.flavor());
    let base = scheme.get(surface);
    let container = scheme
        .get(Role::OnSurface)
        .with_alpha(container_alpha)
        .over(base);
    // 这里是**默认口径**（OnSurfaceVariant）；按变体取值请走 [`disabled_content_over_role`]
    // ——FilledTonal 与三式 IconButton 的禁用内容是 `OnSurface`（A1）。
    let content = scheme
        .get(Role::OnSurfaceVariant)
        .with_alpha(DISABLED_CONTENT_ALPHA)
        .over(container);

    (to_iced(container), to_iced(content))
}

#[cfg(test)]
mod tests {
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
            let scheme = Scheme::baseline(flavor);
            let base = scheme.get(Role::Surface);
            let expected_container = scheme
                .get(Role::OnSurface)
                .with_alpha(EXPECTED_SURFACE_ALPHA)
                .over(base);

            let (surface, surface_content) = disabled_colors(&ui, Role::Surface);
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
            let (button, _) = disabled_button_colors(&ui, Role::Surface);
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
            let scheme = Scheme::baseline(flavor);
            let base = scheme.get(Role::Surface);
            let expected_container = scheme
                .get(Role::OnSurface)
                .with_alpha(EXPECTED_BUTTON_ALPHA)
                .over(base);

            let (button, button_content) = disabled_button_colors(&ui, Role::Surface);
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

            let (surface, _) = disabled_colors(&ui, Role::Surface);
            assert_ne!(
                button, surface,
                "{flavor:?}：10% 与 12% 必须落在不同像素值上（合成一个数就是这次要防的回归）"
            );

            // 内容色同理：按钮 38% 叠 10% 底 ≠ 面状 38% 叠 12% 底
            assert_ne!(
                button_content,
                disabled_content_over_role(&ui, Role::OnSurfaceVariant, true),
                "{flavor:?}：两族的「有容器」内容色不能同源"
            );
            assert_eq!(
                disabled_button_content(&ui, Role::OnSurfaceVariant, true),
                button_content,
                "{flavor:?}：按钮内容入口必须与按钮颜色入口同源"
            );
            // 无容器时两族完全一致（都叠 surface）——差异只在「有容器」那一支
            assert_eq!(
                disabled_button_content(&ui, Role::OnSurfaceVariant, false),
                disabled_content_over_role(&ui, Role::OnSurfaceVariant, false),
                "{flavor:?}：没有容器时不该有族差异"
            );
        }
    }
}
