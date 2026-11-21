//! Tab 环：只在**真正存在**的控件之间走，且顺序自左向右可预期。
//!
//! M393 从 `tests.rs` 原样搬出来，只为行数（那个文件撞了 400 行硬线）。断言、注释与文案一字未动，
//! 只有导入换成显式路径（子模块的 `super` 是 `tests.rs`，`super::super` 才是 `agent`）。

use super::super::state::{AgentUi, Focus, RailState};

/// Tab 环：只在**真正存在**的控件之间走，且顺序自左向右可预期。
#[test]
fn the_focus_ring_walks_the_controls_that_exist() {
    // 展开且空闲：收起 → 拆出 → 发送 → 输入框（没有中断键）
    let mut focus = Focus::Collapse;
    let mut walked = vec![focus];
    for _ in 0..4 {
        focus = focus.step(false, RailState::Expanded, false, false);
        walked.push(focus);
    }
    assert_eq!(
        walked,
        vec![
            Focus::Collapse,
            Focus::Detach,
            Focus::Send,
            Focus::Composer,
            Focus::Collapse,
        ]
    );

    // 反向：Collapse 往回走是 Composer
    assert_eq!(
        Focus::Collapse.step(true, RailState::Expanded, false, false),
        Focus::Composer
    );

    // 流式中多一档中断键（`busy` 由 presenter 的 rail 给）
    assert_eq!(
        Focus::Send.step(false, RailState::Expanded, true, false),
        Focus::Stop
    );

    // 独立页只有「收回 + 输入区」
    assert_eq!(
        Focus::Detach.step(false, RailState::Detached, false, false),
        Focus::Send
    );

    // 折叠档没有会话动作：环上只有「展开」一格
    assert_eq!(
        Focus::Collapse.step(false, RailState::Collapsed, false, false),
        Focus::Collapse
    );

    // `Tab` 真的把键盘巡航打开（`Enter` 的闸门）
    let mut agent = AgentUi::new();
    assert!(!agent.keyboard_mode);
    agent.step_focus(false, false, false);
    assert!(agent.keyboard_mode);

    // M119 r1 / P2-2：有待确认写操作时，环上多两格（允许 / 拒绝），且**排在发送之前**——
    // 阻塞态该先被看见。这也是键盘用户唯一能推进这一轮的路径。
    assert_eq!(
        Focus::Detach.step(false, RailState::Expanded, true, true),
        Focus::ConfirmAllow
    );
    assert_eq!(
        Focus::ConfirmAllow.step(false, RailState::Expanded, true, true),
        Focus::ConfirmDeny
    );
    assert_eq!(
        Focus::ConfirmDeny.step(false, RailState::Expanded, true, true),
        Focus::Send
    );
    // 没有待确认时这两格不在环上（鼠标用户不该 Tab 到不存在的按钮）
    assert_eq!(
        Focus::Detach.step(false, RailState::Expanded, false, false),
        Focus::Send
    );
}
