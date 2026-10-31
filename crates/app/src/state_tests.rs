//! state.rs 的单元测试（拆文件保持单文件 ≤400 行）。

use super::{grid_positions, AiSession, AppState};
use crate::dialogs::{DialogKind, DialogRequest, DialogSource};
use crate::dto::{
    AgentEvent, AppEvent, ChatMessage, ChatRole, CoreEvent, Diagnosis, DiagnosisReport,
    InstanceRuntime, InstanceSummary, LoaderInfo, LoaderKind,
};
use crate::prefs::TileSize;
use crate::router::Route;
use crate::toast::Toast;
use crate::toast::ToastKind;

fn instance(id: &str, name: &str) -> InstanceSummary {
    InstanceSummary {
        id: id.to_string(),
        name: name.to_string(),
        mc_version: "1.20.4".to_string(),
        loader: LoaderInfo {
            kind: LoaderKind::Fabric,
            version: "0.16.5".to_string(),
        },
        last_played: None,
        runtime: InstanceRuntime::Idle,
        icon: None,
    }
}

#[test]
fn grid_positions_wrap_by_column_count() {
    assert_eq!(grid_positions(0, 4), vec![]);
    assert_eq!(grid_positions(3, 2), vec![(0, 0), (0, 1), (1, 0)]);
    // columns == 0 时按 1 列兜底，不允许除零
    assert_eq!(grid_positions(2, 0), vec![(0, 0), (1, 0)]);
}

#[test]
fn tiles_follow_tile_size_and_search() {
    let mut state = AppState::new();
    state.instances = vec![
        instance("a", "生存"),
        instance("b", "创造"),
        instance("c", "Demo"),
    ];

    assert_eq!(state.tiles().len(), 3);
    assert_eq!((state.tiles()[2].row, state.tiles()[2].col), (0, 2));

    state.prefs.tile_size = TileSize::Large;
    assert_eq!((state.tiles()[2].row, state.tiles()[2].col), (0, 2));

    state.prefs.tile_size = TileSize::Small;
    assert_eq!(state.tiles().len(), 3);

    state.instance_query = "demo".to_string();
    let tiles = state.tiles();
    assert_eq!(tiles.len(), 1);
    assert_eq!(tiles[0].instance.id, "c");
    assert_eq!((tiles[0].row, tiles[0].col), (0, 0));
}

#[test]
fn tiles_reflow_when_tile_size_shrinks() {
    let mut state = AppState::new();
    state.instances = (0..8)
        .map(|i| instance(&format!("i{i}"), &format!("实例{i}")))
        .collect();

    state.prefs.tile_size = TileSize::Medium;
    let medium_rows = state.tiles().iter().map(|cell| cell.row).max();

    state.prefs.tile_size = TileSize::Small;
    let small_rows = state.tiles().iter().map(|cell| cell.row).max();

    assert_eq!(medium_rows, Some(1)); // 8 / 4 列 = 2 行
    assert_eq!(small_rows, Some(1)); // 8 / 6 列 = 2 行
}

#[test]
fn runtime_events_update_the_matching_instance() {
    let mut state = AppState::new();
    state.instances = vec![instance("a", "生存"), instance("b", "创造")];

    state.apply(AppEvent::Core(CoreEvent::InstanceRuntimeChanged {
        id: "b".to_string(),
        runtime: InstanceRuntime::Running { pid: 42 },
    }));

    assert_eq!(state.instances[0].runtime, InstanceRuntime::Idle);
    assert_eq!(
        state.instances[1].runtime,
        InstanceRuntime::Running { pid: 42 }
    );
    assert_eq!(state.running().len(), 1);
    assert_eq!(state.running()[0].id, "b");
}

#[test]
fn progress_events_only_touch_busy_instances() {
    let mut state = AppState::new();
    state.instances = vec![instance("a", "生存")];
    state.instances[0].runtime = InstanceRuntime::Busy {
        stage: crate::dto::LaunchStage::Download,
        progress: crate::dto::Progress {
            done: 0,
            total: 100,
        },
    };

    state.set_progress(
        "a",
        crate::dto::Progress {
            done: 50,
            total: 100,
        },
    );
    match &state.instances[0].runtime {
        InstanceRuntime::Busy { progress, .. } => {
            assert!((progress.ratio() - 0.5).abs() < f32::EPSILON);
        }
        other => panic!("unexpected runtime: {other:?}"),
    }

    // 不存在的 id / 非 Busy 状态不改变任何东西
    state.set_progress("missing", crate::dto::Progress { done: 1, total: 1 });
    assert_eq!(state.instances.len(), 1);
}

#[test]
fn ai_stream_deltas_accumulate_and_finish() {
    let mut state = AppState::new();
    state.apply(AppEvent::Agent(AgentEvent::MessageAppended(
        ChatMessage::user("怎么装 1.20.4"),
    )));
    state.apply(AppEvent::Agent(AgentEvent::MessageAppended(
        ChatMessage::assistant_stream(),
    )));
    assert_eq!(state.ai.messages.len(), 2);
    assert!(state.ai.streaming);
    assert_eq!(state.ai.messages[1].role, ChatRole::Assistant);

    state.apply(AppEvent::Agent(AgentEvent::MessageDelta {
        index: 1,
        delta: "先".to_string(),
    }));
    state.apply(AppEvent::Agent(AgentEvent::MessageDelta {
        index: 1,
        delta: "下载".to_string(),
    }));
    state.apply(AppEvent::Agent(AgentEvent::MessageFinished { index: 1 }));

    assert_eq!(state.ai.messages[1].text, "先下载");
    assert!(!state.ai.messages[1].streaming);
    assert!(!state.ai.streaming);
}

#[test]
fn ai_session_finish_stream_clears_every_flag() {
    let mut session = AiSession::new();
    session.messages.push(ChatMessage::user("hi"));
    session.messages.push(ChatMessage::assistant_stream());
    session.finish_stream();
    assert!(session.messages.iter().all(|message| !message.streaming));
    assert!(!session.streaming);
}

#[test]
fn tool_call_finish_updates_the_last_matching_message() {
    let mut state = AppState::new();
    state.apply(AppEvent::Agent(AgentEvent::ToolCallStarted {
        name: "get_config".to_string(),
        summary: "读取设置".to_string(),
    }));
    state.apply(AppEvent::Agent(AgentEvent::ToolCallFinished {
        name: "get_config".to_string(),
        ok: true,
        summary: "读取到 3 项".to_string(),
    }));

    let tool = &state.ai.messages[0];
    assert_eq!(tool.role, ChatRole::Tool);
    assert_eq!(tool.text, "读取到 3 项");
    assert!(!tool.streaming);
    assert!(tool.tool.as_ref().is_some_and(|call| call.ok));
}

#[test]
fn dialogs_and_toasts_flow_through_apply() {
    let mut state = AppState::new();
    state.apply(AppEvent::DialogRequested(DialogRequest {
        id: 42,
        source: DialogSource::UserAction,
        title: "AI 想修改内存".to_string(),
        body: "改为 8G".to_string(),
        kind: DialogKind::Confirm {
            primary: "允许".to_string(),
            secondary: "拒绝".to_string(),
        },
    }));
    state.apply(AppEvent::Toast(Toast {
        id: 0,
        kind: ToastKind::Success,
        text: "下载完成".to_string(),
        remaining_ms: 0,
    }));

    assert_eq!(state.dialogs.pending_len(), 0);
    // id 归生产者：队列原样保留，DialogClosed(42) 才能命中同一条
    assert_eq!(state.dialogs.current().map(|dialog| dialog.id), Some(42));
    assert_eq!(state.toasts.len(), 1);

    state.apply(AppEvent::DialogClosed(42));
    assert!(state.dialogs.is_empty());
}

#[test]
fn navigate_to_agent_page_expands_the_ai_panel() {
    let mut state = AppState::new();
    state.prefs.ai_collapsed = true;
    state.navigate(Route::Agent);
    assert_eq!(state.route(), &Route::Agent);
    assert_eq!(state.ai_mode(), crate::router::AiPanelMode::Page);
    assert!(!state.prefs.ai_collapsed);
}

#[test]
fn toggling_ai_from_the_page_form_docks_it_again() {
    let mut state = AppState::new();
    state.navigate(Route::Agent);
    state.toggle_ai();
    assert_eq!(state.route(), &Route::Instances);
    assert_eq!(state.ai_mode(), crate::router::AiPanelMode::Docked);
    assert!(!state.prefs.ai_collapsed);

    state.toggle_ai();
    assert!(state.prefs.ai_collapsed);
}

#[test]
fn toggle_nav_flips_the_pref() {
    let mut state = AppState::new();
    state.toggle_nav();
    assert!(state.prefs.nav_collapsed);
    state.toggle_nav();
    assert!(!state.prefs.nav_collapsed);
}

#[test]
fn diagnosis_report_lands_on_state() {
    let mut state = AppState::new();
    state.apply(AppEvent::Core(CoreEvent::DiagnosisReady(DiagnosisReport {
        session: "s1".to_string(),
        diagnosis: Diagnosis {
            rule: "gpu.driver_old".to_string(),
            title: "显卡驱动过旧".to_string(),
            confidence: 92,
            hint: "升级驱动".to_string(),
            fix: Some("open_driver_page".to_string()),
        },
    })));
    assert!(state.diagnosis.is_some());
}
