//! `AgentHost` 的用例（M384 r1 起从 `agent.rs` 拆出来：那边贴到了 406 行）。

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use unlucky_presenter::agent::AgentTransport;
use unlucky_presenter::dto::{AgentEvent as DtoEvent, AppEvent, ChatRole};

use super::*;

/// 一次性的临时「数据根」（`launcher.json` 落在 `<home>/config/` 下）。
fn temp_home(name: &str) -> PathBuf {
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "ull-agenthost-{name}-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("config")).expect("建 config 目录");
    dir
}

/// 「设置页保存」的等价动作：写出那份 `launcher.json`（设置页经 `SettingsBridge` 落的是同一个文件）。
fn write_ai_config(home: &Path) -> PathBuf {
    let config = home.join("config").join("launcher.json");
    let body = r#"{"ai":{"enabled":true,"provider":"openai_compatible","base_url":"http://127.0.0.1:1/v1","model":"m384-test-model"}}"#;
    std::fs::write(&config, body).expect("写 launcher.json");
    config
}

/// 抽干队列直到看见一条 `Failed`（真后端的失败要以事件回流，最多等 10 秒）。
fn pump_failed(host: &mut AgentHost) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let mut failed = false;
        for event in host.drain() {
            failed |= matches!(event, AppEvent::Agent(DtoEvent::Failed { .. }));
            host.apply(event);
        }
        if failed {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn each_failure_code_is_stable_ascii_and_has_its_own_body_key() {
    assert_eq!(AgentFailure::Unreachable.code(), "unreachable");
    assert_eq!(AgentFailure::NotImplemented.code(), "not_implemented");
    assert_eq!(AgentFailure::NotConfigured.code(), "not_configured");
    assert_ne!(
        AgentFailure::Unreachable.body_key(),
        AgentFailure::NotImplemented.body_key(),
        "两种失败要两句话，不能都糊成一句"
    );
    assert_ne!(
        AgentFailure::NotConfigured.body_key(),
        AgentFailure::NotImplemented.body_key(),
        "「没配好」与「没接线」是两件事，各说各的"
    );
}

#[test]
fn a_missing_config_degrades_explicitly_and_never_silently() {
    // 走**真入口**（复审 r1 的 #8）：一份不存在的 `launcher.json` ⇒ 具名原因 + 模拟标记，
    // 而且脚本化 rig 仍然可用（不是死路）。
    let home = temp_home("degrade");
    let mut host = AgentHost::from_settings_path(&home.join("config").join("launcher.json"));
    assert!(host.is_simulated(), "降级必须自报「模拟」");
    assert_eq!(
        host.last_failure(),
        Some(AgentFailure::NotConfigured),
        "降级理由是具名的，不是静默"
    );
    assert_eq!(host.submit("hello".to_string()), SubmitOutcome::Sent);
}

#[test]
fn filling_the_key_takes_effect_on_the_next_submit_without_restarting() {
    // M384 r1 复审 P2-1 的验收：填完 key → 保存 → **下一句**就是真模型回的，不用重启。
    let home = temp_home("p2-1");
    let config = home.join("config").join("launcher.json");
    let mut host = AgentHost::from_settings_path(&config);
    assert!(host.is_simulated(), "启动时没有任何 AI 配置");

    // 第一句：脚本化 rig（模拟态），跑完把事件收进 rail
    assert_eq!(host.submit("hello".to_string()), SubmitOutcome::Sent);
    for event in host.drain() {
        host.apply(event);
    }
    assert!(host.is_simulated(), "还没配置，应该仍在模拟态");

    // 「设置页保存」= 写这份 launcher.json（同一个文件、同一条 to_config 判据）
    assert_eq!(write_ai_config(&home), config);

    // 第二句：重试真后端成功 ⇒ 这句话由真 provider 回答，且不再挂「模拟」
    assert_eq!(host.submit("再看看".to_string()), SubmitOutcome::Sent);
    assert!(!host.is_simulated(), "填完 key 之后不该再挂着「模拟」");
    assert_eq!(host.last_failure(), None, "过期的「未配置」横幅要清掉");

    // 真 provider 真的跑了：回环端口 1 必然拒绝，失败要以 `Failed` 事件回流（会话仍可用）
    assert!(pump_failed(&mut host), "真后端的失败要以 Failed 事件回流");
    assert!(
        !host.rail().busy,
        "失败后 busy 必须收敛，输入框不能永远禁用"
    );
    assert!(host.rail().error.is_some(), "横幅要有原因");
}

#[test]
fn a_live_backend_is_never_downgraded_by_a_later_settings_change() {
    // 反方向守护：已经是真后端之后，设置文件被改坏/删掉也不该被踢回脚本 rig
    // （那会静默把用户从真模型换回假回答）。重启才重新判定。
    let home = temp_home("p2-1-sticky");
    let config = write_ai_config(&home);
    let mut host = AgentHost::from_settings_path(&config);
    assert!(!host.is_simulated(), "配置完整就该直接上真后端");

    std::fs::remove_file(&config).expect("删掉设置文件");
    assert_eq!(host.submit("hello".to_string()), SubmitOutcome::Sent);
    assert!(!host.is_simulated(), "已建的真后端会话不因设置变化而降级");
    let _ = pump_failed(&mut host);
}

/// **M384 r2 复审 P2-1 的守门用例（承重墙）**：提交成功到第一个事件到达之间必须算忙。
///
/// 为什么这条必须有：`agent_submit` 收尾会按消息重算 `streaming`（`settle_streaming`）。
/// 对异步后端（`submit` 回空 `Vec`、事件下一拍才回流）那一步会把 `begin_turn()` 刚点亮的
/// `busy` 当场灭掉 ⇒ 真后端第一个 token 之前（几百毫秒到几秒）发送键可用、`Stop` 不出现；
/// 再按一次 Enter 就会两轮并行，而第二轮用旧的 `rail.messages.len()` 当 `base`，
/// `MessageDelta` 会指到上一轮的气泡上（dev 构建里 `rail.rs` 的 `debug_assert` 直接 panic）。
///
/// **改坏了就红**：把 `RealTransport::is_async()` 改回默认 `false`（或不申报），这条会立刻红
/// ——r2 报告里贴了那一次实测的 FAILED 输出。
#[test]
fn the_rail_is_busy_between_submit_and_the_first_event() {
    let home = temp_home("busy-window");
    let config = write_ai_config(&home);
    let mut host = AgentHost::from_settings_path(&config);
    assert!(!host.is_simulated(), "这个夹具应当直接上真后端");

    // 提交成功、**还没有抽过任何事件**
    assert_eq!(host.submit("在吗".to_string()), SubmitOutcome::Sent);
    assert!(
        host.rail().busy,
        "第一个事件到达之前必须算忙（否则发送键可用、能起第二轮）"
    );

    // 事件回来后按 rail 自己的收敛点收场（真 provider 打不通 ⇒ Failed ⇒ busy 灭）
    assert!(pump_failed(&mut host));
    assert!(!host.rail().busy, "失败之后必须收敛，不能留下假忙");
}

/// 同一条墙的另一半：**同步内联**的后端不许被这条新语义锁死。
///
/// `agent_submit` 只有在后端申报 `is_async()` 时才跳过收敛；一个确实一个事件都不回的后端
/// （`SilentTransport` 那种形状）仍按老口径收场——presenter 自己的用例
/// `submit_with_a_silent_backend_does_not_leave_the_shell_busy` 钉的是同一件事。
#[test]
fn a_synchronous_backend_still_settles_busy_right_after_submit() {
    struct Silent;
    impl AgentTransport for Silent {
        fn submit(&mut self, _text: &str, _history: &AgentRail) -> Result<Vec<AppEvent>, String> {
            Ok(Vec::new())
        }
        fn confirm(&mut self, _request_id: &str, _accept: bool) -> Vec<AppEvent> {
            Vec::new()
        }
    }

    let mut presenter = Presenter::new(Box::new(Silent));
    presenter
        .agent_submit("在吗".to_string())
        .expect("提交应当成功");
    assert!(
        !presenter.agent_rail().busy,
        "没申报异步的后端仍按老口径：提交完就重算，不锁输入框"
    );
}

/// `agent_submit_ready` 与 `agent_submit` 的三道闸必须同真同假（壳在换后端之前要问这一句）。
#[test]
fn submit_ready_agrees_with_agent_submit() {
    let mut host = AgentHost::with_script(Script::Qa);
    // 1) 空文本：两边都拒
    assert!(!host.presenter.agent_submit_ready("   "));
    assert!(host.presenter.agent_submit(String::new()).is_err());

    // 2) 正常一句：ready 为真，提交成功
    assert!(host.presenter.agent_submit_ready("hello"));
    assert_eq!(host.submit("hello".to_string()), SubmitOutcome::Sent);

    // 3) 一轮在跑（脚本还没被 apply）：两边都判忙
    assert!(!host.presenter.agent_submit_ready("again"));
    assert_eq!(
        host.presenter.agent_submit("again".to_string()),
        Err("上一轮回复还没结束".to_string())
    );
}

#[test]
fn a_submit_only_reaches_the_backend_once_per_turn_while_busy() {
    let mut host = AgentHost::with_script(Script::Streaming);
    assert_eq!(host.rail().messages.len(), 0);
    assert_eq!(host.submit("hello".to_string()), SubmitOutcome::Sent);
    // 队列里的这一批要经 `apply` 才进 rail（app 层由 `Message::Agent(Events)` 做同一件事）
    for event in host.drain() {
        host.apply(event);
    }
    assert!(host.rail().busy, "流式没收敛前输入要禁用");
    // 第二轮被 presenter 拒收（不是服务器错误，所以不进横幅）
    assert_eq!(host.submit("again".to_string()), SubmitOutcome::Rejected);
    assert_eq!(host.last_failure(), None);
}

#[test]
fn an_unreachable_endpoint_is_named_not_stringified() {
    let mut host = AgentHost::with_script(Script::Unreachable);
    assert_eq!(
        host.submit("hello".to_string()),
        SubmitOutcome::Failed(AgentFailure::Unreachable)
    );
    assert_eq!(host.last_failure(), Some(AgentFailure::Unreachable));
    assert!(host.drain().is_empty());
}

#[test]
fn visibility_is_written_back_once_per_change() {
    let mut host = AgentHost::with_script(Script::Qa);
    host.sync_visibility(false, true);
    host.sync_visibility(false, true);
    assert!(host.presenter.state().prefs.ai_collapsed);
    host.sync_visibility(true, false);
    assert_eq!(host.presenter.state().ai_mode(), AiPanelMode::Page);
    assert!(!host.presenter.state().prefs.ai_collapsed);
    host.sync_visibility(false, false);
    assert_eq!(host.presenter.state().ai_mode(), AiPanelMode::Docked);
}

// ------------------------------------------------ M388 r1 P2-1：切后端之后超时结论不能丢

// 两个「用例入口」放在这里而不是 `agent.rs`：那边贴着 400 行硬线（复审 r1 之后又长了两条断言）。
// 固有 impl 可以写在同 crate 的**子模块**里，`#[cfg(test)]` 下才编译，所以产品快照一位不变。

impl Reopen {
    /// 指定设置文件 **+ 指定空闲时限**的通路（复审 r1 P2-1 的「切后端后超时」那条用例要它）。
    pub(crate) fn at_path_with_timeout(
        path: std::path::PathBuf,
        timeout: std::time::Duration,
    ) -> Self {
        Self(Arc::new(move || {
            crate::app::state::presenter::real::Plan::at(&path)
                .map(|plan| plan.with_timeout(timeout))
                .map(crate::app::state::presenter::real::RealTransport::from_plan)
        }))
    }
}

impl AgentHost {
    /// 同 [`AgentHost::from_settings_path`]，但把切到真后端之后的**空闲时限**压短。
    pub(crate) fn from_settings_path_with_timeout(
        path: &std::path::Path,
        timeout: std::time::Duration,
    ) -> Self {
        let reopen = Reopen::at_path_with_timeout(path.to_path_buf(), timeout);
        Self::from_choice(reopen.call(), Some(reopen))
    }
}
#[cfg(test)]
mod stop;
