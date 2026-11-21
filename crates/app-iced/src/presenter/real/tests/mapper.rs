//! 事件映射的用例（`Mapper`：agent 事件 → presenter 事件 + 下标递推 + 空卡回收）。
//!
//! 从 `real/tests.rs` 拆出来的唯一理由是 400 行红线（`guards.yml` 的 file-size 门禁）。

use serde_json::json;
use unlucky_agent::event::AgentEvent as BrainEvent;
use unlucky_presenter::agent::AgentRail;
use unlucky_presenter::dto::AgentEvent as DtoEvent;

use unlucky_cli::commands::agent::probe;

use super::super::mapper::Scrubber;
use super::*;

// ───────────────────── 一、事件映射（纯函数） ─────────────────────

fn mapper() -> (Mapper, EventQueue) {
    let queue = EventQueue::default();
    let scrub = Scrubber::new(|text: &str| text.to_string());
    (Mapper::new(queue.clone(), scrub), queue)
}

#[test]
fn a_streamed_turn_becomes_user_bubble_plus_one_assistant_bubble() {
    let (mut mapper, queue) = mapper();
    mapper.begin(0);
    mapper.on_event(&BrainEvent::TurnStarted {
        session_id: "s".into(),
        user_text: "装 Fabric 要注意什么".into(),
    });
    mapper.on_event(&BrainEvent::TextDelta {
        text: "先确认版本".into(),
    });
    mapper.on_event(&BrainEvent::TextDelta {
        text: "，再装。".into(),
    });
    // provider 两边都发：增量已经上过屏，全文不该再说一遍
    mapper.on_event(&BrainEvent::AssistantText {
        text: "先确认版本，再装。".into(),
    });
    mapper.on_event(&BrainEvent::TurnFinished {
        steps: 1,
        text: "先确认版本，再装。".into(),
    });

    let events = drained(&queue);
    assert!(
        matches!(&events[0], DtoEvent::MessageAppended(message) if message.role == unlucky_presenter::dto::ChatRole::User),
        "{events:?}"
    );
    assert!(
        matches!(&events[1], DtoEvent::MessageAppended(message) if message.streaming),
        "第二条是流式助手气泡：{events:?}"
    );
    // 两条增量都落在下标 1 上，没有第三条「全文」增量
    let deltas: Vec<(usize, &str)> = events
        .iter()
        .filter_map(|event| match event {
            DtoEvent::MessageDelta { index, delta } => Some((*index, delta.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(
        deltas,
        vec![(1, "先确认版本"), (1, "，再装。")],
        "增量要拼在同一条气泡上，且 AssistantText 不重复"
    );
    assert!(matches!(
        events.last(),
        Some(DtoEvent::MessageFinished { index: 1 })
    ));
}

/// 把事件喂进真的 rail 投影（与 UI 同一条路径），返回视图模型。
fn rail_of(events: &[DtoEvent]) -> AgentRail {
    let mut rail = AgentRail::new();
    for event in events {
        rail.ingest(event);
    }
    rail
}

/// 会话里有没有**空白助手卡**（`astream` 开了却一个字都没有就关掉的那张）。
fn blank_assistant_cards(rail: &AgentRail) -> usize {
    rail.messages
        .iter()
        .filter(|message| {
            message.role == unlucky_presenter::dto::ChatRole::Assistant
                && message.text.trim().is_empty()
        })
        .count()
}

/// M384 r1 复审 P2-2：**工具先行**的一轮不该留下空白助手卡。
///
/// 模型这一步一个字都没说，直接给工具调用 ⇒ 改前会在用户气泡与工具条目之间压一张空的
/// 「助手」卡（`DevDocs/artifacts/agentbrain/03-tools.png` 改前那张）。
#[test]
fn a_tool_only_step_leaves_no_blank_assistant_card() {
    let (mut mapper, queue) = mapper();
    mapper.begin(0);
    mapper.on_event(&BrainEvent::TurnStarted {
        session_id: "s".into(),
        user_text: "看看上次为什么崩".into(),
    });
    // 这一步只有工具调用，`AssistantText` 是空的（rig 对纯工具回合就是这个形状）
    mapper.on_event(&BrainEvent::AssistantText {
        text: String::new(),
    });
    mapper.on_event(&BrainEvent::ToolCallStarted {
        tool: "read_game_log".into(),
        args: json!({ "session_id": "s1" }),
        step: 1,
    });
    mapper.on_event(&BrainEvent::ToolCallFinished {
        tool: "read_game_log".into(),
        ok: true,
        summary: "读到 3 条".into(),
        requires_confirmation: false,
    });
    // 拿到工具结果之后模型才说话
    mapper.on_event(&BrainEvent::TextDelta {
        text: "崩在 Fabric loader 缺类。".into(),
    });
    mapper.on_event(&BrainEvent::TurnFinished {
        steps: 2,
        text: "崩在 Fabric loader 缺类。".into(),
    });

    let events = drained(&queue);
    let rail = rail_of(&events);
    assert_eq!(
        blank_assistant_cards(&rail),
        0,
        "工具先行的一轮不许留空白助手卡：{:#?}",
        rail.messages
    );
    // 结构仍然是 user → tool → assistant（顺序与改前一致，只有空卡消失）
    assert_eq!(rail.messages.len(), 3, "{:#?}", rail.messages);
    assert_eq!(
        rail.messages[0].role,
        unlucky_presenter::dto::ChatRole::User
    );
    assert!(rail.messages[1].tool.is_some(), "第二条是工具条目");
    assert_eq!(
        rail.messages[1].text, "读到 3 条",
        "工具条目要收敛成结果摘要"
    );
    assert_eq!(rail.messages[2].text, "崩在 Fabric loader 缺类。");
    assert!(!rail.busy, "一轮结束不许把输入框锁住");
}

/// 一个字都没说的回合（模型空回复 / 撞上步数上限的最后一轮）：只留用户气泡。
#[test]
fn an_empty_turn_leaves_only_the_user_bubble() {
    let (mut mapper, queue) = mapper();
    mapper.begin(0);
    mapper.on_event(&BrainEvent::TurnStarted {
        session_id: "s".into(),
        user_text: "在吗".into(),
    });
    mapper.on_event(&BrainEvent::TurnFinished {
        steps: 1,
        text: String::new(),
    });

    let rail = rail_of(&drained(&queue));
    assert_eq!(blank_assistant_cards(&rail), 0, "{:#?}", rail.messages);
    assert_eq!(rail.messages.len(), 1, "只有用户气泡：{:#?}", rail.messages);
    assert!(!rail.busy);
}

/// 报错的回合（provider 一个字都没吐）：用户气泡 + 系统提示，没有空白助手卡。
#[test]
fn a_failed_turn_leaves_no_blank_assistant_card() {
    let (mut mapper, queue) = mapper();
    mapper.begin(0);
    mapper.on_event(&BrainEvent::TurnStarted {
        session_id: "s".into(),
        user_text: "在吗".into(),
    });
    mapper.on_event(&BrainEvent::Error {
        message: "401 Unauthorized".into(),
    });

    let rail = rail_of(&drained(&queue));
    assert_eq!(blank_assistant_cards(&rail), 0, "{:#?}", rail.messages);
    assert!(rail.error.is_some(), "错误要立横幅");
    assert!(!rail.busy, "失败后 busy 必须收敛");
}

#[test]
fn a_tool_round_opens_a_second_bubble_at_the_next_free_index() {
    let (mut mapper, queue) = mapper();
    mapper.begin(4); // rail 上已经有 4 条（模拟历史）
    mapper.on_event(&BrainEvent::TurnStarted {
        session_id: "s".into(),
        user_text: "看看日志".into(),
    });
    mapper.on_event(&BrainEvent::AssistantText {
        text: "我先读一下日志。".into(),
    });
    mapper.on_event(&BrainEvent::ToolCallStarted {
        tool: "read_game_log".into(),
        args: json!({ "session_id": "20260101T000000-abcdef" }),
        step: 1,
    });
    mapper.on_event(&BrainEvent::ToolCallFinished {
        tool: "read_game_log".into(),
        ok: true,
        summary: "读到 3 条".into(),
        requires_confirmation: false,
    });
    mapper.on_event(&BrainEvent::TextDelta {
        text: "日志里没崩。".into(),
    });
    mapper.on_event(&BrainEvent::TurnFinished {
        steps: 2,
        text: "日志里没崩。".into(),
    });

    let events = drained(&queue);
    // 用户 4、助手 5、工具条目 6、第二个助手气泡 7
    assert!(
        matches!(&events[1], DtoEvent::MessageAppended(message) if message.streaming),
        "助手气泡在 base+1：{events:?}"
    );
    assert!(events.iter().any(|event| matches!(
        event,
        DtoEvent::ToolCallStarted { name, .. } if name == "read_game_log"
    )));
    let second = events
        .iter()
        .find_map(|event| match event {
            DtoEvent::MessageDelta { index, delta } if delta == "日志里没崩。" => {
                Some(*index)
            }
            _ => None,
        })
        .expect("第二轮文本要有增量");
    assert_eq!(second, 7, "工具条目占掉 6，下一段文本落在 7：{events:?}");
}

#[test]
fn a_provider_error_is_masked_before_it_reaches_the_rail() {
    let queue = EventQueue::default();
    // 与生产同一条脱敏：`probe::scrub` 会把「本次密钥原文」换成掩码
    let scrub = Scrubber::new(|text: &str| probe::scrub(text, Some("sk-live-abcdef123456")));
    let mut mapper = Mapper::new(queue.clone(), scrub);
    mapper.begin(0);
    mapper.on_event(&BrainEvent::TurnStarted {
        session_id: "s".into(),
        user_text: "hi".into(),
    });
    mapper.on_event(&BrainEvent::Error {
        message: "401 Unauthorized: key sk-live-abcdef123456 was rejected".into(),
    });

    let events = drained(&queue);
    let failed = events
        .iter()
        .find_map(|event| match event {
            DtoEvent::Failed { message } => Some(message.clone()),
            _ => None,
        })
        .expect("要有 Failed 事件");
    assert!(
        !failed.contains("sk-live-abcdef123456"),
        "密钥原文不许出现在横幅里：{failed}"
    );
    assert!(
        failed.contains(unlucky_agent::redact::MASK),
        "要留下掩码痕迹：{failed}"
    );
    assert!(failed.contains("401"), "人话要保留可诊断的部分：{failed}");
}

#[test]
fn a_turn_that_dies_without_an_error_event_still_settles_the_streaming_flag() {
    let (mut mapper, queue) = mapper();
    mapper.begin(0);
    mapper.on_event(&BrainEvent::TurnStarted {
        session_id: "s".into(),
        user_text: "hi".into(),
    });
    // worker 只知道 run_turn 返回了 Err（这一轮没有 Error 事件：不许把输入框永远锁住）
    mapper.end_turn(Some("provider 报错".into()));
    let events = drained(&queue);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, DtoEvent::Failed { .. })),
        "{events:?}"
    );
    assert!(matches!(
        events.last(),
        Some(DtoEvent::MessageFinished { .. }) | Some(DtoEvent::Failed { .. })
    ));
}

// ───────────────────── 五、M388：停止之后的「丢弃」 ─────────────────────

#[test]
fn a_muted_mapper_swallows_everything_until_the_next_turn() {
    let (mut mapper, queue) = mapper();
    mapper.begin(0);
    mapper.on_event(&BrainEvent::TurnStarted {
        session_id: "s".into(),
        user_text: "跑一轮".into(),
    });
    assert_eq!(mapper.mute(), 0, "此前没有丢弃过");
    let before = queue.lock().expect("queue").len();

    // 用户按了停止：worker 若还在吐（内核没被打断 / 停在工具里），这些都属于被停掉的那一轮
    mapper.on_event(&BrainEvent::TextDelta {
        text: "迟到的一句".into(),
    });
    mapper.end_turn(Some("本轮失败".into()));
    assert_eq!(
        mapper.dropped(),
        1,
        "只数被吞掉的**事件**；`end_turn` 是收尾调用，不是事件"
    );
    assert_eq!(
        queue.lock().expect("queue").len(),
        before,
        "静音之后一个事件都不许入队（否则『已取消』下面还会冒正文）"
    );

    // 下一轮 begin 自动解除静音（「取消后能立刻重试」靠这一行）
    mapper.begin(0);
    mapper.on_event(&BrainEvent::TextDelta {
        text: "下一轮".into(),
    });
    let events = drained(&queue);
    assert!(
        events.iter().any(|event| matches!(
            event,
            DtoEvent::MessageDelta { delta, .. } if delta == "下一轮"
        )),
        "下一轮必须照常产出：{events:?}"
    );
}
