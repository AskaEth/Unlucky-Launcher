//! 三档状态机、两个宿主、a11y 文案的判据（数值表与磁铁在 `metrics/tests.rs`）。
//!
//! 每条断言都写清「抓得住什么 / 抓不住什么」——纯函数与源码扫描能钉住语义，**钉不住**像素。
//!
//! M119 起这里的重心变了：会话内容不再由本模块持有，所以「提交会怎样」这类端到端断言搬到了
//! `app/update/agent/tests.rs`（那里能拿到 presenter 句柄）。这里留的是**纯 UI 态**与**视图契约**。
//!
//! M393 只为行数：草稿两条（`tests/draft.rs`）与 Tab 环（`tests/focus.rs`）原样搬进子模块。

use unlucky_design::color::SchemeFlavor;
use unlucky_design::typography::Platform;
use unlucky_i18n::{Catalog, Key, Lang};

use unlucky_presenter::agent::AgentRail;
use unlucky_presenter::dto::{ChatMessage, ChatRole, ConfirmRequest, Sensitivity};

use crate::theme::{Families, Ui};

mod collapse;
mod disclaimer;
mod draft;
mod drag;
mod focus;

use super::conversation::{Conversation, Host};
use super::state::{AgentUi, RailState};
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
    // M257：右栗面板整段搬去了 `panel.rs`，所以「两个宿主各调一次」的扫描面是两个文件。
    let hosts = concat!(include_str!("mod.rs"), include_str!("panel.rs"));
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
