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

use super::base::Base;

// 四个入口都收 `impl Into<Base>`：容器导出的 `BASE` 常量（推荐）与裸 `Role`（历史调用点，
// 例如 `pages/settings/java/tests/marker.rs`）都能进来。**没有隐式默认值**——不给就是编译错误，
// 这一条正是 M229 缺的：参数在、语义在，只是调用点全传了 `Role::Surface`。

/// 禁用态内容色（**按角色、按有没有容器**求值）——A1 口径的入口（**面状控件族**的底）。
///
/// - `base`：控件**脚下那一层的底**（[`Base`]）。容器给的 `BASE` 常量，不是猜；
/// - `role`：由变体决定（Filled/Outlined/Text/Elevated = `OnSurfaceVariant`；
///   FilledTonal 与三式 IconButton = `OnSurface`，见 `Variant::disabled_content`）；
/// - `on_container = true`：叠在**面状控件族的 12% 容器**（[`DISABLED_SURFACE_ALPHA`]）上
///   （Filled TextField / 有容器控件在 12% 底上求内容时），容器本身叠在 `base` 上；
///   `false`：直接叠在 `base` 上（Text / Outlined / standard IconButton / 描边类控件的内容）。
///
/// **按钮家族不要用这个函数**（按钮容器是 10%）：走 [`disabled_button_content`]。
///
/// 之前这里写死 `OnSurfaceVariant` 且只有一个「底」，导致 FilledTonal 与 IconButton 两族偏浅
/// 15/255（视觉 r3 用像素量的）。
pub fn disabled_content_over_role(
    ui: &Ui,
    base: impl Into<Base>,
    role: Role,
    on_container: bool,
) -> Color {
    // **取 `Ui` 里已注入的方案**，不重算 baseline：M229 之后产品方案是品牌色板
    // （Ink / Silver），照 baseline 重算会画出旧的天蓝中性色（复审 F1）。
    disabled_content_with_alpha(
        ui.scheme(),
        base.into(),
        role,
        on_container,
        DISABLED_SURFACE_ALPHA,
    )
}

/// 禁用态内容色——**按钮家族**（`on_container`：是否叠在本族 10% 容器上）。
///
/// 与 [`disabled_content_over_role`] 的差别只有「底」的 α：按钮 10%、面状控件 12%。
/// 两个入口分开是 M86 的结论：合成一个数会让一族永远差 1/255，肉眼看不见、只有 probe 会红。
pub fn disabled_button_content(
    ui: &Ui,
    base: impl Into<Base>,
    role: Role,
    on_container: bool,
) -> Color {
    disabled_content_with_alpha(
        ui.scheme(),
        base.into(),
        role,
        on_container,
        DISABLED_CONTAINER_ALPHA,
    )
}

/// 两个族共用的内容色求值（「底」的 α 由调用方给）。
fn disabled_content_with_alpha(
    scheme: &Scheme,
    base: Base,
    role: Role,
    on_container: bool,
    container_alpha: f32,
) -> Color {
    let underneath = scheme.get(base.role());
    let base = if on_container {
        scheme
            .get(Role::OnSurface)
            .with_alpha(container_alpha)
            .over(underneath)
    } else {
        underneath
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
/// 合成的输入是 [`Ui::scheme`]（品牌色板 / 取色方案），**不是** `Scheme::baseline(flavor)`：
/// 后者是 tone 派生的旧口径，在 Ink 板上会让禁用容器偏蓝（复审 F1 的根因）。
///
/// 顺序必须「先容器后内容」：Compose 也是这么画的，先叠容器再叠 label，
/// 所以内容叠的角色是容器实色而不是底下的 surface。
///
/// ⚠️ **`base` 必须是控件脚下那一层的底**（[`Base`]）：M229 之前这里的参数名叫 `surface`，
/// 而 8 个调用点全传 `Role::Surface`，于是卡片/面板里的禁用控件把 12% 叠在画布色上（M234）。
pub fn disabled_colors(ui: &Ui, base: impl Into<Base>) -> (Color, Color) {
    disabled_colors_with_alpha(ui, base.into(), DISABLED_SURFACE_ALPHA)
}

/// 禁用态的一对颜色 `(容器, 内容)`——**按钮家族**（10% 容器）。
///
/// - 容器 = `OnSurface` @ 10%（[`DISABLED_CONTAINER_ALPHA`]）叠在 `base` 上；
/// - 内容 = **默认** `OnSurfaceVariant` @ 38%；按变体的取值走 [`disabled_button_content`]。
///
/// 除了按钮家族，**没有别的消费者**：面状控件（chip / slider 轨道 / switch 轨道 / 描边）走
/// [`disabled_colors`] 的 12%。
pub fn disabled_button_colors(ui: &Ui, base: impl Into<Base>) -> (Color, Color) {
    disabled_colors_with_alpha(ui, base.into(), DISABLED_CONTAINER_ALPHA)
}

/// 两个族共用的求值：容器 = `OnSurface` @ `container_alpha` 叠在 `base` 上，内容 = 38% 叠在容器上。
fn disabled_colors_with_alpha(ui: &Ui, base: Base, container_alpha: f32) -> (Color, Color) {
    let scheme = ui.scheme();
    let underneath = scheme.get(base.role());
    let container = scheme
        .get(Role::OnSurface)
        .with_alpha(container_alpha)
        .over(underneath);
    // 这里是**默认口径**（OnSurfaceVariant）；按变体取值请走 [`disabled_content_over_role`]
    // ——FilledTonal 与三式 IconButton 的禁用内容是 `OnSurface`（A1）。
    let content = scheme
        .get(Role::OnSurfaceVariant)
        .with_alpha(DISABLED_CONTENT_ALPHA)
        .over(container);

    (to_iced(container), to_iced(content))
}

#[cfg(test)]
mod tests;
