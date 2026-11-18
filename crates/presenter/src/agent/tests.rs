//! `agent` 模块的单元测试：四条路径（正常问答 / 流式中途出错 / 需确认的写操作 / 未读计数）+
//! YOLO 语义 + 提交前置条件 + 投影健壮性。
//!
//! 红线（凭据形状的串不得进 rail）单列在 `agent/tests_red_line.rs`（单文件 ≤400 行）。
//! 断言口径照 M103 的要求写清「抓得住什么 / 抓不住什么」：这些测试盯的是**投影与裁决**，
//! 不盯真实模型的措辞、真实时序与网络错误——那些要接 rig 之后跑回放回归。

use super::mock::{MockAgentTransport, Script};
use super::{AgentTransport, ConfirmVerdict, Presenter};
use crate::agent::rail::scrub_text;
use crate::agent::AgentRail;
use crate::dto::{AgentEvent, AppEvent, ChatMessage, ChatRole, ConfirmRequest, Sensitivity};

fn presenter(script: Script) -> Presenter {
    Presenter::new(Box::new(MockAgentTransport::new(script)))
}

// ───────────────────────── 路径 1：正常问答 ─────────────────────────

#[test]
fn normal_answer_accumulates_into_one_assistant_message() {
    let mut presenter = presenter(Script::Qa);
    assert!(presenter.agent_submit("怎么装 1.20.4".to_string()).is_ok());

    let rail = presenter.agent_rail();
    // 抓得住：用户气泡 + 一条助手消息（增量累积在同一条上，不是一个字一个气泡）
    assert_eq!(rail.messages.len(), 2);
    assert_eq!(rail.messages[0].role, ChatRole::User);
    assert_eq!(rail.messages[0].text, "怎么装 1.20.4");
    assert_eq!(rail.messages[1].role, ChatRole::Assistant);
    assert_eq!(
        rail.messages[1].text,
        "先确认版本：官方 1.20.4 是正式版，装完再加 Fabric。"
    );
    assert!(!rail.messages[1].streaming);
    assert!(!rail.streaming);
    assert!(!rail.busy);
    assert!(rail.confirm.is_none());
    assert!(rail.error.is_none());
    // 抓不住：不校验助手措辞是否正确（那是工具/模型的事）
}

// ─────────────────── 路径 2：流式中途出错 ───────────────────

#[test]
fn mid_stream_error_keeps_partial_text_and_flags_the_turn() {
    let mut presenter = presenter(Script::FailsMidStream);
    assert!(presenter.agent_submit("日志里有错吗".to_string()).is_ok());

    let rail = presenter.agent_rail();
    // 抓得住：已到的增量不丢、流式收敛（不会永远转圈）、错误进横幅也进会话历史
    assert_eq!(rail.messages[1].role, ChatRole::Assistant);
    assert_eq!(rail.messages[1].text, "我先读一下最近的日志");
    assert!(!rail.streaming);
    assert!(!rail.busy, "出错后输入框必须重新可用");
    assert_eq!(rail.error.as_deref(), Some("provider 断流：read timeout"));
    assert_eq!(rail.messages[2].role, ChatRole::System);
    assert_eq!(rail.messages[2].text, "provider 断流：read timeout");
    // 抓不住：不判断这是网络问题还是模型问题（归因在 core::diagnose）
}

// ───────────── 路径 3a/3b：需确认的写操作（接受 / 拒绝）─────────────

#[test]
fn write_op_accept_applies_and_audits() {
    let mut presenter = presenter(Script::WriteOp);
    assert!(presenter.agent_submit("并发调小点".to_string()).is_ok());

    // 确认之前：rail 挂着请求，且 UI 靠 busy 禁用输入
    {
        let rail = presenter.agent_rail();
        assert!(rail.busy);
        let request = rail.confirm.as_ref().expect("应当有待确认请求");
        assert_eq!(request.tool, "set_config");
        assert_eq!(request.details["path"], "download.concurrency");
        assert!(request.sensitivity.needs_confirm());
        assert_eq!(rail.messages.len(), 3); // user / assistant / tool
        assert_eq!(rail.messages[2].role, ChatRole::Tool);
    }

    presenter.agent_confirm(true);
    let rail = presenter.agent_rail();
    assert!(rail.confirm.is_none());
    assert!(!rail.busy);
    assert_eq!(rail.messages[2].text, "已写入 download.concurrency = 16");
    assert!(rail.messages[2].tool.as_ref().is_some_and(|tool| tool.ok));
    assert_eq!(
        rail.messages.last().map(|message| message.role),
        Some(ChatRole::Assistant)
    );

    let audit = presenter.audit();
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].verdict, ConfirmVerdict::Approved);
    assert!(!audit[0].yolo && audit[0].confirmed, "用户亲手点的允许");
    assert_eq!(audit[0].sensitivity, Sensitivity::High);
}

#[test]
fn write_op_reject_denies_and_audits() {
    let mut presenter = presenter(Script::WriteOp);
    assert!(presenter.agent_submit("并发调小点".to_string()).is_ok());
    presenter.agent_confirm(false);

    let rail = presenter.agent_rail();
    assert!(rail.confirm.is_none());
    assert!(!rail.busy);
    assert_eq!(rail.messages[2].text, "用户拒绝，未写入");
    assert!(rail.messages[2].tool.as_ref().is_some_and(|tool| !tool.ok));
    assert_eq!(
        rail.messages.last().map(|text| text.text.clone()),
        Some("那就不改了。".to_string())
    );

    let audit = presenter.audit();
    assert_eq!(audit[0].verdict, ConfirmVerdict::Denied);
    assert!(!audit[0].yolo && audit[0].confirmed);
}

// ─────────────────────── 路径 4：未读计数 ───────────────────────

#[test]
fn unread_grows_while_collapsed_and_mark_read_clears() {
    let mut presenter = presenter(Script::WriteOp);
    presenter.state_mut().prefs.ai_collapsed = true; // 收成 icon 轨 = 用户离开了栏
    presenter
        .agent_submit("并发调小点".to_string())
        .expect("提交应当成功");

    // 抓得住：助手消息 + 工具条目 + 待确认各算一条；用户自己的话与纯增量不算
    assert_eq!(presenter.agent_rail().unread, 3);

    presenter.agent_confirm(true);
    // 工具收尾不是新内容，收尾的助手消息算一条
    assert_eq!(presenter.agent_rail().unread, 4);

    presenter.agent_mark_read();
    assert_eq!(presenter.agent_rail().unread, 0);
}

// ───────────────────────── YOLO 语义 ─────────────────────────

#[test]
fn yolo_auto_accepts_but_still_records_the_verdict() {
    let mut presenter = presenter(Script::WriteOp);
    presenter.state_mut().ai.yolo = true;
    presenter
        .agent_submit("并发调小点".to_string())
        .expect("提交应当成功");

    // 用户点了「拒绝」，但 YOLO 下自动放行
    presenter.agent_confirm(false);

    let rail = presenter.agent_rail();
    assert!(rail.confirm.is_none());
    assert_eq!(rail.messages[2].text, "已写入 download.concurrency = 16");

    // 不得静默跳过：审计里必须有这一条，且两种标记分开记
    let audit = presenter.audit();
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].verdict, ConfirmVerdict::Approved);
    assert!(audit[0].yolo, "YOLO 自动放行要留痕");
    assert!(!audit[0].confirmed, "自动放行不是「用户确认」");
}

#[test]
fn pending_confirm_forces_busy_without_yolo() {
    let mut presenter = presenter(Script::WriteOp);
    assert!(!presenter.agent_rail().yolo);
    presenter
        .agent_submit("并发调小点".to_string())
        .expect("提交应当成功");
    assert!(presenter.agent_rail().confirm.is_some());

    // 先把流式标记清干净：否则 `busy` 可能只是「streaming 顺手置真」，
    // 这条测试就钉不住「待确认算忙」这条规则（r1 P2-2：变异掉 `confirm.is_some()`
    // 之后这里必须变红）。
    presenter.state_mut().ai.finish_stream();
    let rail = presenter.agent_rail();
    assert!(!rail.streaming, "此刻没有流式消息在跑");
    assert!(rail.busy, "非 YOLO 下待确认必须算忙，UI 靠它禁用输入");
}

#[test]
fn begin_turn_marks_busy_before_any_event_arrives() {
    // 提交成功到第一个事件之间异步后端有空窗：这段时间必须已经算忙，
    // 否则用户能再点一次发送（两条 user 消息、两轮回复）。
    let mut rail = AgentRail::new();
    rail.begin_turn();
    assert!(rail.busy && rail.streaming);

    // 空事件流也不会把输入框永久锁死：收敛时按消息重算
    rail.settle_streaming();
    assert!(!rail.busy && !rail.streaming);
}

#[test]
fn submit_with_a_silent_backend_does_not_leave_the_shell_busy() {
    let mut presenter = Presenter::new(Box::new(SilentTransport));
    presenter
        .agent_submit("在吗".to_string())
        .expect("提交应当成功");
    assert!(
        !presenter.agent_rail().busy,
        "后端一个事件都没回，输入框必须重新可用"
    );
}

/// 工具收尾事件早于开始事件（或重复投递）：不许覆盖已收敛的历史条目（r1 P2-3）。
#[test]
fn tool_call_finished_without_a_pending_call_never_rewrites_history() {
    let mut presenter = presenter(Script::Qa);
    presenter.apply(AppEvent::Agent(AgentEvent::ToolCallStarted {
        name: "set_config".to_string(),
        summary: "修改 download.concurrency".to_string(),
    }));
    presenter.apply(AppEvent::Agent(AgentEvent::ToolCallFinished {
        name: "set_config".to_string(),
        ok: true,
        summary: "并发已改成 16".to_string(),
    }));
    // 重放 / 乱序：没有 pending 的收尾事件只能被忽略
    presenter.apply(AppEvent::Agent(AgentEvent::ToolCallFinished {
        name: "set_config".to_string(),
        ok: false,
        summary: "另一次失败".to_string(),
    }));

    let tool = presenter.agent_rail().messages[0]
        .tool
        .as_ref()
        .expect("工具条目");
    assert_eq!(tool.summary, "并发已改成 16", "历史条目被改写了");
    assert!(tool.ok);
}

// ────────────────── scrub_text 的 Bearer 右边界 ──────────────────

/// 钉住 `mask_bearer` 的**右边界**（r2 P2-2：停字符集收窄或换字符都必须让这条红）。
///
/// 方向说明：停字符越少 = 吞得越多（安全）；放宽才会裸奔。所以句点故意不在停字符集里
/// （吞掉句点，换「短 JWT 只掩前半段」是不划算的）。
#[test]
fn bearer_right_boundary_is_pinned() {
    // 空白 / 逗号 / 分号 / 引号是停字符：令牌之后的内容原样保留
    assert_eq!(
        scrub_text("Authorization: Bearer abc123, retry=1"),
        "Authorization: Bearer <redacted:token>, retry=1"
    );
    assert_eq!(
        scrub_text("Bearer abc123; rest"),
        "Bearer <redacted:token>; rest"
    );
    assert_eq!(
        scrub_text("\"Bearer abc123\""),
        "\"Bearer <redacted:token>\""
    );
    assert_eq!(scrub_text("Bearer abc123"), "Bearer <redacted:token>");
    // 句点不是停字符：整段吞掉（宁可多掩，也不让短 JWT 只掩前半段）
    assert_eq!(scrub_text("Bearer abc123."), "Bearer <redacted:token>");
    assert_eq!(scrub_text("Bearer eyJ.abc.def"), "Bearer <redacted:token>");
}

// ─────────────────────── 提交前置条件 ───────────────────────

#[test]
fn submit_rejects_empty_busy_and_pending_confirm() {
    let mut presenter = presenter(Script::Qa);
    assert!(presenter.agent_submit("   \n ".to_string()).is_err());
    assert!(presenter.agent_rail().messages.is_empty());

    // 上一轮还在跑
    presenter.apply(AppEvent::Agent(AgentEvent::MessageAppended(
        ChatMessage::assistant_stream(),
    )));
    assert!(presenter.agent_submit("再说一句".to_string()).is_err());

    // 有写操作待确认
    presenter.apply(AppEvent::Agent(AgentEvent::Failed {
        message: "断流".to_string(),
    }));
    presenter.apply(AppEvent::Agent(AgentEvent::ConfirmRequested {
        request: ConfirmRequest {
            id: "req-1".to_string(),
            tool: "set_config".to_string(),
            summary: "改内存".to_string(),
            details: serde_json::json!({}),
            sensitivity: Sensitivity::High,
        },
    }));
    let error = presenter.agent_submit("再说一句".to_string());
    assert!(error.is_err(), "待确认时不许再提交");
}

#[test]
fn submit_keeps_the_draft_when_the_backend_fails() {
    let mut presenter = Presenter::new(Box::new(BrokenTransport));
    presenter.state_mut().ai.draft = "还没发出去的话".to_string();
    assert!(presenter
        .agent_submit("还没发出去的话".to_string())
        .is_err());
    // 抓得住：起不来的一轮不产生事件、草稿保留（用户能改完再发）
    assert!(presenter.agent_rail().messages.is_empty());
    assert_eq!(presenter.agent_rail().draft, "还没发出去的话");
}

#[test]
fn draft_is_cleared_only_on_successful_submit() {
    let mut presenter = presenter(Script::Qa);
    presenter.state_mut().ai.draft = " 怎么装 ".to_string();
    presenter
        .agent_submit(" 怎么装 ".to_string())
        .expect("提交应当成功");
    assert!(presenter.agent_rail().draft.is_empty());
}

#[test]
fn confirm_without_pending_is_a_noop() {
    let mut presenter = presenter(Script::Qa);
    presenter.agent_confirm(true);
    presenter.agent_confirm(false);
    assert!(presenter.audit().is_empty());
    assert!(presenter.agent_rail().messages.is_empty());
}

#[test]
fn apply_forwards_every_event_to_app_state() {
    let mut presenter = presenter(Script::Qa);
    presenter.apply(AppEvent::Agent(AgentEvent::MessageAppended(
        ChatMessage::assistant_stream(),
    )));
    assert_eq!(presenter.state().ai.messages.len(), 1);
    assert_eq!(presenter.agent_rail(), &presenter.state().ai);
}

// ───────────────────────── 测试替身 ─────────────────────────

/// 起不来的后端（返回 `Err`，不产生任何事件）。
struct BrokenTransport;

impl AgentTransport for BrokenTransport {
    fn submit(&mut self, _text: &str, _history: &AgentRail) -> Result<Vec<AppEvent>, String> {
        Err("本地模型还没加载".to_string())
    }

    fn confirm(&mut self, _request_id: &str, _accept: bool) -> Vec<AppEvent> {
        Vec::new()
    }
}

/// 收下输入但一个事件都不回的后端（异步推流的极端形态：这一拍还没到）。
struct SilentTransport;

impl AgentTransport for SilentTransport {
    fn submit(&mut self, _text: &str, _history: &AgentRail) -> Result<Vec<AppEvent>, String> {
        Ok(Vec::new())
    }

    fn confirm(&mut self, _request_id: &str, _accept: bool) -> Vec<AppEvent> {
        Vec::new()
    }
}
