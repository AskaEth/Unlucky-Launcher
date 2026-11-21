//! M388 的三条验收用例（停止 / 超时结论 / 切后端之后的超时）：从 `tests.rs` 拆出来守 400 行。
//!
//! 拆法照 `real/tests/`：同一个模块的子文件，`use super::*;` 继承父模块的夹具与引用。

use crate::app::state::presenter::transport::Conclusion;

use super::*;

// ---------------------------------------------------------------- M388：停止 / 超时 / 重试

#[test]
fn stopping_a_streaming_turn_converges_the_rail_and_keeps_the_text_it_already_showed() {
    let mut host = AgentHost::with_script(Script::Streaming);
    assert_eq!(host.submit("hello".to_string()), SubmitOutcome::Sent);
    for event in host.drain() {
        host.apply(event);
    }
    assert!(host.rail().busy, "前提：这一轮还在跑（停止键这时才出现）");

    let notice = "已取消：本轮不会再往下写，可以直接再发一句。".to_string();
    host.stop(notice.clone());

    let rail = host.rail();
    assert!(!rail.busy, "停止之后输入区必须立刻可用");
    assert!(!rail.streaming, "流式标记必须收敛（否则界面卡在忙碌态）");
    let last = rail.messages.last().expect("要有一条结论");
    assert_eq!(last.role, ChatRole::System, "结论是系统条目，不是助手气泡");
    assert_eq!(last.text, notice);
    assert_eq!(host.last_failure(), None, "用户自己按的停止不是错误");
    // 已经上屏的那半句话不擦：擦掉等于伪造历史（runner 侧只保证「不再往下追加」）
    assert!(rail.messages.iter().any(|message| !message.text.is_empty()));
}

#[test]
fn stopping_drops_events_that_were_already_queued_for_that_turn() {
    let mut host = AgentHost::with_script(Script::Streaming);
    assert_eq!(host.submit("hello".to_string()), SubmitOutcome::Sent);
    // **不抽队列**：这一批事件仍躺在队列里，属于用户即将停掉的那一轮
    host.stop("已取消".to_string());
    assert!(
        host.drain().is_empty(),
        "停止要把队列里属于本轮的事件一起丢掉，否则它们会在结论下面冒正文"
    );
    assert_eq!(
        host.rail().messages.last().map(|m| m.text.as_str()),
        Some("已取消")
    );
}

#[test]
fn the_turn_after_a_stop_runs_normally() {
    let mut host = AgentHost::with_script(Script::Qa);
    assert_eq!(host.submit("一问".to_string()), SubmitOutcome::Sent);
    for event in host.drain() {
        host.apply(event);
    }
    host.stop("已取消".to_string());
    assert!(!host.rail().busy);

    // 不重启、不手动清状态：直接再发一句
    assert_eq!(host.submit("再问一句".to_string()), SubmitOutcome::Sent);
    for event in host.drain() {
        host.apply(event);
    }
    let rail = host.rail();
    assert!(!rail.busy, "第二轮的脚本会收敛");
    assert!(
        rail.messages.len() >= 4,
        "两轮各两条起（user + assistant）：{:#?}",
        rail.messages
    );
}

#[test]
fn a_conclusion_lands_after_the_events_of_that_turn() {
    let mut host = AgentHost::with_script(Script::Qa);
    assert_eq!(host.submit("hello".to_string()), SubmitOutcome::Sent);
    for event in host.drain() {
        host.apply(event);
    }
    host.conclude("请求超时：5 秒没有任何响应。".to_string());
    let last = host.rail().messages.last().expect("要有一条结论");
    assert_eq!(last.role, ChatRole::System);
    assert!(!host.rail().busy, "结论落地要顺手解锁输入区");
}

#[test]
fn a_scripted_turn_reports_that_it_could_not_be_interrupted() {
    // 同步脚本 rig 没有线程可打断：`cancel` 的默认实现如实回 false（「只能丢弃」）。
    let mut host = AgentHost::with_script(Script::Streaming);
    assert_eq!(host.submit("hello".to_string()), SubmitOutcome::Sent);
    assert!(
        !host.stop("已取消".to_string()),
        "脚本 rig 打不断，不许谎报"
    );
}

// ------------------------------------------------ M388 r1 P2-1：切后端之后超时结论不能丢

/// 一个**收了请求就再也不说话**的本机端点（返回端口；连接被 accept 后一直挂着）。
///
/// 为什么不复用 `Scripted` 回放件：这条要证的正是「真后端的 await 永远不回来」，而回放件
/// 立刻 Ready。这里只需要一条真的 TCP 连接，不需要任何 HTTP 语义。
fn hanging_endpoint() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("绑本机端口");
    let port = listener.local_addr().expect("取端口").port();
    std::thread::spawn(move || {
        // 接住连接、一个字节都不写，直到进程退出（用例结束就没人管它了）
        if let Ok(stream) = listener.accept() {
            std::thread::sleep(Duration::from_secs(30));
            drop(stream);
        }
    });
    port
}

#[test]
fn a_timeout_after_switching_to_the_real_backend_still_reaches_the_ui() {
    // ① 启动时没有 launcher.json ⇒ 模拟态 + 带短时限的 reopen 通路
    let home = temp_home("switch-timeout");
    let config = home.join("config").join("launcher.json");
    let mut host = AgentHost::from_settings_path_with_timeout(&config, Duration::from_millis(120));
    assert!(host.is_simulated(), "前提：启动时 AI 还没配好");

    // ② 「设置页保存」：指向一个收了请求就不说话的本机端点
    let port = hanging_endpoint();
    std::fs::write(
        &config,
        format!(
            r#"{{"ai":{{"enabled":true,"provider":"openai_compatible","base_url":"http://127.0.0.1:{port}/v1","model":"m388-switch"}}}}"#
        ),
    )
    .expect("写 launcher.json");

    // ③ 下一句触发「无需重启」的切换；这一轮真后端沉默到时限
    assert_eq!(host.submit("hello".to_string()), SubmitOutcome::Sent);
    assert!(!host.is_simulated(), "填完 key 的这一句就该切到真后端");

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut conclusion = None;
    while Instant::now() < deadline {
        for event in host.drain() {
            host.apply(event);
        }
        if let Some(found) = host.take_conclusion() {
            conclusion = Some(found);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        matches!(conclusion, Some(Conclusion::Timeout { .. })),
        "切后端之后超时结论必须还能被 UI 取到（复审 r1 P2-1：漏刷 conclusion 槽会让界面静默卡住）"
    );

    // ④ UI 收尾：结论落地 + 输入区可用（与 `App::drain_agent` 同一条两步）
    host.conclude("请求超时：0 秒没有任何响应。".to_string());
    let rail = host.rail();
    assert!(!rail.busy, "结论落地之后输入区必须可用");
    assert!(!rail.streaming);
    assert_eq!(
        rail.messages.last().map(|message| message.role),
        Some(ChatRole::System),
        "结论是系统条目"
    );
}
