//! 拖拽与「缝」的判据（从 `tests.rs` 拆出来：那边加完复审 r1 的断言后顶到 436 行，
//! 而 AGENTS.md §3-1 是 400 行硬线）。
//!
//! 拆的位置是「指针/几何」这一族：拖拽三条收尾路径、吸附宽度写回、缝的预算、缝与折叠条的标签。

use unlucky_presenter::agent::AgentRail;

use super::super::metrics::{layout_for, PRESETS, RESIZE_GUTTER};
use super::super::state::{AgentUi, RailState};

use super::super::{docked, drag_overlay};
use super::ui;

/// 拖拽层只在拖拽期间存在（否则它会吃掉整窗的点击）。
#[test]
fn the_drag_overlay_only_exists_while_dragging() {
    let mut agent = AgentUi::new();
    agent.expand();
    assert!(drag_overlay(&agent).is_none());

    agent.drag_start();
    assert!(agent.is_dragging());
    assert!(drag_overlay(&agent).is_some());

    agent.drag_end();
    assert!(drag_overlay(&agent).is_none());
}

/// 拆出档右栗必须**不占地**（否则会话会在两个地方各画一份）。
#[test]
fn the_detached_state_frees_the_rail_slot() {
    let ui = ui();
    let mut agent = AgentUi::new();
    agent.detach();
    let session = super::super::Session {
        agent: &agent,
        rail: &AgentRail::new(),
        failure: None,
        simulated: true,
    };
    assert!(docked(
        &ui,
        session,
        unlucky_i18n::Catalog::of(unlucky_i18n::Lang::En),
        1180.0
    )
    .is_none());
    assert_eq!(agent.rail_width(1180.0), 0.0);
    assert_eq!(agent.content_width(1180.0), 1180.0);
}

/// 拖拽把「展开宽度」写进状态（松手后保有该宽度）。
#[test]
fn the_drag_writes_back_the_snapped_width() {
    let mut agent = AgentUi::new();
    agent.expand();
    agent.drag_start();
    agent.drag_to(1180.0, 1180.0 - 280.0);
    agent.drag_end();
    assert_eq!(agent.expanded(), 280.0);
    assert!(!agent.is_dragging());

    // 没按下就收到的 DragTo（例如上一步松手后的残留）不该改宽度
    let before = agent.expanded();
    agent.drag_to(1180.0, 100.0);
    assert_eq!(agent.expanded(), before);

    // 三档预设都能被拖到（1180 下上限 612 > 360）
    for preset in PRESETS {
        agent.drag_start();
        agent.drag_to(1180.0, 1180.0 - preset);
        agent.drag_end();
        assert_eq!(agent.expanded(), preset);
    }
}

/// 面板与中栗之间的缝在布局里被真的留出来了。
///
/// **全用字面量**（复审 P3-4：早先写成 `rail + GUTTER == window − content`，两边同一组常量，
/// 把 `RESIZE_GUTTER` 改成 0 照样绿 = 自证）。现在的算式是写死的：
/// `320（面板）+ 8（缝）+ 852（中栗）= 1180`。
#[test]
fn the_resize_gutter_is_part_of_the_budget() {
    let mut agent = AgentUi::new();
    agent.expand();

    assert_eq!(agent.rail_width(1180.0), 320.0, "1180 下展开档 = 令牌 320");
    assert_eq!(agent.content_width(1180.0), 852.0, "中栗 = 1180 − 320 − 8");
    assert_eq!(320.0 + 8.0 + 852.0, 1180.0, "三段之和必须回到窗口宽");
    assert_eq!(RESIZE_GUTTER, 8.0, "缝宽就是 8（`Space::X2`）");

    // 折叠档 48 / 拆出档 0：这两档**没有**缝（中栗 = 1180 − 48 / 1180 − 0）
    let collapsed = layout_for(1180.0, RailState::Collapsed, 320.0);
    assert_eq!(
        (collapsed.rail, collapsed.gutter, collapsed.content),
        (48.0, 0.0, 1132.0)
    );
    let detached = layout_for(1180.0, RailState::Detached, 320.0);
    assert_eq!(
        (detached.rail, detached.gutter, detached.content),
        (0.0, 0.0, 1180.0)
    );
}

/// 拖拽的三条收尾路径里，**能测的两条**（复审 P3-7）。
///
/// 真实鼠标在窗口外松手无法注入（这台机器连 `SetCursorPos` 都失败），所以这里只钉状态机：
/// ①松手（`DragEnd`）之后指针再动不改宽度；②拖拽中「再按一次」（叠层的 `on_press`）能清态，
/// 也就是上一条事件丢了的兜底。叠层本身挂了 `on_exit`（指针离窗 = 收在当前值），那条只能真窗口验。
#[test]
fn the_drag_recovers_when_a_release_is_missed() {
    let mut agent = AgentUi::new();
    agent.expand();

    // ① 正常松手
    agent.drag_start();
    agent.drag_to(1180.0, 1180.0 - 360.0);
    agent.drag_end();
    assert_eq!(agent.expanded(), 360.0);
    agent.drag_to(1180.0, 900.0);
    assert_eq!(agent.expanded(), 360.0, "松手后指针再动不该改宽度");

    // ② 拖拽中「再按一次」= 叠层的 on_press 兜底：清态，后续 DragTo 不再改宽度
    agent.drag_start();
    agent.drag_to(1180.0, 1180.0 - 280.0);
    assert_eq!(agent.expanded(), 280.0);
    agent.drag_end(); // ← 叠层 on_press → DragEnd
    assert!(
        !agent.is_dragging(),
        "再按一次必须能清态（窗内松手丢失时的兜底）"
    );
    agent.drag_to(1180.0, 100.0);
    assert_eq!(agent.expanded(), 280.0);
    assert!(drag_overlay(&agent).is_none(), "清态后叠层必须消失");
}

/// 折叠条与拖拽缝都有可读文字（复审 P3-5：两条 key 以前在视图里一次没用）。
///
/// 抓得住：谁把 `AgentResize` / `AgentWidthLabel` 从视图里删掉（那样它们又变回空头支票）。
/// 抓不住：tooltip 真弹出来长什么样（那要 hover 出图）。
#[test]
fn the_gutter_and_strip_are_labelled() {
    let hosts = include_str!("../mod.rs");
    let strip = include_str!("../strip.rs");
    for key in ["Key::AgentResize", "Key::AgentWidthLabel"] {
        assert!(hosts.contains(key), "视图里没用上 {key}");
    }
    // M119：折叠条整段搬去了 `strip.rs`（`mod.rs` 顶到 400 行）
    assert!(strip.contains("Key::AgentStripLabel"), "折叠条上没画标签");
}
