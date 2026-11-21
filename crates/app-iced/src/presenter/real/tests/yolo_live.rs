//! YOLO 的**运行期语义**（M389）：开关是活的、审计跟着换、显式拒绝不被改写。
//!
//! 从 `e2e.rs` 拆出来的唯一理由是 400 行红线（`guards.yml` 的 file-size 门禁）。
//! 重量级与 `e2e.rs` 相同：真端口、真十工具、真 JSONL 审计，provider 是回放件（不联网）。

use std::sync::atomic::Ordering;

use serde_json::{json, Value};
use unlucky_agent::brain::chat::ChatReply;
use unlucky_cli::commands::agent::ports;
use unlucky_cli::context::Ctx;
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;
use unlucky_presenter::agent::AgentRail;
use unlucky_presenter::dto::AgentEvent as DtoEvent;

use super::*;

/// 本轮收尾的标记：映射器在一轮结束时关掉助手消息（`MessageFinished`）。
///
/// 为什么不用「最后一句助手文本非空」：那要靠把事件喂进 `AgentRail` 才能算，而**半截**的
/// 增量事件喂进一条新一轮的 rail 会让 `MessageDelta` 指到不存在的下标（`rail.rs` 的 debug
/// 断言）。所以这里只等「服务端说这一轮结束了」，再把事件按顺序灌进同一个 rail。
fn turn_closed(events: &[AppEvent]) -> bool {
    dto_events(events)
        .iter()
        .any(|event| matches!(event, DtoEvent::MessageFinished { .. }))
}

/// 把这一批事件按顺序喂进**同一条** rail（`submit` 的 `base` 靠它的长度算）。
fn feed(rail: &mut AgentRail, events: &[AppEvent]) {
    for event in dto_events(events) {
        rail.ingest(event);
    }
}

#[test]
fn yolo_skips_the_question_but_still_writes_the_audit_line() {
    let (mut transport, home, data, _calls) = harness(
        "yolo",
        vec![
            tool_call(
                "set_config",
                json!({ "path": "instances.mode", "value": "shared" }),
            ),
            ChatReply::text("改好了。"),
        ],
        json!({ "yolo": true }),
    );

    transport
        .submit("实例默认用 shared 模式", &AgentRail::new())
        .expect("提交应成功");
    let events = pump(&transport, |events| {
        has_tool_finished(events, "set_config", true)
    });
    assert!(
        !dto_events(&events)
            .iter()
            .any(|event| matches!(event, DtoEvent::ConfirmRequested { .. })),
        "YOLO 下不问人：{events:#?}"
    );
    let _ = data;
    let record = audit_lines(&home)
        .into_iter()
        .find(|line| line["tool"] == "set_config" && line["outcome"] == "ok")
        .expect("YOLO 也要留审计");
    assert_eq!(record["yolo"], true, "审计必须标出这是自动放行");
    assert_eq!(record["confirmed"], false, "自动放行不是「用户确认」");
}

#[test]
fn turning_yolo_off_mid_session_makes_the_next_high_write_ask() {
    // M389 的**主证据**：同一个进程、同一条会话、同一条 `RealTransport`。
    // 第 1 轮 YOLO 开着（不问、审计 yolo=true）→ 设置页把 `ai.yolo` 改成 false
    // （与 `host::settings::set` 同一条 API：`SettingStore::set` + `save`，原子写）→
    // 第 2 轮同一笔高敏感写**必须问人**（审计 yolo=false, confirmed=true）。
    let (mut transport, home, data, calls) = harness(
        "yolo-live",
        vec![
            // 第 1 轮：YOLO 下不问，直接写
            tool_call(
                "set_config",
                json!({ "path": "instances.mode", "value": "shared" }),
            ),
            ChatReply::text("改成 shared 了。"),
            // 第 2 轮：文件里 yolo 已关 → 必须先问人
            tool_call(
                "set_config",
                json!({ "path": "instances.mode", "value": "isolated" }),
            ),
            ChatReply::text("改回 isolated 了。"),
        ],
        json!({ "yolo": true }),
    );

    let mut rail = AgentRail::new();
    transport
        .submit("实例默认用 shared 模式", &rail)
        .expect("提交应成功");
    let first = pump(&transport, |events| {
        has_tool_finished(events, "set_config", true) && turn_closed(events)
    });
    assert!(
        !dto_events(&first)
            .iter()
            .any(|event| matches!(event, DtoEvent::ConfirmRequested { .. })),
        "装配时 YOLO 是开的：第一笔不问人：{first:#?}"
    );
    feed(&mut rail, &first);

    // ── 用户在设置页关掉 YOLO（不重启、不重建会话）──
    let config = home.join("config").join("launcher.json");
    let mut store = SettingStore::load(&config, launcher_schema());
    store.set("ai.yolo", json!(false)).expect("写 ai.yolo");
    store.save().expect("落盘");

    // ── 同一会话的下一笔高敏感写：必须弹确认 ──
    transport
        .submit("还是改回 isolated 吧", &rail)
        .expect("提交应成功");
    let asked = pump(&transport, |events| {
        dto_events(events)
            .iter()
            .any(|event| matches!(event, DtoEvent::ConfirmRequested { .. }))
    });
    let request_id = dto_events(&asked)
        .iter()
        .find_map(|event| match event {
            DtoEvent::ConfirmRequested { request } => Some(request.id.clone()),
            _ => None,
        })
        .expect("关掉 YOLO 之后必须问人（这正是 X6 要修的那一条）");
    assert!(
        !has_tool_finished(&asked, "set_config", true),
        "没人点允许之前不许写：{asked:#?}"
    );
    feed(&mut rail, &asked);

    transport.confirm(&request_id, true);
    let second = pump(&transport, |events| {
        has_tool_finished(events, "set_config", true) && turn_closed(events)
    });
    assert!(
        has_tool_finished(&second, "set_config", true),
        "{second:#?}"
    );
    feed(&mut rail, &second);
    assert_eq!(calls.load(Ordering::Relaxed), 4, "两轮各两次补全");
    let _ = data;

    // ── 审计跟着换：第一笔 yolo=true，第二笔 yolo=false + confirmed=true ──
    let writes: Vec<Value> = audit_lines(&home)
        .into_iter()
        .filter(|line| line["tool"] == "set_config" && line["outcome"] == "ok")
        .collect();
    assert_eq!(writes.len(), 2, "两笔写都要留审计：{writes:#?}");
    assert_eq!(writes[0]["yolo"], true, "第 1 笔是自动放行");
    assert_eq!(writes[0]["confirmed"], false);
    assert_eq!(
        writes[1]["yolo"], false,
        "第 2 笔：审计里的 yolo 字段必须跟着开关变（不是装配期快照）"
    );
    assert_eq!(writes[1]["confirmed"], true, "第 2 笔是用户亲手点的允许");
}

#[test]
fn the_agent_can_flip_yolo_itself_and_the_next_write_skips_the_question() {
    // 这是 iced GUI 上**真能点到**的那条路（M389 调查结论：设置页的 AI 分组正文是自定义表单，
    // 没有 YOLO 开关；`ai.yolo` 只能由聊天里的 `set_config`、`ull config set` 或手改文件来翻）。
    // 生产里工具的 `Setting` 端口与 `Plan` 读的是**同一份** `launcher.json`（同一个 `Ctx`）——
    // 这里直接用 `Ctx` 解析出来的那份路径写文件，而不是猜 `<root>/config/`。
    let root = temp_dir("yolo-selfflip");
    let ctx = Ctx::resolve_in(
        &unlucky_cli::cli::Globals {
            data_dir: Some(root.clone()),
            ..unlucky_cli::cli::Globals::default()
        },
        None,
    )
    .expect("临时数据根必须能解析");
    let config = ctx.launcher_config();
    std::fs::create_dir_all(config.parent().expect("config 目录")).expect("建 config 目录");
    std::fs::write(
        &config,
        serde_json::to_string_pretty(&json!({ "ai": { "enabled": true, "provider": "openai_compatible", "base_url": "http://127.0.0.1:8080/v1", "model": "m" } }))
            .expect("序列化"),
    )
    .expect("写 launcher.json");
    let plan = Plan::at(&config).expect("配置完整");
    let audit_root = plan.data_root.clone();
    let (factory, _calls) = scripted(vec![
        // 第 1 轮：AI 自己打开 YOLO（`ai.yolo` 是 High 敏感键 ⇒ 先问人）
        tool_call("set_config", json!({ "path": "ai.yolo", "value": true })),
        ChatReply::text("已打开 YOLO。"),
        // 第 2 轮：同一笔高敏感写，YOLO 现在开着 ⇒ 不再问人
        tool_call(
            "set_config",
            json!({ "path": "instances.mode", "value": "shared" }),
        ),
        ChatReply::text("改好了。"),
    ]);
    let mut transport = RealTransport::spawn(plan, ports::build(&ctx), factory);

    let mut rail = AgentRail::new();
    transport.submit("把 YOLO 打开", &rail).expect("提交应成功");
    let asked = pump(&transport, |events| {
        dto_events(events)
            .iter()
            .any(|event| matches!(event, DtoEvent::ConfirmRequested { .. }))
    });
    let request_id = dto_events(&asked)
        .iter()
        .find_map(|event| match event {
            DtoEvent::ConfirmRequested { request } => Some(request.id.clone()),
            _ => None,
        })
        .expect("`ai.yolo` 是 High 敏感键：打开它本身也要问人");
    feed(&mut rail, &asked);
    transport.confirm(&request_id, true);
    let first = pump(&transport, |events| {
        has_tool_finished(events, "set_config", true) && turn_closed(events)
    });
    feed(&mut rail, &first);

    // ── 第 2 轮：同一条会话，YOLO 已在第 1 轮里被写进设置文件 ──
    transport
        .submit("实例默认改成 shared", &rail)
        .expect("提交应成功");
    let second = pump(&transport, |events| {
        has_tool_finished(events, "set_config", true) && turn_closed(events)
    });
    assert!(
        !dto_events(&second)
            .iter()
            .any(|event| matches!(event, DtoEvent::ConfirmRequested { .. })),
        "YOLO 打开之后下一笔高敏感写不再问人（同一条会话）：{second:#?}"
    );
    assert!(
        has_tool_finished(&second, "set_config", true),
        "{second:#?}"
    );
    feed(&mut rail, &second);

    let writes: Vec<Value> = audit_lines(&audit_root)
        .into_iter()
        .filter(|line| line["tool"] == "set_config" && line["outcome"] == "ok")
        .collect();
    assert_eq!(writes.len(), 2, "两笔写都要留审计：{writes:#?}");
    assert_eq!(
        writes[0]["yolo"], false,
        "第 1 笔（打开 YOLO 自己）是按用户点的允许写的"
    );
    assert_eq!(
        writes[1]["yolo"], true,
        "第 2 笔是自动放行：审计必须跟着开关走"
    );
    assert_eq!(writes[1]["confirmed"], false, "自动放行不是「用户确认」");
}
