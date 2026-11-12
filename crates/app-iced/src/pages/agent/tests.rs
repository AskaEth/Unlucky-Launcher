//! 三档状态机、两个宿主、a11y 文案的判据（数值表与磁铁在 `metrics/tests.rs`）。
//!
//! 每条断言都写清「抓得住什么 / 抓不住什么」——纯函数与源码扫描能钉住语义，**钉不住**像素。
//!
//! M119 起这里的重心变了：会话内容不再由本模块持有，所以「提交会怎样」这类端到端断言搬到了
//! `app/update/agent/tests.rs`（那里能拿到 presenter 句柄）。这里留的是**纯 UI 态**与**视图契约**。

use unlucky_design::color::SchemeFlavor;
use unlucky_design::typography::Platform;
use unlucky_i18n::{Catalog, Key, Lang};

use unlucky_presenter::agent::AgentRail;
use unlucky_presenter::dto::{ChatMessage, ChatRole, ConfirmRequest, Sensitivity};

use crate::theme::{Families, Ui};

mod disclaimer;
mod drag;

use super::conversation::{Conversation, Host};
use super::state::{AgentUi, Focus, RailState};
use super::*;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

pub(super) fn ui() -> Ui {
    Ui::with_platform(SchemeFlavor::Light, Platform::Windows, FAMILIES)
}

/// 一条只给视图用的 rail（会话内容由 presenter 生产，这里只造**渲染输入**）。
fn rail(messages: Vec<ChatMessage>, unread: usize, busy: bool) -> AgentRail {
    AgentRail {
        messages,
        unread,
        busy,
        ..AgentRail::new()
    }
}

fn conversation_rail() -> AgentRail {
    rail(
        vec![
            ChatMessage::user("Which instance should I start?"),
            ChatMessage {
                role: ChatRole::Assistant,
                text: "Survival is on 1.20.4.".to_string(),
                tool: None,
                streaming: false,
            },
        ],
        2,
        false,
    )
}

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

/// 空会话：零消息 + 一句引导文案（任务第 7 条）。
#[test]
fn the_empty_session_has_a_guidance_line() {
    let catalog = Catalog::of(Lang::En);
    assert!(!catalog.text(Key::AgentEmptyTitle).is_empty());
    assert!(
        catalog.text(Key::AgentEmptyBody).split_whitespace().count() >= 5,
        "空态要有一句能读懂下一步的引导，而不是一个词"
    );
    // 中文也得有且不同（D28：两套目录一行一个 key，缺译会红）
    let zh = Catalog::of(Lang::ZhCn);
    assert_ne!(
        zh.text(Key::AgentEmptyBody),
        catalog.text(Key::AgentEmptyBody)
    );
}

/// **单实现判据**：右栗与独立页都只通过 `conversation::view` 画对话。
///
/// 抓得住：有人把「独立页要更宽」当成理由另写一份气泡/输入框装配（复制粘贴是这类任务最常见的坏法）。
/// 抓不住：`conversation::view` **内部**再分叉成两套（那要人读那个文件）。
#[test]
fn both_hosts_render_the_same_conversation() {
    let hosts = include_str!("mod.rs");
    let conversation = include_str!("conversation.rs");
    let composer = include_str!("conversation/composer.rs");

    assert_eq!(
        conversation.matches("pub fn view<").count(),
        1,
        "会话视图只能有一个 `view`"
    );
    assert_eq!(
        hosts.matches("conversation::view(").count(),
        2,
        "右栗与独立页各调用一次同一个 `conversation::view`（不多不少）"
    );
    assert!(
        !hosts.contains("text_editor("),
        "输入框的装配只许在 conversation/composer.rs 里（宿主不许自己拼一份）"
    );
    assert!(
        !hosts.contains("scrollable("),
        "滚动区的装配只许在 conversation/transcript.rs 里"
    );
    assert!(
        composer.contains("text_editor(") || composer.contains("editor(spec.draft)"),
        "输入框就在 composer 里"
    );
    // M119：会话内容的唯一来源是 presenter 的 rail ⇒ 宿主把 rail 借进来，而不是自己造
    assert!(
        !hosts.contains("Vec<ChatMessage>") && !hosts.contains("Vec<Turn>"),
        "宿主里不许出现自持的会话容器"
    );
}

/// a11y：所有动作按钮的标签都来自词表，折叠条也不是「只有图标」。
///
/// 抓得住：有人写死一句英文/中文在视图里（"Cancel"/"发送"），或给折叠条只留一个图标按钮。
/// 抓不住：译文本身好不好读（那要人看）。
#[test]
fn every_action_has_a_readable_label() {
    let catalog = Catalog::of(Lang::En);
    for key in [
        Key::AgentCollapse,
        Key::AgentDetach,
        Key::AgentDock,
        Key::AgentOpen,
        Key::AgentSend,
        Key::AgentStop,
        Key::AgentStripLabel,
        Key::AgentResize,
        Key::AgentWidthLabel,
        // M119 新增：确认条的两个动作 + 失败横幅的标题
        Key::AgentConfirmAllow,
        Key::AgentConfirmDeny,
        Key::AgentConfirmRequest,
        Key::AgentErrorTitle,
        Key::AgentErrorUnavailable,
        Key::AgentErrorNotImplemented,
        // M119 r1 / P2-1：「这是模拟」的常驻标记也必须有词（用户不点就要能看到）
        Key::AgentSimulated,
        Key::AgentSubtitle,
    ] {
        assert!(
            !catalog.text(key).trim().is_empty(),
            "{} 没有可读标签",
            key.name()
        );
    }

    for source in [
        include_str!("conversation.rs"),
        include_str!("conversation/composer.rs"),
        include_str!("conversation/banners.rs"),
        include_str!("conversation/transcript.rs"),
        include_str!("strip.rs"),
        include_str!("mod.rs"),
    ] {
        for literal in ["Button::new(\"", "Button::disabled(\""] {
            assert!(
                !source.contains(literal),
                "按钮标签不许写死在视图里：{literal}"
            );
        }
        assert!(
            !source.contains("text(\"Assistant\""),
            "文案不许写死在视图里（走 i18n 的 Key）"
        );
    }

    // 折叠条的 48px 条上，图标下面必须有文字标签（否则就是「只有图标」）
    assert!(
        include_str!("strip.rs").contains("Key::AgentStripLabel"),
        "折叠条必须有可读的条上标签"
    );
}

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

/// 视图契约：`spec()` 把 presenter 的三件事**原样借**给视图（零拷贝、不复制）。
///
/// 抓得住：有人在 `spec()` 里换掉/派生一份会话（那就会有两份真相）。
/// 抓不住：`Conversation` 字段在视图里怎么用（那要人读 conversation.rs）。
#[test]
fn the_spec_borrows_the_presenter_rail_verbatim() {
    let mut agent = AgentUi::new();
    agent.expand();
    let rail = conversation_rail();
    let session = Session {
        agent: &agent,
        rail: &rail,
        failure: None,
        simulated: true,
    };
    let spec = super::spec(session, Catalog::of(Lang::En), Host::Rail);
    assert_eq!(spec.messages.len(), rail.messages.len());
    assert_eq!(spec.messages[0].text, rail.messages[0].text, "同一条消息");
    assert_eq!(spec.unread, rail.unread);
    assert_eq!(spec.busy, rail.busy);
    assert!(spec.failure.is_none());
    assert!(
        spec.note.simulated,
        "模拟事实要原样传给视图（标记不是硬编码）"
    );
    assert!(
        spec.note.subtitle.to_lowercase().contains("simulated"),
        "副标题也归这个开关管：{}",
        spec.note.subtitle
    );
}

/// 两个宿主都能构图（含空会话 / 有会话 / 流式中 / 待确认 / 失败横幅），且吃 880~2560 的宽度。
///
/// 抓得住：某档位下 `view` 的某条分支 panic（例如空消息 + 非零未读、确认条 + 失败横幅同时出现）。
/// 抓不住：像素级观感（出图另算）。
#[test]
fn every_state_builds_in_both_hosts() {
    let ui = ui();
    let catalog = Catalog::of(Lang::En);
    let empty = AgentRail::new();
    let full = conversation_rail();
    let busy = rail(conversation_rail().messages, 0, true);
    let failed = AgentRail {
        error: Some("provider 断流：read timeout".to_string()),
        ..conversation_rail()
    };
    let mut confirm_rail = conversation_rail();
    confirm_rail.confirm = Some(ConfirmRequest {
        id: "req-1".to_string(),
        tool: "set_config".to_string(),
        summary: "把下载并发 64 → 16".to_string(),
        details: Default::default(),
        sensitivity: Sensitivity::High,
    });
    let failure = Failure {
        title: Key::AgentErrorTitle,
        body: "The assistant backend is unreachable.",
    };

    for window in [880.0, 900.0, 1180.0, 2560.0] {
        for rail in [&empty, &full, &busy, &failed, &confirm_rail] {
            for state in RailState::ALL {
                let mut agent = AgentUi::new();
                agent.rail = state;
                for shown in [None, Some(failure)] {
                    let session = Session {
                        agent: &agent,
                        rail,
                        failure: shown,
                        simulated: true,
                    };
                    let _: iced::Element<'_, Message> = page(&ui, session, catalog, window);
                    let _ = docked(&ui, session, catalog, window);
                    let spec = Conversation {
                        messages: &rail.messages,
                        draft: &agent.draft,
                        busy: rail.busy,
                        unread: rail.unread,
                        blank: agent.draft_is_blank(),
                        confirm: rail.confirm.as_ref(),
                        failure: shown,
                        note: Disclaimer::of(catalog, true),
                        rail: agent.rail,
                        host: Host::Rail,
                        focus: agent.keyboard_mode.then_some(agent.focus),
                    };
                    let _: iced::Element<'_, Message> = conversation::view(&ui, spec, catalog);
                }
            }
        }
    }
}
