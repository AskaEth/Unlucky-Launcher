//! M3 状态层（state layer）与禁用态透明度。
//!
//! M3 的 hover/pressed 不是「另一种灰」，而是**内容色自身 + 固定 alpha** 叠在容器上——
//! 所以同一套令牌在任何底色上都成立，不必为每种底色各配一套（这条从 Fluent 时代沿用，
//! 见 `DevDocs/SubAgentsTalk/ui-integration-contract.md` §2 的三态叠加层思路）。

use super::Color;

/// 状态层的四种状态与对应 alpha（M3 规范值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateLayer {
    /// 悬停：内容色 8%。
    Hover,
    /// 键盘焦点：内容色 10%。
    Focus,
    /// 按下：内容色 10%。
    Pressed,
    /// 拖拽中：内容色 16%。
    Dragged,
}

impl StateLayer {
    /// 叠加透明度（0..1）。
    pub const fn alpha(self) -> f32 {
        match self {
            StateLayer::Hover => 0.08,
            StateLayer::Focus | StateLayer::Pressed => 0.10,
            StateLayer::Dragged => 0.16,
        }
    }

    /// 把状态层叠到容器色上，得到**最终不透明色**。
    ///
    /// `content` 一般是容器上的前景角色（例如 FilledButton 用 `on-primary`，
    /// 中性表面上的按钮用 `on-surface`）。
    pub fn over(self, content: Color, container: Color) -> Color {
        content.with_alpha(self.alpha()).over(container)
    }
}

/// 禁用态的透明度（M3）：**按钮家族**容器 10%、内容 38%。
///
/// 别用「灰化」近似——深色模式下灰化会降低对比度，alpha 叠加两种模式都成立。
///
/// ## 取值依据（每个 token 的消费者都用 `javap -c ButtonDefaults` 数过）
///
/// - **按钮容器 10%**：`FilledButtonTokens` / `ElevatedButtonTokens` / `OutlinedButtonTokens` 的
///   `DisabledContainerOpacity = 0.1f`，且 `ButtonDefaults.<clinit>` 里**真的调了**
///   `getDisabledContainerOpacity()`（各 1 次）——活令牌。
///   `TextButtonTokens` 同值但容器透明（`ButtonDefaults` 只读它的 `DisabledLabel*`）；
///   `BaselineButtonTokens` 同值，它是**基准表**、运行时零消费者，只作参照。
/// - ⚠️ `TonalButtonTokens` 也写 0.1f，但它在**整个 AAR 里零消费者**（`ButtonDefaults` 只引用
///   `FilledTonalButtonTokens`；`grep` 命中的 `…TonalButtonTokens` 是后者的子串假象）。
///   **别拿它当依据**——这正是本 mission 反复警告的死令牌坑（同 `ButtonSmallTokens.IconSize = 20`）。
/// - **面状控件（chip 容器/描边、slider 轨道、switch 轨道）12%** 是另一类令牌，口径在
///   `unlucky_app_iced::components::interaction::disabled_colors`。
///
/// ## ⚠️ 已知偏离：filled-tonal 的禁用容器**官方是 0.12f，我们走 0.10**
///
/// A 级事实（`javap -p -constants androidx.compose.material3.tokens.FilledTonalButtonTokens`）：
/// `public static final float DisabledContainerOpacity = 0.12f;`
/// ——它是**编译期常量**，被内联进 `ButtonDefaults.getDefaultFilledTonalButtonColors` 的字节码
/// （所以那里看到的是 `ldc_w 0.12f`，`getDisabledContainerOpacity()` 根本不会被调用；
/// 在 `<clinit>` 里也看不到它——那是「没在 clinit 里出现」，**不是「字段不存在」**）。
///
/// 我们的实现在 tonal 与 filled 之间**没有区分**，两者共用本常量 ⇒ **tonal 实际画的是 10%**。
/// 这是**已登记的偏离，不是对齐**：要修得让 `disabled_button_colors` 知道「哪个变体」，
/// 并同步 `gallery/probe` 的逐行断言；本轮（M86）只做令牌一致性收口，不扩到变体分派。
///
/// 曾经的缺陷不是「12 是错的」，而是**同一个常量同时服务这两类**：按钮那半边一直错、另半边一直对。
pub const DISABLED_CONTAINER_ALPHA: f32 = 0.10;
/// 禁用态内容（文字/图标）透明度。
pub const DISABLED_CONTENT_ALPHA: f32 = 0.38;

/// 禁用态的透明度（M3）：**面状控件族**容器与描边 12%。
///
/// 这一族是「一整面涂上去的容器或用它画出来的描边/轨道」，官方 token 各写一份但值统一是 `0.12f`：
/// `AssistChipTokens.FlatDisabledOutlineOpacity`、`FilterChipTokens.FlatDisabledSelectedContainerOpacity`
/// / `FlatDisabledUnselectedOutlineOpacity`、`SliderTokens.DisabledInactiveTrackOpacity`、
/// `SwitchTokens.DisabledTrackOpacity`（AAR 字节码 `<clinit>`）。
///
/// **不适用于按钮容器**：按钮是 [`DISABLED_CONTAINER_ALPHA`]（10%，`*ButtonTokens.DisabledContainerOpacity`）。
/// 更不适用于 TextField 的 Filled 容器——那是 `FilledTextFieldTokens.FieldDisabledContainerOpacity = 0.04f`，
/// 我们仍在用 12%（既有 D10 偏离，未在本轮收口）。
pub const DISABLED_SURFACE_ALPHA: f32 = 0.12;

/// 禁用态容器色：`on_surface` 10%（按钮家族的 `DisabledContainerOpacity`）叠在 `surface` 上。
pub fn disabled_container(scheme: &super::Scheme) -> Color {
    scheme
        .get(super::Role::OnSurface)
        .with_alpha(DISABLED_CONTAINER_ALPHA)
        .over(scheme.get(super::Role::Surface))
}

/// 禁用态内容色：`on_surface` 38% 叠在容器上。
pub fn disabled_content(scheme: &super::Scheme, container: Color) -> Color {
    scheme
        .get(super::Role::OnSurface)
        .with_alpha(DISABLED_CONTENT_ALPHA)
        .over(container)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{Role, Scheme, SchemeFlavor};

    #[test]
    fn hover_is_lighter_than_pressed_on_dark_container() {
        let scheme = Scheme::baseline(SchemeFlavor::Dark);
        let container = scheme.get(Role::SurfaceContainer);
        let content = scheme.get(Role::OnSurface);
        let hover = StateLayer::Hover.over(content, container);
        let pressed = StateLayer::Pressed.over(content, container);
        assert_eq!(hover.a, 255);
        assert!(pressed.relative_luminance() > hover.relative_luminance());
    }

    #[test]
    fn state_layer_alpha_matches_m3() {
        assert!((StateLayer::Hover.alpha() - 0.08).abs() < f32::EPSILON);
        assert!((StateLayer::Dragged.alpha() - 0.16).abs() < f32::EPSILON);
    }

    #[test]
    fn disabled_content_stays_visible() {
        let scheme = Scheme::baseline(SchemeFlavor::Light);
        let container = disabled_container(&scheme);
        let content = disabled_content(&scheme, container);
        // 禁用态不追求 AA（规范就是这么定的），但必须仍可辨（> 1.5:1）。
        let ratio = content.contrast_ratio(container);
        assert!(ratio > 1.5, "禁用态对比度过低：{ratio}");
        assert!(ratio < 4.5, "禁用态不该和正文一样醒目：{ratio}");
    }

    /// 两族 α 的**令牌层钉值**（M86）：期望值硬编码，不从被测常量取——否则改坏常量也照样绿。
    ///
    /// 出处（AAR `material3-android-1.4.0` 的 `<clinit>`）：
    /// `*ButtonTokens.DisabledContainerOpacity = 0.1f`（按钮族）与
    /// `AssistChipTokens.FlatDisabledOutlineOpacity` / `SliderTokens.DisabledInactiveTrackOpacity` /
    /// `SwitchTokens.DisabledTrackOpacity = 0.12f`（面状控件族）。
    #[test]
    fn disabled_alpha_classes_are_pinned_at_the_token_layer() {
        const EXPECTED_BUTTON: f32 = 0.10;
        const EXPECTED_SURFACE: f32 = 0.12;
        const EXPECTED_CONTENT: f32 = 0.38;
        let button = DISABLED_CONTAINER_ALPHA;
        let surface = DISABLED_SURFACE_ALPHA;
        let content = DISABLED_CONTENT_ALPHA;
        assert_eq!(
            button, EXPECTED_BUTTON,
            "按钮族禁用容器 α 必须是官方 0.1f（*ButtonTokens.DisabledContainerOpacity）"
        );
        assert_eq!(
            surface, EXPECTED_SURFACE,
            "面状控件族 α 必须是官方 0.12f（chip 描边 / slider 轨道 / switch 轨道）"
        );
        assert_eq!(content, EXPECTED_CONTENT, "禁用内容 0.38f");
        assert!(
            surface > button,
            "两族必须可区分（{surface} > {button}）；合成一个数就是 M86 要防的回归"
        );
    }
}
