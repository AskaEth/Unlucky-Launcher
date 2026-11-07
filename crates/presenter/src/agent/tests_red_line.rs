//! 红线测试：凭据形状的串不得进 [`crate::agent::AgentRail`]，也不得**出站**。
//!
//! 与 `agent/tests.rs` 同源、拆文件只为守「单文件 ≤400 行」。这一组测试的目标是**守住闸门**：
//! 两道闸门（出站 [`Presenter::agent_submit`] 的 prompt/Err、显示 [`crate::agent::rail`] 的
//! ingest 投影）与每条具体入口各自注入一次，任何一条把脱敏摘掉都必须变红。
//! r1 / r2 复审都是靠「逐入口造变异」抓出假绿的，改动这里时请照做自证。
//!
//! 注入的凭据形状两类：
//! - JWT（`header.payload.signature`，靠 core `Redactor` 的「带点 token」规则拦）；
//! - 非 JWT 形状（`{"password":"hunter2"}`、短 `Bearer abcdef123456`，靠键名与 Bearer 规则拦）。
//!
//! **`rail.draft` 不在这组断言的覆盖范围**：草稿是「还没发出去的字」，UI 要原样显示，
//! 它按设计可以躺着凭据原文；安全性由「`agent_submit` 脱敏的是入参而非草稿」保证。
//! 所以本文件的 `rail_text()` 刻意不含 draft——别把「rail 全文无 JWT」读成覆盖了 draft。

use std::cell::RefCell;
use std::rc::Rc;

use super::mock::{MockAgentTransport, Script};
use super::{AgentTransport, Presenter};
use crate::agent::rail::scrub_text;
use crate::agent::AgentRail;
use crate::dto::{
    AgentEvent, AppEvent, ChatMessage, ChatRole, ConfirmRequest, Sensitivity, ToolCallView,
};

/// 真实形状的 JWT（header.payload.signature，三段各 ≥8）。
const JWT: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.c2lnbmF0dXJlLXZhbHVlLXRoYXQtaXMtbG9uZy1lbm91Z2g";

fn presenter(script: Script) -> Presenter {
    Presenter::new(Box::new(MockAgentTransport::new(script)))
}

#[test]
fn jwt_shaped_text_never_reaches_the_rail() {
    let mut presenter = presenter(Script::Qa);
    // 五个入口各注入一次：用户提交、增量、工具摘要、错误、确认请求细节
    presenter
        .agent_submit(format!("我的 token 是 {JWT}"))
        .expect("提交应当成功");
    presenter.apply(AppEvent::Agent(AgentEvent::MessageDelta {
        index: 1,
        delta: format!("顺手贴一个 {JWT}"),
    }));
    presenter.apply(AppEvent::Agent(AgentEvent::ToolCallStarted {
        name: "read_game_log".to_string(),
        summary: format!("Authorization: Bearer {JWT}"),
    }));
    presenter.apply(AppEvent::Agent(AgentEvent::Failed {
        message: format!("401: access_token={JWT}"),
    }));
    presenter.apply(AppEvent::Agent(AgentEvent::ConfirmRequested {
        request: ConfirmRequest {
            id: "req-jwt".to_string(),
            tool: "set_config".to_string(),
            summary: format!("把 access_token 改成 {JWT}"),
            details: serde_json::json!({ "access_token": JWT, "note": JWT }),
            sensitivity: Sensitivity::High,
        },
    }));

    let rail = presenter.agent_rail();
    let text = rail_text(rail);
    assert!(
        !text.contains("eyJhbGci"),
        "rail 上不得出现 JWT 原文：{text}"
    );
    assert!(
        text.contains("<redacted:token>"),
        "应当被脱敏成占位符：{text}"
    );
    // 凭据键名的值整段打码（不只是靠内容规则）
    let details = &rail.confirm.as_ref().expect("确认请求仍在").details;
    assert_eq!(details["access_token"], "<redacted:token>");
    assert!(!details["note"]
        .as_str()
        .unwrap_or_default()
        .contains("eyJhbGci"));
}

/// 上一条测试里 `MessageAppended` 走的是 `agent_submit`（已先 scrub），踩不到 rail 自己的
/// `scrub_message`。这条**绕过 submit** 直接投递，钉住那条入口（r1 P1 变异 A）。
#[test]
fn message_appended_from_the_backend_is_scrubbed_inside_the_rail() {
    let mut presenter = presenter(Script::Qa);
    presenter.apply(AppEvent::Agent(AgentEvent::MessageAppended(
        ChatMessage::user(format!("access_token={JWT}")),
    )));
    presenter.apply(AppEvent::Agent(AgentEvent::MessageAppended(ChatMessage {
        role: ChatRole::Assistant,
        text: format!("听到你贴了 {JWT}"),
        tool: None,
        streaming: false,
    })));
    presenter.apply(AppEvent::Agent(AgentEvent::MessageAppended(
        ChatMessage::system(format!("refresh_token={JWT}")),
    )));

    let text = rail_text(presenter.agent_rail());
    assert!(
        !text.contains("eyJhbGci"),
        "MessageAppended 入口未脱敏：{text}"
    );
    assert_eq!(
        presenter.agent_rail().messages.len(),
        3,
        "三条消息都要在（脱敏不等于丢消息）"
    );
}

/// 工具**收尾**事件（`ToolCallFinished`）同样带 name + summary，必须过闸门（r1 P1 变异 C2）。
///
/// 两边用同一个「名字里带 JWT」的工具名：脱敏后仍然相等，所以**只要有一侧没脱敏**，
/// 名字就对不上（条目收敛不了、`ok` 仍是 false），断言会红。
#[test]
fn tool_call_finished_summary_is_scrubbed_inside_the_rail() {
    let mut presenter = presenter(Script::Qa);
    let name = format!("set_config_{JWT}");
    presenter.apply(AppEvent::Agent(AgentEvent::ToolCallStarted {
        name: name.clone(),
        summary: "修改 download.concurrency".to_string(),
    }));
    presenter.apply(AppEvent::Agent(AgentEvent::ToolCallFinished {
        name,
        ok: true,
        summary: format!("写进去了：Authorization: Bearer {JWT}"),
    }));

    let text = rail_text(presenter.agent_rail());
    assert!(
        !text.contains("eyJhbGci"),
        "ToolCallFinished 入口未脱敏：{text}"
    );
    let tool = presenter.agent_rail().messages[0]
        .tool
        .as_ref()
        .expect("工具条目");
    assert!(tool.ok, "结论位要照实写");
    assert!(!tool.name.contains("eyJhbGci"), "工具名也要过闸门");
}

/// 非 JWT 形状的凭据（`{"password":"hunter2"}`）只能靠**键名**规则拦（r1 P1 变异 H）。
///
/// 同时钉住「值随 path 敏感」这条上下文规则（`set_config` 的 details 形状）。
#[test]
fn sensitive_keys_in_confirm_details_are_masked_without_jwt_shape() {
    let mut presenter = presenter(Script::Qa);
    presenter.apply(AppEvent::Agent(AgentEvent::ConfirmRequested {
        request: ConfirmRequest {
            id: "req-keys".to_string(),
            tool: "set_config".to_string(),
            summary: "改几个设置".to_string(),
            details: serde_json::json!({
                "password": "hunter2",
                "api_key": "short-key-123",
                "nested": { "refresh_token": "rt-1" },
                "list": [{ "token": "xyz" }],
                "plain": "nothing here",
                "path": "ai.api_key",
                "old": "",
                "new": "sk-proj-abcdefghijklmnop",
            }),
            sensitivity: Sensitivity::High,
        },
    }));

    let details = &presenter
        .agent_rail()
        .confirm
        .as_ref()
        .expect("确认请求")
        .details;
    let placeholder = serde_json::json!("<redacted:token>");
    // 键名命中：对象 / 嵌套 / 数组各一
    assert_eq!(details["password"], placeholder);
    assert_eq!(details["api_key"], placeholder);
    assert_eq!(details["nested"]["refresh_token"], placeholder);
    assert_eq!(details["list"][0]["token"], placeholder);
    // 值随 path 敏感：old/new 一并掩码（不然 API key 就躺在确认弹窗的 diff 里）
    assert_eq!(details["new"], placeholder);
    assert_eq!(details["old"], placeholder);
    // 无关键原样保留（审计要看得出改的是什么）
    assert_eq!(details["plain"], serde_json::json!("nothing here"));
    assert_eq!(details["path"], serde_json::json!("ai.api_key"));

    let text = rail_text(presenter.agent_rail());
    for leak in [
        "hunter2",
        "short-key-123",
        "rt-1",
        "xyz",
        "sk-proj-abcdefghijklmnop",
    ] {
        assert!(!text.contains(leak), "凭据泄漏到 rail：{leak} / {text}");
    }
}

/// 短 `Bearer` 令牌不是 JWT 形状，core 的 `Redactor` 会放过它（r1 P2-1）。
#[test]
fn short_bearer_token_is_masked() {
    let mut presenter = presenter(Script::Qa);
    presenter.apply(AppEvent::Agent(AgentEvent::ToolCallStarted {
        name: "read_game_log".to_string(),
        summary: "Authorization: Bearer abcdef123456".to_string(),
    }));
    let text = rail_text(presenter.agent_rail());
    assert!(
        !text.contains("abcdef123456"),
        "短 Bearer 令牌要掩码：{text}"
    );
    assert!(text.contains("Bearer <redacted:token>"), "{text}");
}

#[test]
fn scrub_text_is_idempotent_on_already_redacted_text() {
    let once = scrub_text(JWT);
    assert_eq!(scrub_text(&once), once);
    let bearer = scrub_text("Authorization: Bearer abcdef123456");
    assert_eq!(scrub_text(&bearer), bearer);
}

// ───────────────── 出站闸门：发给后端的那份 payload ─────────────────

/// 记录 `submit` 实参的后端替身：红线里最重要的是「**真正发出去的是什么**」，
/// 而不是 rail 上显示成什么（显示层自己还会再脱一次，会把出站的漏洞盖住）。
#[derive(Clone, Default)]
struct RecordingTransport {
    seen: Rc<RefCell<Vec<String>>>,
}

impl AgentTransport for RecordingTransport {
    fn submit(&mut self, text: &str, _history: &AgentRail) -> Result<Vec<AppEvent>, String> {
        self.seen.borrow_mut().push(text.to_string());
        // 照常回一条用户气泡，让整轮流程跟真实后端一致（rail 会再脱一次，所以
        // 只断言 rail 干净是抓不住出站泄漏的）
        Ok(vec![AppEvent::Agent(AgentEvent::MessageAppended(
            ChatMessage::user(text),
        ))])
    }

    fn confirm(&mut self, _request_id: &str, _accept: bool) -> Vec<AppEvent> {
        Vec::new()
    }
}

/// 一律报错的后端替身（用来守 Err 文案那道 scrub）。
struct ErrorTransport(String);

impl AgentTransport for ErrorTransport {
    fn submit(&mut self, _text: &str, _history: &AgentRail) -> Result<Vec<AppEvent>, String> {
        Err(self.0.clone())
    }

    fn confirm(&mut self, _request_id: &str, _accept: bool) -> Vec<AppEvent> {
        Vec::new()
    }
}

/// 出站守卫（r2 P1）：`agent_submit` 送去后端的那句 prompt 必须已经脱敏。
///
/// 关键在**看后端收到什么**：rail 侧也会脱敏，所以「界面上没有 JWT」证明不了出站安全。
#[test]
fn outbound_prompt_is_scrubbed_before_it_reaches_the_backend() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let mut presenter = Presenter::new(Box::new(RecordingTransport {
        seen: Rc::clone(&seen),
    }));
    presenter
        .agent_submit(format!("帮我看看这个 token：{JWT}"))
        .expect("提交应当成功");

    let seen = seen.borrow();
    assert_eq!(seen.len(), 1, "一轮输入只该发一次");
    assert!(
        !seen[0].contains("eyJhbGci"),
        "出站 prompt 里还有 JWT 原文：{}",
        seen[0]
    );
    assert!(
        seen[0].contains("<redacted:token>"),
        "出站 prompt 应当带占位符：{}",
        seen[0]
    );
}

/// 失败原因（provider 的 401 常把 key 抄进 message）同样是 UI 可见文本（r2 P2-1 / N）。
#[test]
fn outbound_error_text_is_scrubbed() {
    let mut presenter = Presenter::new(Box::new(ErrorTransport(format!(
        "401 Unauthorized: key={JWT}"
    ))));
    let error = presenter
        .agent_submit("随便问一句".to_string())
        .expect_err("后端报错时应当返回 Err");
    assert!(!error.contains("eyJhbGci"), "Err 文案里还有 JWT：{error}");
    assert!(error.contains("<redacted:token>"), "{error}");
}

// ─────────── 显示闸门：另三处 scrub 站点（r2 P2-1 / K、M）───────────

/// `ConfirmRequest.tool` 也过闸门（工具名来自注册表，凭据落名字不现实，但「删了没人发现」）。
#[test]
fn confirm_tool_name_is_scrubbed() {
    let mut presenter = presenter(Script::Qa);
    presenter.apply(AppEvent::Agent(AgentEvent::ConfirmRequested {
        request: ConfirmRequest {
            id: "req-tool".to_string(),
            tool: format!("set_config_{JWT}"),
            summary: "改设置".to_string(),
            details: serde_json::json!({}),
            sensitivity: Sensitivity::High,
        },
    }));

    let tool = &presenter
        .agent_rail()
        .confirm
        .as_ref()
        .expect("确认请求")
        .tool;
    assert!(!tool.contains("eyJhbGci"), "confirm.tool 未脱敏：{tool}");
    assert!(!rail_text(presenter.agent_rail()).contains("eyJhbGci"));
}

/// `MessageAppended` 里带 `ToolCallView` 的那条分支（role = Tool）也要脱敏——
/// mock 只会发 user，只有真实后端会走到，所以必须单独钉住。
#[test]
fn tool_message_from_the_backend_has_its_tool_view_scrubbed() {
    let mut presenter = presenter(Script::Qa);
    presenter.apply(AppEvent::Agent(AgentEvent::MessageAppended(ChatMessage {
        role: ChatRole::Tool,
        text: format!("工具摘要：{JWT}"),
        tool: Some(ToolCallView {
            name: format!("read_game_log_{JWT}"),
            ok: true,
            summary: format!("Authorization: Bearer {JWT}"),
        }),
        streaming: false,
    })));

    let tool = presenter.agent_rail().messages[0]
        .tool
        .as_ref()
        .expect("工具条目");
    assert!(
        !tool.name.contains("eyJhbGci"),
        "tool.name 未脱敏：{}",
        tool.name
    );
    assert!(
        !tool.summary.contains("eyJhbGci"),
        "tool.summary 未脱敏：{}",
        tool.summary
    );
    assert!(!rail_text(presenter.agent_rail()).contains("eyJhbGci"));
}

// ───────────────────────── 断言辅助 ─────────────────────────

/// 把 rail 上所有对用户可见的文本拼起来（红线断言用）。
///
/// **不含 `draft`**（见模块文档：草稿按设计可以存原文）。
fn rail_text(rail: &AgentRail) -> String {
    let mut out = String::new();
    for message in &rail.messages {
        out.push_str(&message.text);
        if let Some(tool) = &message.tool {
            out.push_str(&tool.name);
            out.push_str(&tool.summary);
        }
    }
    out.push_str(rail.error.as_deref().unwrap_or_default());
    out.push_str(&rail.endpoint_label);
    if let Some(confirm) = &rail.confirm {
        out.push_str(&confirm.tool);
        out.push_str(&confirm.summary);
        out.push_str(&confirm.details.to_string());
    }
    out
}
