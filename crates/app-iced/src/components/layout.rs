//! 布局陷阱的两个固定解法（D39 §4）：**要精确尺寸就调这里，别手写**。
//!
//! ## 陷阱 Q8：`center_x(Length::Fill)` 会把已设的 `.width()` 覆盖成 Fill
//!
//! iced 的 `container::center_x/center_y/center` 不只是「对齐」——它们**同时**把容器该轴的
//! `Length` 改成 `Fill`（源码里 `self.width = Length::Fill`）。于是
//! `.width(56).center_x(Length::Fill)` 最终是 `Fill`，56 被父级宽度吃掉：
//! M68 的导航药丸因此从 **56×32 变成 88×32**（父级是 88 宽的轨），图上看着「横贯全轨」，
//! 而单元测试测的 `indicator_size()` 返回值一直是 56×32 —— **函数对、图错**。
//!
//! 正确写法是二选一：
//!
//! - 尺寸留在**自己**身上，居中交给**父级**（`row/column` 的 `align_x`）——`shell/nav.rs` 的做法；
//! - 或者用 [`centered_fixed`]：容器写死宽高，**用 `align_x/align_y` 居中**（它们只改对齐，
//!   不改 `Length`）。
//!
//! ## 陷阱 Q9：`Button` 给内容 Fill 约束，`svg().width(24)` 会被忽略
//!
//! `button` 在 layout 里把「内容可用区」作为约束交给子元素，`svg` 拿到的是 Fill，
//! usvg 于是按 `Contain` 撑满内区：**字形渲染成 46px、线宽从 1.5 变 2.9**（M68 视觉复审 P2-2），
//! 比导航图标重一倍。反过来「把 `.width(24)` 设在 button 上」也不行——那会缩掉点击区。
//!
//! 正确写法：**在中间插一层固定尺寸容器**——[`fixed`]：
//!
//! ```text
//! let icon = layout::fixed(24.0, 24.0, icons::view(kind, color));   // 字形 24×24
//! button(icon).width(46).height(48)                                 // 命中区仍是 46×48
//! ```
//!
//! ## 为什么还要 [`inset`]
//!
//! 「左右缩进 16 的一条线」「内容四周留 24」这类**只改内边距、不改尺寸**的语义，
//! 写 `.padding(Padding::from([...]))` 时数组顺序（上右下左）极易写反；封装一次，
//! 全组件线只写一次顺序。
//!
//! ## 自检
//!
//! `components::gate` 会扫 `src/components/**`：出现 `center_x(` / `center_y(` / `.center(`
//! 即判红（这条门禁就是 Q8 的回归测试）；图标一律经 [`fixed`] / [`centered_fixed`] 包一层。

use iced::widget::container;
use iced::{Alignment, Element, Length, Padding};

/// 固定尺寸的盒子：宽高**只由参数决定**，子元素贴左上（对齐交给子元素自己或父级）。
///
/// Q9 的解法：`svg` 放进 24×24 的盒子，它拿到的约束就是 24，`Contain` 无从撑满。
pub fn fixed<'a, M: 'a>(
    width: f32,
    height: f32,
    content: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    container(content)
        .width(Length::Fixed(width))
        .height(Length::Fixed(height))
        .into()
}

/// 固定尺寸 + 内容居中（Q8 的解法）。
///
/// 用 `align_x/align_y` 而不是 `center_x/center_y`：前者只改对齐，后者会把本容器的
/// `Length` 改成 `Fill`，把参数里的宽高**静默丢掉**。
///
/// 典型用法：导航轨的选中指示器 [`centered_fixed(56.0, 32.0, icon)`](centered_fixed)。
pub fn centered_fixed<'a, M: 'a>(
    width: f32,
    height: f32,
    content: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    container(content)
        .width(Length::Fixed(width))
        .height(Length::Fixed(height))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
}

/// 只加内边距、不碰尺寸（`horizontal` = 左右，`vertical` = 上下）。
///
/// 语义等价于 `.padding(Padding { top: vertical, right: horizontal, bottom: vertical,
/// left: horizontal })`，但只暴露两个参数——避免把「上右下左」写反。
pub fn inset<'a, M: 'a>(
    horizontal: f32,
    vertical: f32,
    content: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    container(content)
        .padding(Padding {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        })
        .into()
}

/// 单侧缩进（只缩左侧；分隔线的「列表内左缩进 16」用这个）。
pub fn inset_start<'a, M: 'a>(start: f32, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    container(content)
        .width(Length::Fill)
        .padding(Padding {
            left: start,
            ..Padding::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Q8 的**形态级**钉法：这三个 API 一旦内部改用 `center_*`，这条测试之外的门禁
    /// （`components::gate`）也会红；这里再钉一次「参数就是参数」，防止签名被悄悄改成
    /// `impl Into<Length>` 之类。
    #[test]
    fn helpers_take_pixel_sizes_not_lengths() {
        let _: fn(f32, f32, Element<'static, ()>) -> Element<'static, ()> = fixed;
        let _: fn(f32, f32, Element<'static, ()>) -> Element<'static, ()> = centered_fixed;
        let _: fn(f32, f32, Element<'static, ()>) -> Element<'static, ()> = inset;
        let _: fn(f32, Element<'static, ()>) -> Element<'static, ()> = inset_start;
    }

    #[test]
    fn helpers_build_a_tree_without_panicking() {
        let _: Element<'_, ()> = fixed(24.0, 24.0, iced::widget::text("x"));
        let _: Element<'_, ()> = centered_fixed(56.0, 32.0, iced::widget::text("x"));
        let _: Element<'_, ()> = inset(16.0, 8.0, iced::widget::text("x"));
        let _: Element<'_, ()> = inset_start(16.0, iced::widget::text("x"));
    }
}
