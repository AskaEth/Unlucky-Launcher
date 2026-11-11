//! **测试专用**：记录「画一帧时从词表取出的每一条译文」。
//!
//! ## 为什么需要这一层
//!
//! 判据是「中文界面下外观分组渲染出来的字里必须含汉字」，但 iced 0.14 **没有**可以遍历控件树的
//! 公开接口：
//!
//! - `Element::as_widget()` 拿到的是 `&dyn Widget`，而 `Widget::children()` 返回的是**状态** `Tree`
//!   （不是子控件），没有 `as_any` 之类的向下转型口；
//! - 想走 `Widget::operate` 得自己造 `Tree` + 跑布局 + 传 `Renderer`，而单测里**没有渲染器**
//!   （`pages/download/tests/options_screen.rs` 记的正是这条边界）。
//!
//! 所以取证点退到「字进树之前」那一层：`section::t` / `section::tr` 是本分组**唯一**两个取词口，
//! 它们把每条译文顺手记一份（M180 起分成两份：`t()` 的 `&'static str` 与 `tr()` 替换完占位符的
//! 整句），测试再断言这批字里有没有汉字。这不是「看源码字符串」那种假守卫——它要求真的把
//! [`crate::pages::appearance::section`] 跑一遍（并且走 `settings::view` 的语言分发），只是不画像素。
//!
//! ## 为什么是 thread-local
//!
//! `cargo test` 默认多线程跑，每个测试一个线程；thread-local 让并发测试的取证互不串台，
//! 也不需要加锁或串行化。
//!
//! 整个模块只在 `#[cfg(test)]` 下编译，产品路径零代价。

use std::cell::RefCell;

use unlucky_i18n::catalog::Key;

// 自上次 `take` 以来记录下的 `(key, 译文)`，顺序 = 取词顺序（`t()` 取的 `&'static str`）。
thread_local! {
    // clippy 1.99.1（= 本仓 CI 的 stable）对这条 lint **误报**：初始化器已经写成 `const { … }`
    // 它还是报「can be made const」。最小复现（clippy-driver 原地可验，无本仓任何符号）：
    // `thread_local!{ static X: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) }; }`
    // —— 同一个文件加不加 `const` 报同一句。等 clippy 修掉这条误报即可删掉本行 allow。
    #[allow(clippy::missing_const_for_thread_local)]
    static TEXTS: RefCell<Vec<(Key, &'static str)>> = const { RefCell::new(Vec::new()) };
}

// 自上次 `take_rendered` 以来记录下的 `(key, 整句)`：`tr()` 那条带占位符的路径（M180 加）。
//
// 为什么单独一份而不是把上面那份改成 `String`：M179 的守卫
// （`pages/settings/tests/appearance_section.rs::the_appearance_group_speaks_chinese_when_the_interface_does`）
// 断言的是 `&'static str`，那个文件不在 M180 的 scope 里、不能跟着改签名。两份记录各自的
// `take*` 也各自清空，互不干扰。
thread_local! {
    #[allow(clippy::missing_const_for_thread_local)]
    static RENDERED: RefCell<Vec<(Key, String)>> = const { RefCell::new(Vec::new()) };
}

/// 记一条（由 [`super::section::t`] 调用；不对外）。
pub(crate) fn record(key: Key, text: &'static str) {
    TEXTS.with(|texts| texts.borrow_mut().push((key, text)));
}

/// 记一条**替换完占位符**的整句（由 [`super::section::tr`] 调用；不对外）。
pub(crate) fn record_rendered(key: Key, text: String) {
    RENDERED.with(|texts| texts.borrow_mut().push((key, text)));
}

/// 取走并清空本线程记录下的取词（测试用）。
pub(crate) fn take() -> Vec<(Key, &'static str)> {
    TEXTS.with(|texts| std::mem::take(&mut *texts.borrow_mut()))
}

/// 取走并清空本线程记录下的整句（测试用）。
pub(crate) fn take_rendered() -> Vec<(Key, String)> {
    RENDERED.with(|texts| std::mem::take(&mut *texts.borrow_mut()))
}
