//! worker 线程：一条活会话 + 串行轮次 + **超时/取消**两档收尾（M388）。
//!
//! 从 `real.rs` 拆出来：那边已经把三件事（装配 / 映射 / 闸门）各拆一份，本文件只放
//! 「会话线程自己怎么跑」，M388 又在这条循环里加了两档结局，再挤回 `real.rs` 会顶到 400 行。

use std::sync::{Arc, Mutex};

use unlucky_agent::brain::session::Session;
use unlucky_agent::brain::ProviderFactory;
use unlucky_agent::confirm::ConfirmGate;
use unlucky_agent::event::AgentEventSink;
use unlucky_agent::port::Ports;

use super::super::transport::{CancelSlot, Conclusion, ConclusionSlot, FailureSlot};
use super::super::AgentFailure;
use super::mapper::{failed, push_event, Mapper};
use super::{Job, Plan};

/// worker 线程：装着一条活会话，串行跑每一轮。
pub(super) fn run_worker(
    plan: &Plan,
    ports: &Ports,
    factory: &ProviderFactory,
    gate: Arc<dyn ConfirmGate>,
    sink: Arc<dyn AgentEventSink>,
    worker: Worker,
    intake: std::sync::mpsc::Receiver<Job>,
) {
    let scrub = plan.scrubber();
    let Some(mut session) = open_session(plan, ports, factory, gate, sink) else {
        if let Ok(mut slot) = worker.failure.lock() {
            *slot = Some(AgentFailure::NotConfigured);
        }
        let detail = scrub.apply(&format!(
            "会话装配失败：{}（设置文件 {}）",
            plan.config.redacted_debug(),
            plan.config_path.display()
        ));
        push_event(&worker.mapper.lock().expect("mapper").queue, failed(detail));
        return;
    };
    // 取消旗标交给 UI（「停止」那一侧要它）：槽是空的说明还在装配，此时没有可打断的东西。
    if let Ok(mut slot) = worker.cancel.lock() {
        *slot = Some(session.cancel_handle());
    }
    eprintln!(
        "[agent] 真后端就绪：{} · {} 个工具 · 审计 {} · YOLO {}（每次判定现读设置文件，改完即时生效）",
        plan.config.redacted_debug(),
        session.tool_names().len(),
        unlucky_agent::audit::audit_dir(&plan.data_root).display(),
        plan.yolo_now()
    );

    while let Ok(job) = intake.recv() {
        if let Ok(mut state) = worker.mapper.lock() {
            state.begin(job.base);
        }
        let outcome = unlucky_core::download::blocking::block_on(session.run_turn(&job.text));
        match &outcome {
            // 取消：用户已经当场看到「已取消」并拿回输入区（UI 侧收尾），这里**什么都不推**。
            // 映射器这时是静音的，推了也会被丢掉；日志把「丢弃了几条」写清楚。
            Err(unlucky_agent::brain::BrainError::Cancelled) => {
                let dropped = worker
                    .mapper
                    .lock()
                    .map(|mut state| {
                        state.end_turn(None);
                        state.dropped()
                    })
                    .unwrap_or(0);
                eprintln!(
                    "[agent] 本轮已被用户取消：内核在 await 处析构了请求 future（这是**中断**，\
                     不是等它跑完再丢）；另有 {dropped} 条**已产生**的事件被映射器丢弃"
                );
            }
            // 超时：**结论由 UI 按当前语言渲染**（worker 没有 catalog），这里只报事实 +
            // 把这一轮的流式气泡收掉。
            Err(unlucky_agent::brain::BrainError::Timeout { secs }) => {
                if let Ok(mut state) = worker.mapper.lock() {
                    state.end_turn(None);
                }
                if let Ok(mut slot) = worker.conclusion.lock() {
                    *slot = Some(Conclusion::Timeout { secs: *secs });
                }
                eprintln!("[agent] 本轮请求超时：{secs} 秒没有任何响应（结论由界面按语言渲染）");
            }
            other => {
                if let Ok(mut state) = worker.mapper.lock() {
                    state.end_turn(other.as_ref().err().map(ToString::to_string));
                }
            }
        }
    }
    eprintln!("[agent] 会话线程收工（UI 已释放）");
}

/// worker 线程与 UI 共享的四件东西（打包成一个结构体：`run_worker` 的参数已经够多）。
pub(super) struct Worker {
    pub(super) mapper: Arc<Mutex<Mapper>>,
    pub(super) failure: FailureSlot,
    pub(super) cancel: CancelSlot,
    pub(super) conclusion: ConclusionSlot,
}

/// 端口 + 会话的装配（失败返回 `None`，由调用方挂具名失败）。
fn open_session(
    plan: &Plan,
    ports: &Ports,
    factory: &ProviderFactory,
    gate: Arc<dyn ConfirmGate>,
    sink: Arc<dyn AgentEventSink>,
) -> Option<Session> {
    match Session::open(ports, factory, plan.session_config(), gate, sink) {
        Ok(session) => Some(session),
        Err(error) => {
            eprintln!("[agent] {error}");
            None
        }
    }
}
