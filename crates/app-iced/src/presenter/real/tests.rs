//! `presenter::real` 的用例（M384）：事件映射 / 配置判定 / 确认闸门（端到端在 `tests/e2e.rs`）。
//!
//! 三段证据，重量级递增（端到端那一段见子模块）：
//!
//! | 段 | 跑的是什么 | 需要网络吗 |
//! |---|---|---|
//! | 事件映射 / 闸门 | 纯函数与 future | 不要 |
//! | 配置判定 | `Plan::at(<临时 launcher.json>)` | 不要 |
//! | 端到端（`e2e.rs`） | `RealTransport::spawn` + 临时数据根 + 回放 provider | 不要 |
//!
//! 端到端那几条**不碰真模型**：provider 是回放件，但下游（`build_v1_registry` 的十个工具、
//! `CoreSessions` / `CoreSetting` 端口、`JsonlAudit`、确认闸门、事件映射）全是生产实现。
//! 这正是「没有真 key 也能闭环」的那部分证据（报告「没验的部分」会写明到模型那段没走）。

mod e2e;
mod fixtures;
mod mapper;
mod yolo_live;

use std::sync::{Arc, Mutex};

use serde_json::json;

use unlucky_agent::confirm::ConfirmRequest as BrainConfirm;
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;
use unlucky_presenter::dto::AgentEvent as DtoEvent;

use super::mapper::Scrubber;
use super::*;
use fixtures::*;

// ───────────────────── 二、配置判定（不联网） ─────────────────────

#[test]
fn a_local_endpoint_needs_no_key_but_a_missing_model_is_named() {
    let home = temp_dir("plan");
    let config = write_launcher(
        &home,
        json!({ "ai": { "enabled": true, "provider": "openai_compatible", "base_url": "http://127.0.0.1:8080/v1", "model": "qwen2.5" } }),
    );
    let plan = Plan::at(&config).expect("本地端点不需要密钥");
    assert_eq!(plan.config.model, "qwen2.5");
    assert_eq!(plan.data_root, home, "<home>/config/launcher.json → <home>");
    assert!(!plan.yolo_now(), "没写 ai.yolo 就按关处理");

    let missing_model = write_launcher(
        &home,
        json!({ "ai": { "enabled": true, "provider": "openai_compatible", "base_url": "http://127.0.0.1:8080/v1", "model": "" } }),
    );
    assert_eq!(Plan::at(&missing_model).err(), Some(Missing::Model));
}

#[test]
fn disabled_and_anthropic_without_a_key_are_both_named() {
    let home = temp_dir("plan-missing");
    let disabled = write_launcher(
        &home,
        json!({ "ai": { "enabled": false, "provider": "openai_compatible", "base_url": "http://127.0.0.1:8080/v1", "model": "m" } }),
    );
    assert_eq!(Plan::at(&disabled).err(), Some(Missing::Disabled));

    let anthropic = write_launcher(
        &home,
        json!({ "ai": { "enabled": true, "provider": "anthropic", "model": "claude-sonnet-4" } }),
    );
    assert_eq!(
        Plan::at(&anthropic).err(),
        Some(Missing::ApiKey),
        "Anthropic 必须有密钥（与设置页「测试连接」同一条判据）"
    );
}

#[test]
fn yolo_is_a_live_switch_not_a_spawn_time_snapshot() {
    // M389 的验收：**同一个 `Plan` / 同一个开关**，文件一改，判定就跟着改。
    let home = temp_dir("plan-yolo");
    let config = write_launcher(
        &home,
        json!({ "ai": { "enabled": true, "provider": "openai_compatible", "base_url": "http://127.0.0.1:8080/v1", "model": "m", "yolo": true } }),
    );
    let plan = Plan::at(&config).expect("配置完整");
    assert!(plan.yolo_now(), "文件里写着 true");
    let switch = plan.session_config().yolo;
    assert!(
        !switch.requires_confirmation(unlucky_agent::confirm::Sensitivity::High),
        "YOLO 开着：高敏感写不问"
    );

    // 运行期关掉（设置页那条 `host::settings::set` 走的是同一条 `SettingStore::save`）
    let mut store = SettingStore::load(&config, launcher_schema());
    store.set("ai.yolo", json!(false)).expect("写 ai.yolo");
    store.save().expect("落盘");

    assert!(!plan.yolo_now(), "同一个 Plan 现读就是关的");
    assert!(
        switch.requires_confirmation(unlucky_agent::confirm::Sensitivity::High),
        "**同一个**开关（不是新读一份 Plan）在文件改动之后必须改判"
    );
}

#[test]
fn a_broken_settings_file_reads_as_yolo_off() {
    // fail-closed：装配之后文件被外部改坏 ⇒ 探针按「关」处理（会问人），
    // 绝不拿装配时那个 `true` 兜底。
    let home = temp_dir("plan-yolo-broken");
    let config = write_launcher(
        &home,
        json!({ "ai": { "enabled": true, "provider": "openai_compatible", "base_url": "http://127.0.0.1:8080/v1", "model": "m", "yolo": true } }),
    );
    let plan = Plan::at(&config).expect("配置完整");
    assert!(plan.yolo_now());
    std::fs::write(&config, b"{ this is not json").expect("把文件改坏");
    assert!(
        !plan.yolo_now(),
        "坏文件必须读成关（会弹确认），不许按装配期那份 true 放行"
    );
}

#[test]
fn the_plan_summarises_itself_without_leaking_the_key() {
    let home = temp_dir("plan-debug");
    let config = write_launcher(
        &home,
        json!({ "ai": { "enabled": true, "provider": "openai_compatible", "base_url": "https://api.example.com/v1", "model": "m", "api_key": "sk-live-abcdef123456" } }),
    );
    let plan = Plan::at(&config).expect("配置完整");
    let shown = format!("{:?}", plan.config);
    assert!(!shown.contains("sk-live-abcdef123456"), "{shown}");
    // 脱敏器也认得它：provider 把 key 回显在错误体里时要被掩掉
    let scrub = plan.scrubber();
    assert_eq!(scrub.apply("bad key sk-live-abcdef123456"), "bad key ***");
}

// ───────────────────── 三、确认闸门（纯 std future） ─────────────────────

#[test]
fn the_gate_parks_the_tool_call_until_the_ui_answers() {
    let queue = EventQueue::default();
    let decision: DecisionSlot = Arc::new(Mutex::new(DecisionState::default()));
    let gate = ConfirmBridge {
        queue: queue.clone(),
        decision: Arc::clone(&decision),
        scrub: Scrubber::new(str::to_string),
    };
    let mut future = gate.request(BrainConfirm {
        id: "confirm-1".into(),
        tool: "set_config".into(),
        summary: "修改 instances.mode".into(),
        details: json!({ "path": "instances.mode", "new": "shared" }),
        sensitivity: unlucky_agent::confirm::Sensitivity::High,
    });

    // 请求已经进队列（UI 靠这一条画确认条）
    let events = drained(&queue);
    assert!(
        matches!(&events[0], DtoEvent::ConfirmRequested { request } if request.id == "confirm-1"),
        "{events:?}"
    );

    let waker = std::task::Waker::noop();
    let mut context = std::task::Context::from_waker(waker);
    assert!(
        future.as_mut().poll(&mut context).is_pending(),
        "没人裁决时工具调用必须挂住（不能静默放行）"
    );

    assert!(decision.lock().expect("decision").decide("confirm-1", true));
    assert!(matches!(
        future.as_mut().poll(&mut context),
        std::task::Poll::Ready(unlucky_agent::confirm::ConfirmDecision::Approved)
    ));
}

#[test]
fn a_mismatched_request_id_is_ignored() {
    let mut state = DecisionState {
        id: Some("confirm-2".into()),
        ..DecisionState::default()
    };
    assert!(
        !state.decide("confirm-1", true),
        "id 对不上就不该算数（否则一次点击会放行另一条写操作）"
    );
    assert!(state.value.is_none());
    assert!(state.decide("confirm-2", false));
    assert_eq!(state.value, Some(false));
}
