//! **选中容器的底色**：`SecondaryContainer` **叠上**状态层（M296）。
//!
//! ## 为什么抽在这里（第三次法则）
//!
//! 「已选中的那一个」在壳里出现五次：菜单项、列表行、导航项药丸、导航轨（`shell/nav.rs`）、
//! 账户头像位（`shell/nav/account.rs`）。M296 之前这五处各写一遍同一个形状：
//!
//! ```text
//! if selected {
//!     layer_color(ui, state, OnSecondaryContainer, SecondaryContainer)
//!         .or_else(|| Some(ui.color(SecondaryContainer)))
//! }
//! ```
//!
//! 五份拷贝守的是**同一条 MD3 行为**，而第五份迟早会拿到 0.12 的 hover（[`super`] 的模块文档
//! 开头写的就是这件事）。塔 2026-11-13 的裁决把它钉成语义：选中容器上**再叠**状态层，
//! 两者不互斥——「选中了就不响应」是死控件，不是 Material。
//!
//! 抽在 `components` 层而不是 `pages/settings` 的 `forced_button` / `resolve_state`：那两个服务的是
//! 「强制态」（取证图怎么摆），这里服务的是「底色怎么算」，两回事，别合并。
//!
//! ## 与未选中态叠层的分工
//!
//! | 情形 | 入口 | 静息时返回 |
//! |---|---|---|
//! | **未**选中（容器透明，状态层就是它自己的底） | [`super::layer_color`] | `None`（不画底） |
//! | **已**选中（底是实色 `SecondaryContainer`） | [`selected_fill`] | 那个实色 |
//!
//! 两条不许合成一条：未选中的静息**没有底**，选中项的静息**有底**。合成一个入口就得在返回值里
//! 塞「要不要画」，调用方又要各自决定 `None` 的含义——那正是这里想消灭的东西。
//!
//! ## 走 `layer_color` 而不是 `overlay_color`
//!
//! M257 起状态层有过渡时间轴（`short1` 50ms）。`layer_color` 在**没有取样控件**时逐位等于
//! `overlay_color`，所以取证图（`--force-state=*`）与旧基线一字不差；有取样控件时才有中间帧。
//! 选中的底是实色，端点精确可判，插值只会落在两个不透明色之间（D27）。

use iced::Color;
use unlucky_design::color::Role;

use crate::theme::Ui;

use super::{layer_color, Interaction};

/// 选中容器的底色（**不透明**，`Color` 而不是 `Option<Color>`）。
///
/// - `state` 是当前交互态；静息（[`Interaction::Enabled`]）与过渡端点逐位等于 `ui.color(container)`；
/// - `content` 是叠在选中容器上的前景角色（MD3 的状态层用**内容色**，不是容器色）；
/// - `container` 是选中容器角色。
///
/// 现存调用点的 `(content, container)` 一律是
/// `(Role::OnSecondaryContainer, Role::SecondaryContainer)`——菜单项 / 列表行 / 导航项 / 版本选择行
/// 都用「次强调容器表示当前项」这一套语汇（见 `components/navigation/list_item.rs` 的模块文档）。
/// 参数留着是为了将来出现别的选中容器（例如 `TertiaryContainer` 的 chip 变体）时不再抄一遍函数体。
///
/// 禁用态**不**走这里：禁用换的是容器/内容色，不叠状态层（[`super::disabled`]）。
/// 调用点必须先处理 `Interaction::Disabled`——那时选中项也不画满档 `SecondaryContainer`
/// （菜单项与列表行已有这条判据，见各自的 `tests`）。
pub fn selected_fill(ui: &Ui, state: Interaction, content: Role, container: Role) -> Color {
    // 静息 / 禁用 → `layer_color` 给 `None` → 兜底到实色容器（选中项的静息**有底**）。
    layer_color(ui, state, content, container).unwrap_or_else(|| ui.color(container))
}

#[cfg(test)]
mod tests {
    use super::*;
    use unlucky_design::color::{SchemeFlavor, StateLayer};
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

    /// 选中项的静息底 = `SecondaryContainer`，**逐位**等于角色求值。
    ///
    /// 抓得住：有人把兜底写成别的角色（`SurfaceContainerHigh` 之类），或让静息也叠一层
    /// （`--force-state=rest` 的取证图会整片变色，判据 1 的 md5 立刻红）。
    #[test]
    fn resting_selected_is_the_plain_container_role() {
        for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
            let ui = ui(flavor);
            assert_eq!(
                selected_fill(
                    &ui,
                    Interaction::Enabled,
                    Role::OnSecondaryContainer,
                    Role::SecondaryContainer
                ),
                ui.color(Role::SecondaryContainer),
                "{flavor:?}：选中项静息必须是实色容器"
            );
        }
    }

    /// **选中 + hover / pressed / focus 三色两两不同，且都与静息不同**（M296 判据 2 的单测半边）。
    ///
    /// 抓得住：把函数体改回「选中就不叠层」（直接 `ui.color(container)`）——四个断言一起红；
    /// 把 hover 的 α 抄成 pressed 的 10%——`hover != pressed` 红。
    /// 抓不住：图上到底差几个色阶（那是 `DevDocs/artifacts/selstate` 的裁图判据）。
    #[test]
    fn selected_reacts_to_hover_and_press() {
        for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
            let ui = ui(flavor);
            let color = |state| {
                selected_fill(
                    &ui,
                    state,
                    Role::OnSecondaryContainer,
                    Role::SecondaryContainer,
                )
            };
            let rest = color(Interaction::Enabled);
            let hover = color(Interaction::Hovered);
            let pressed = color(Interaction::Pressed);
            let focus = color(Interaction::Focused);

            assert_ne!(hover, rest, "{flavor:?}：选中项 hover 必须与静息不同");
            assert_ne!(pressed, rest, "{flavor:?}：选中项 pressed 必须与静息不同");
            assert_ne!(hover, pressed, "{flavor:?}：8% 与 10% 必须能区分");
            assert_eq!(
                focus, pressed,
                "{flavor:?}：focus 与 pressed 同 α（M3 口径），图上一样是**对的**"
            );
            for (state, color) in [
                (Interaction::Enabled, rest),
                (Interaction::Hovered, hover),
                (Interaction::Pressed, pressed),
            ] {
                assert_eq!(color.a, 1.0, "{flavor:?}/{state:?}：结果必须不透明（D27）");
            }
        }
    }

    /// 叠的那一层**就是令牌求值**（改 α 会红，改组件抓不到）。
    #[test]
    fn the_layer_is_the_token_blend_not_local_math() {
        let ui = ui(SchemeFlavor::Light);
        for (state, layer) in [
            (Interaction::Hovered, StateLayer::Hover),
            (Interaction::Pressed, StateLayer::Pressed),
            (Interaction::Focused, StateLayer::Focus),
        ] {
            assert_eq!(
                selected_fill(
                    &ui,
                    state,
                    Role::OnSecondaryContainer,
                    Role::SecondaryContainer
                ),
                ui.state_over(layer, Role::OnSecondaryContainer, Role::SecondaryContainer),
                "{state:?} 必须逐位等于 state_over 的令牌求值"
            );
        }
    }

    /// 没有取样控件时**逐位**等于 M257 之前的 `overlay_color(..).or(实色)` 写法。
    ///
    /// 抓得住：有人把时间轴接进求值路径时顺手改了静息/终态的颜色（旧基线会漂）。
    /// 抓不住：过渡中间帧（`layer::tests` 的事）。
    #[test]
    fn without_a_sampler_it_is_bit_identical_to_the_old_expression() {
        let ui = ui(SchemeFlavor::Dark);
        for state in Interaction::ALL {
            let old = super::super::overlay_color(
                &ui,
                state,
                Role::OnSecondaryContainer,
                Role::SecondaryContainer,
            )
            .unwrap_or_else(|| ui.color(Role::SecondaryContainer));
            assert_eq!(
                selected_fill(
                    &ui,
                    state,
                    Role::OnSecondaryContainer,
                    Role::SecondaryContainer
                ),
                old,
                "{state:?} 必须逐位退回旧写法"
            );
        }
    }
}
