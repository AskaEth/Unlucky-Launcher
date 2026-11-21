//! 设置页的**强制态**（`--force-state`）两个入口：[`resolve_state`] / [`forced_button`]（M285）。
//!
//! M393 从 `settings/mod.rs` 原样搬出来，只为行数（那个文件撞了 400 行硬线）。函数体、文档与断言
//! 一字未动；`mod.rs` 用 `pub(super) use` 留住 `pages::settings::{resolve_state, forced_button}`
//! 这两个路径，全仓调用点一行都不用改。
//!
//! 为什么条目本身是 `pub(crate)`（搬前是 `pub(super)`）：`mod interaction;` 私有，而 `mod.rs` 要把
//! 它们再导出到 `pages`（`pub(super)`），条目自身的可见性就必须至少到 `pub(crate)`——与
//! `registry::Group` 同一条形状。对 `settings` 之外可见性没变：路径仍要经过私有的 `interaction`。

use iced::widget::button;

use crate::app::components::buttons::Button;
use crate::app::components::interaction::Interaction;

/// 这一帧某个控件**实际要画**的交互态：`forced`（`--force-state`）优先，否则退回观测到的 `status`。
///
/// **全仓唯一的 resolve 点（M285）**：语言档 / 主题档 / 日志 `action_button` / 外观 `choice` 一律调它
/// ——各自再写一遍 `unwrap_or_else` 就是把同一行语义复制四份，下次只改三处就等着漂。语义与
/// `pages/instances.rs` 的 `forced_state` 相同（`Some` 覆盖、`None` 不动），只是裸 `iced::button` 没有
/// `.state()`，覆盖只能发生在样式闭包**里面**（换掉它拿到的那一态）。
pub(crate) fn resolve_state(forced: Option<Interaction>, status: button::Status) -> Interaction {
    forced.unwrap_or_else(|| Interaction::from_status(status))
}

/// 把一个 `components::buttons::Button` 钉到 `forced` 那一态（`pages/instances.rs` 的 `forced_state`
/// 在设置页的这一份）：`Some` ⇒ `.state(..)`，`None` ⇒ 原样返回。
///
/// ⚠️ **优先级是 `state` 赢**（`Button::forced()` = `self.state.or(没有消息 ⇒ Disabled)`，`state` 在前）
/// ⇒ `forced_button(Button::disabled(..), Some(Hovered))` 会把禁用按钮**画成 hover**。今天 7 个站点都
/// 写在**启用分支里**所以安全，**别**套到 `if..else` 外面。第二份不是抽象：三次法则到 2。
pub(crate) fn forced_button<'a, M>(
    button: Button<'a, M>,
    forced: Option<Interaction>,
) -> Button<'a, M> {
    match forced {
        Some(state) => button.state(state),
        None => button,
    }
}

#[cfg(test)]
mod resolve_tests {
    use super::*;

    /// 两条语义各钉一次（M285）：`forced_button` 的 `Some` 钉态 / `None` 原样（**禁用态也会被 `state`
    /// 盖掉**——`Button::forced()` 里 `state` 在 disabled 默认之前，钉住实际行为，别让站点踩）、
    /// `resolve_state` 的 `Some` 覆盖观测 status / `None` 退回观测值。`Button::state` 没有 getter，
    /// 所以断 derived Debug 的那一格。抓不住：按钮真画成那色（出图负责）、站点传错 forced（只有像素）。
    #[test]
    fn forced_button_forces_only_when_asked() {
        let plain = Button::new("x", ());
        let forced = format!(
            "{:?}",
            forced_button(plain.clone(), Some(Interaction::Hovered))
        );
        assert!(
            forced.contains("state: Some(Hovered)"),
            "forced 必须钉到那一态：{forced}"
        );
        let untouched = format!("{:?}", forced_button(plain, None));
        assert!(
            untouched.contains("state: None"),
            "没给 forced 时必须原样返回：{untouched}"
        );
        // 禁用按钮同样会被钉态（`state` 覆盖 disabled 默认，类型层不挡）：
        let disabled: Button<'_, ()> =
            forced_button(Button::disabled("x"), Some(Interaction::Hovered));
        assert!(
            format!("{disabled:?}").contains("state: Some(Hovered)"),
            "state 在 disabled 默认之前 ⇒ 禁用按钮也会被画成 hover/pressed"
        );
    }

    #[test]
    fn forced_state_wins_over_the_observed_status() {
        assert_eq!(
            resolve_state(Some(Interaction::Hovered), button::Status::Pressed),
            Interaction::Hovered,
            "forced 必须覆盖观测到的 status，否则 --force-state 在这四个控件上等于没接线"
        );
        assert_eq!(
            resolve_state(None, button::Status::Pressed),
            Interaction::Pressed,
            "没给 forced 时必须退回观测值（既有行为不变）"
        );
    }
}
