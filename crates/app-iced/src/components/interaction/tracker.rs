//! 自定义 `Widget` 的状态机（[`Tracker`]）：把 iced 事件折成 [`Interaction`] 五态。
//!
//! 与 [`super`] 的分工：`super` 只有「态 → 颜色」的纯映射；**事件 → 态**在这里。
//! 本 mission 的按钮家族全走 iced 的 `button`（自带四态），所以 `Tracker` 目前只有单测在用——
//! 它给四个后续家族里**要自绘**的控件（Slider 轨道 / Switch 轨道 / 进度条）准备。
//!
//! 加东西前先看行数余额（单文件 ≤400，AGENTS.md §3-1）。

use iced::Event;

use super::Interaction;

/// 自定义 `Widget` 的状态机：把 iced 事件折成 [`Interaction`]。
///
/// 这是 D39 §3 说的「状态机的公共部分」。用法（自定义 Widget 的 `update` 里）：
///
/// ```ignore
/// if self.tracker.event(event, cursor.is_over(layout.bounds())) {
///     shell.request_redraw();       // 只有真变了才重绘
/// }
/// let style = style_of(self.tracker.state());
/// ```
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tracker {
    enabled: bool,
    hovered: bool,
    focused: bool,
    pressed: bool,
}

#[allow(dead_code)]
impl Tracker {
    /// 新建；`enabled` 由组件自己决定（例如 `on_press.is_some()`）。
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            ..Self::default()
        }
    }

    /// 启用/禁用热切换（禁用时清掉悬停与按下：禁用元素不该留着 hover 高亮）。
    pub fn set_enabled(&mut self, enabled: bool) -> bool {
        let changed = self.enabled != enabled;
        self.enabled = enabled;
        if !enabled {
            self.hovered = false;
            self.pressed = false;
            self.focused = false;
        }
        changed
    }

    /// 键盘焦点（由 `operation`/`shell` 侧的聚焦消息驱动）。
    pub fn set_focused(&mut self, focused: bool) -> bool {
        let next = focused && self.enabled;
        let changed = self.focused != next;
        self.focused = next;
        changed
    }

    /// 折进一个事件；返回「视觉态变了，要重绘吗」。
    ///
    /// `over` = 指针是否在控件 bounds 内（`Cursor::is_over(bounds)` 的结果，由调用方算——
    /// 这样本函数不依赖 `Layout`，可以在单测里直接喂）。
    pub fn event(&mut self, event: &Event, over: bool) -> bool {
        if !self.enabled {
            return false;
        }

        let before = self.state();
        match event {
            Event::Mouse(iced::mouse::Event::CursorMoved { .. }) => self.hovered = over,
            Event::Mouse(iced::mouse::Event::CursorLeft) => {
                self.hovered = false;
                self.pressed = false;
            }
            Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)) => {
                self.pressed = over;
            }
            Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
                self.pressed = false;
            }
            _ => {}
        }
        self.state() != before
    }

    /// 当前态（优先级：按下 > 焦点 > 悬停 > 静息；禁用最高）。
    pub const fn state(&self) -> Interaction {
        if !self.enabled {
            Interaction::Disabled
        } else if self.pressed {
            Interaction::Pressed
        } else if self.focused {
            Interaction::Focused
        } else if self.hovered {
            Interaction::Hovered
        } else {
            Interaction::Enabled
        }
    }
}

/// `Tracker` 的行为用例（自绘控件用，与颜色无关）。
#[cfg(test)]
mod tests {
    use super::*;
    use iced::Event;

    fn moved() -> Event {
        Event::Mouse(iced::mouse::Event::CursorMoved {
            position: iced::Point::ORIGIN,
        })
    }

    #[test]
    fn tracker_folds_events_into_the_five_states() {
        let mut tracker = Tracker::new(true);
        assert_eq!(tracker.state(), Interaction::Enabled);

        assert!(tracker.event(&moved(), true), "首次悬停要报变化");
        assert_eq!(tracker.state(), Interaction::Hovered);
        assert!(!tracker.event(&moved(), true), "重复悬停不该再报变化");

        let press = Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left));
        assert!(tracker.event(&press, true));
        assert_eq!(tracker.state(), Interaction::Pressed);

        let release = Event::Mouse(iced::mouse::Event::ButtonReleased(
            iced::mouse::Button::Left,
        ));
        assert!(tracker.event(&release, true));
        assert_eq!(tracker.state(), Interaction::Hovered);

        assert!(tracker.set_focused(true));
        assert_eq!(tracker.state(), Interaction::Focused, "焦点态优先于悬停");

        let left = Event::Mouse(iced::mouse::Event::CursorLeft);
        tracker.event(&left, false);
        assert_eq!(tracker.state(), Interaction::Focused, "鼠标离开不影响焦点");
    }

    #[test]
    fn disabled_tracker_swallows_everything() {
        let mut tracker = Tracker::new(true);
        assert!(tracker.set_enabled(false));
        assert_eq!(tracker.state(), Interaction::Disabled);

        let press = Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left));
        assert!(!tracker.event(&press, true), "禁用态不响应事件");
        assert_eq!(tracker.state(), Interaction::Disabled);
        assert!(!tracker.set_focused(true), "禁用态拿不到焦点");
    }
}
