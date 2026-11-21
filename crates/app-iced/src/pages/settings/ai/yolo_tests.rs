//! M390：「跳过确认（YOLO）」在**状态层**的三条口径。
//!
//! | 用例 | 守什么 | 变异（改成什么会红） |
//! |---|---|---|
//! | `yolo_starts_off_and_reads_back_off` | 缺省 fail-closed：读不到 `ai.yolo` 就是关 | 默认 `true` |
//! | `a_dirty_draft_does_not_freeze_the_switch` | 脏草稿也照灌 YOLO | 把 YOLO 一起塞进 `if self.dirty` 那条早退 |
//! | `a_yolo_only_change_still_rebuilds_the_view` | 外部只改了 YOLO 时也要重建视图 | `changed` 里漏掉 `yolo_changed` |
//! | `the_screenshot_seed_knows_the_yolo_suffix` | 出图通道能出「开着」那张图 | 后缀没识别（永远出关着的图） |
//!
//! 为什么要单独一个文件：`tests.rs` 已经 390 行，而全仓单文件 ≤400 行是硬门禁
//! （`guards.yml` 的 file-size）。

use super::state::{ProviderId, Secret, Snapshot, State};

fn configured() -> State {
    State {
        enabled: true,
        provider: ProviderId::OpenAiCompatible,
        base_url: "https://api.deepseek.com/v1".to_string(),
        model: "deepseek-chat".to_string(),
        key: Secret::new("sk-test"),
        storage: true,
        ..State::default()
    }
}

/// 没有 `ai.yolo` 这一键时，快照与开关都是**关**（`fail-closed`：宁可多问一次）。
#[test]
fn yolo_starts_off_and_reads_back_off() {
    assert!(!State::default().yolo, "新建的状态必须是关的");
    assert!(!Snapshot::default().yolo, "快照缺省也是关的");

    let mut state = State::default();
    state.hydrate(Snapshot {
        yolo: true,
        ..Snapshot::default()
    });
    assert!(state.yolo, "文件里是 true 就得显示 true");

    state.hydrate(Snapshot::default());
    assert!(!state.yolo, "文件里没有这一键就得回到关");
}

/// **脏草稿不许把开关冻住**：用户正在改模型名时，`ull config set ai.yolo true`（或 AI 自己的
/// `set_config`）改的值必须照样出现在界面上。
///
/// 这是 M390 特意与别的键分开的那一条：其余四项在脏的时候**不灌**（不冲掉用户输入），
/// 而 YOLO 不是输入框——它是一个**正在生效的安全状态**，显示陈值就是让用户按错判断做决定。
#[test]
fn a_dirty_draft_does_not_freeze_the_switch() {
    let mut state = configured();
    state.model = "half-typed".to_string();
    state.dirty = true;

    state.hydrate(Snapshot {
        model: "deepseek-chat".to_string(),
        yolo: true,
        ..Snapshot::default()
    });

    assert_eq!(
        state.model, "half-typed",
        "脏草稿不许被冲掉（这是既有行为，M390 不动它）"
    );
    assert!(state.yolo, "YOLO 不是草稿：脏着也要跟着文件变");
}

/// 只改了 YOLO ⇒ `hydrate` 必须说「变了」，否则调用方不重建视图，开关停在旧值上。
#[test]
fn a_yolo_only_change_still_rebuilds_the_view() {
    let mut state = configured();
    let snapshot = Snapshot {
        enabled: state.enabled,
        provider: state.provider,
        base_url: state.base_url.clone(),
        model: state.model.clone(),
        key_chars: state.key_chars,
        yolo: true,
    };

    assert!(
        state.hydrate(snapshot),
        "只有 yolo 变了的这一帧必须报告 changed=true"
    );
    assert!(
        !state.hydrate(Snapshot {
            enabled: state.enabled,
            provider: state.provider,
            base_url: state.base_url.clone(),
            model: state.model.clone(),
            key_chars: state.key_chars,
            yolo: true,
        }),
        "什么都没变就不该重建"
    );
}

/// 出图通道：`-yolo` 后缀要给「开着」那一张图，别的种子行为不变。
#[test]
fn the_screenshot_seed_knows_the_yolo_suffix() {
    let off = State::screenshot_seed("configured").expect("种子");
    let on = State::screenshot_seed("configured-yolo").expect("带后缀的种子");
    assert!(!off.yolo, "不带后缀的种子仍是关着");
    assert!(on.yolo, "带 `-yolo` 的种子必须是开着");
    assert_eq!(on.model, off.model, "后缀不许把别的字段搞坏");
    assert!(
        State::screenshot_seed("nonsense-yolo").is_none(),
        "未知种子仍不许伪造状态"
    );
}
