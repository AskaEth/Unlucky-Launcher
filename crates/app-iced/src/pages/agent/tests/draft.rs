//! 草稿的**两个写口**（`edit` 打字 / `clear_draft` 发送成功之后）与三档迁移时草稿跟着走。
//!
//! M393 从 `tests.rs` 原样搬出来，只为行数（那个文件撞了 400 行硬线）。断言、注释与文案一字未动，
//! 只有导入换成显式路径（子模块的 `super` 是 `tests.rs`，`super::super` 才是 `agent`）。

use super::super::state::{AgentUi, RailState};

/// 三档迁移：**草稿与焦点**跟着走（会话在 presenter 上，本来就不归这里管）。
///
/// 抓得住：`expand()/detach()/dock()` 把档位改对且不清草稿；`collapse()` 对 Detached 等价于 `dock()`。
/// 抓不住：切换时有没有一次「闪一下」的重排（那要出图看）。
#[test]
fn the_three_states_share_one_draft() {
    let mut agent = AgentUi::new();
    assert_eq!(agent.rail, RailState::Collapsed, "出厂是折叠档");

    agent.expand();
    assert_eq!(agent.rail, RailState::Expanded);

    // 走 `AgentUi::edit`（= app 层真正的落点），而不是直接操作 iced 的 `Content`：
    // 走到这里的 `Edit(Enter)` 只可能是 Shift+Enter（非 shift 的 Enter 在 composer 的
    // `key_binding` 里就变成 `Message::Submit` 了）。
    agent.draft = iced::widget::text_editor::Content::with_text("kick off Survival");
    agent.edit(iced::widget::text_editor::Action::Edit(
        iced::widget::text_editor::Edit::Enter,
    ));
    assert_eq!(
        agent.draft.line_count(),
        2,
        "多行：一条 Edit(Enter) 之后成两行"
    );

    let draft = agent.draft.text();
    agent.detach();
    assert_eq!(agent.rail, RailState::Detached);
    assert_eq!(agent.draft.text(), draft, "拆出去不丢草稿");

    agent.dock();
    assert_eq!(agent.rail, RailState::Expanded);
    assert_eq!(agent.draft.text(), draft);

    // 收起 / Esc：只改档位
    agent.collapse();
    assert_eq!(agent.rail, RailState::Collapsed);
    // 独立页上的 Esc = 收回（`collapse` 对 Detached 等价于 dock）
    agent.rail = RailState::Detached;
    agent.collapse();
    assert_eq!(agent.rail, RailState::Expanded);
}

/// 草稿只有两个写口：`edit`（打字）与 `clear_draft`（发送成功之后）。
#[test]
fn sending_clears_the_draft_and_blank_is_trimmed() {
    let mut agent = AgentUi::new();
    assert!(agent.draft_is_blank());
    agent.draft = iced::widget::text_editor::Content::with_text("   ");
    assert!(agent.draft_is_blank(), "只有空白也算空（发送键是禁用态）");
    agent.draft = iced::widget::text_editor::Content::with_text("  hello  ");
    assert!(!agent.draft_is_blank());
    agent.clear_draft();
    assert!(agent.draft_is_blank(), "发出去后草稿清空");
}
