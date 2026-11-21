//! 判据：弹窗里**每一个**按钮都带着 Overlay 面板的底（M449 修的 M234 那条洞）。
//!
//! ## 两条断言、两个方向
//!
//! 1. **值**：真造一个按钮，看它身上那个 `base` 是什么。`Button` 的字段是
//!    `components::buttons` 私有的（本模块读不到），而 `Button` 又实现了 `Debug`——`Debug` 是
//!    字段的一一投影，所以判据落在它的文本上：把底换回默认值，这段文字就会变成 `Base(Surface)`。
//! 2. **接线**：本文件里没有**第二个**按钮构造点绕过 [`panel_button`]。这条走源码扫描，因为
//!    「调用点忘了 `.base(..)`」在编译期不报错（默认值兜着），运行时也看不出来（`Element` 不可
//!    内省）——M234 的洞就是这么活了一百多个 mission 的。
//!
//! ## 抓得住 / 抓不住
//!
//! - 抓得住：①`panel_button` 不再给 Overlay 底（两问都红）；②有人在本文件里直接 `Button::new(..)`
//!   而不给 `.base(..)`（第二问红）；③`Base::OVERLAY` 本身被改回画布角色（
//!   `interaction::base::tests` 那条红）。
//! - 抓不住：①**像素**——悬停/按下/禁用三态到底差多少级，见 `DevDocs/artifacts/m449/`；
//!   ②源码扫描只认本文件：别处（例如新写的弹窗子视图）绕过 `panel_button` 它看不见；
//!   ③面板 α < 1 时状态层仍按不透明角色色合成（`Base` 只装一个 `Role`，已登记的边界）。

use crate::app::dialog::Message as DialogMessage;
use crate::app::message::Message;

use super::panel_button;

/// 本文件的源码（`include_str!` 相对**本文件**所在目录解析 ⇒ `../actions.rs`）。
const SOURCE: &str = include_str!("../actions.rs");

/// 数源码里的构造点时先剔掉整行注释：文档注释里出现 `Button::new` 是常事，不剔就是假绿。
fn code_only(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// **变异原文（改一处）**：删掉 `actions.rs` 里 `panel_button` 尾部的 `.base(Base::OVERLAY)`
/// （或把它换成 `.base(Base::CANVAS)`）⇒ 期望红在本文件的 `assert_eq!(based, made, …)`
/// 与 `assert!(printed.contains("Base(SurfaceContainerHigh)"), …)`。
#[test]
fn every_dialog_button_carries_the_overlay_base() {
    let printed = format!(
        "{:?}",
        panel_button("确定", Message::Dialog(DialogMessage::Primary))
    );
    assert!(
        printed.contains("Base(SurfaceContainerHigh)"),
        "弹窗按钮的底必须是 Overlay 面板（`Base::OVERLAY` = SurfaceContainerHigh），实际：{printed}"
    );
    assert!(
        !printed.contains("Base(Surface)"),
        "弹窗按钮不许回落到画布底 `Base::CANVAS`（那是 M449 修的那条洞）：{printed}"
    );

    let code = code_only(SOURCE);
    let made = code.matches("Button::new(").count() + code.matches("Button::disabled(").count();
    let based = code.matches(".base(Base::OVERLAY)").count();
    assert_eq!(
        made, 1,
        "本文件只许有一个按钮构造入口（`panel_button`），实际 {made} 个"
    );
    assert_eq!(
        based, made,
        "每个按钮构造点都必须紧跟 `.base(Base::OVERLAY)`：{made} 个构造点、{based} 处给底"
    );
}
