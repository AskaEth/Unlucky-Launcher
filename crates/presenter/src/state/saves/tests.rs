//! `SavesState` 的判据：绑定 / 迟到丢弃 / 三样交互态（含删除确认的安全不变量）。
//!
//! 从 `state/saves.rs` 拆出（父模块守 400 行）。**判据一条没改**，另外补了复审 r1 P1-1 / P2-1
//! 那两条：认框不认人、无关快照不再吃掉确认目标。

use super::*;
use crate::dto::{SavesFailureKind, WorldView};

fn view(instance: &str) -> SavesListView {
    SavesListView {
        instance: instance.to_string(),
        dir: "C:/mc/saves".to_string(),
        exists: true,
        worlds: vec![WorldView {
            dir: "World".to_string(),
            icon: false,
            modified_ms: None,
        }],
        screenshots_dir: "C:/mc/screenshots".to_string(),
        screenshots_exists: false,
        screenshots: Vec::new(),
        skipped: Vec::new(),
        last_op: None,
    }
}

fn failure(instance: &str) -> SavesFailureView {
    SavesFailureView {
        instance: instance.to_string(),
        file: String::new(),
        op: None,
        kind: SavesFailureKind::DirUnusable,
        detail: "Access is denied.".to_string(),
    }
}

/// 换实例必须清空（A 的存档不许画在 B 的详情页上）。
#[test]
fn binding_another_instance_drops_the_old_view() {
    let mut saves = SavesState::default();
    saves.bind(Some("a"));
    saves.changed(view("a"));
    assert!(saves.has_view());

    saves.bind(Some("b"));
    assert!(!saves.has_view(), "换实例后还留着 A 的存档");
    assert_eq!(saves.bound.as_deref(), Some("b"));

    // 同一个实例再 bind 一次不该清（`refresh_view` 每帧都调）。
    saves.changed(view("b"));
    saves.bind(Some("b"));
    assert!(saves.has_view());
}

/// 迟到/串台的结果被丢掉；成功一次清掉旧失败（重试成功后红字要消失）。
#[test]
fn late_results_are_dropped_and_success_clears_the_failure() {
    let mut saves = SavesState::default();
    saves.bind(Some("a"));
    saves.changed(view("b"));
    assert!(!saves.has_view(), "B 的列表画到了 A 上");

    saves.failed(failure("a"));
    assert!(saves.failed.is_some());
    assert!(saves.bound.is_some(), "失败不许把绑定清掉（要能重试）");
    saves.changed(view("a"));
    assert!(saves.failed.is_none(), "成功后旧失败没清掉");

    // 不在详情页（`bound = None`）时任何结果都不落地。
    let mut idle = SavesState::default();
    idle.changed(view("a"));
    idle.failed(failure("a"));
    assert!(!idle.has_view());
    assert!(idle.failed.is_none());
}

/// 换实例要把三样交互态一起清掉（草稿 / 在飞 / 确认目标）。
///
/// 抓得住：换到 B 之后「A 的草稿」还留着 ⇒ 用户点确定会去改 B 页上的一个不存在的名字。
#[test]
fn switching_instances_drops_draft_pending_and_confirm() {
    let mut saves = SavesState::default();
    saves.bind(Some("a"));
    saves.changed(view("a"));
    saves.start_rename("World");
    saves.edit_rename("Renamed".to_string());
    saves.ask_delete(7, "World");
    saves.mark_pending(SavesOp::Backup {
        dir: "World".to_string(),
    });
    assert!(saves.draft.is_none(), "发请求时草稿该收起来");
    assert!(saves.pending.is_some());

    saves.bind(Some("b"));
    assert!(saves.draft.is_none(), "换实例后草稿还在");
    assert!(saves.pending.is_none(), "换实例后在飞标记还在");
    assert!(saves.confirm.is_none(), "换实例后删除确认目标还在");
}

/// 回程（成功或失败）都要把「在飞」收掉：按钮不能永远灰着。
#[test]
fn both_outcomes_clear_the_pending_marker() {
    let mut saves = SavesState::default();
    saves.bind(Some("a"));
    saves.changed(view("a"));
    saves.mark_pending(SavesOp::Delete {
        dir: "World".to_string(),
    });
    assert!(saves.is_pending("World"));
    saves.failed(failure("a"));
    assert!(
        !saves.is_pending("World"),
        "失败后在飞标记没清：重试按钮会一直是灰的"
    );

    saves.mark_pending(SavesOp::Rename {
        dir: "World".to_string(),
    });
    assert!(saves.is_pending("World"));
    saves.changed(view("a"));
    assert!(!saves.is_pending("World"), "成功后没清在飞标记");
}

/// 确认目标只发一次（同一次确认连点两下不该发两条删除）。
#[test]
fn the_delete_confirmation_hands_out_once() {
    let mut saves = SavesState::default();
    saves.bind(Some("a"));
    saves.ask_delete(7, "Doomed");
    assert_eq!(saves.take_confirm(7).as_deref(), Some("Doomed"));
    assert!(
        saves.take_confirm(7).is_none(),
        "第二次取还拿得到：连点会删两次"
    );

    saves.ask_delete(8, "Other");
    saves.clear_confirm();
    assert!(saves.take_confirm(8).is_none(), "取消之后还留着目标");
}

/// **复审 r2 加固**：别人那个框被关掉时，不该清掉我们的删除目标。
///
/// `close_confirm(current)` 只在 `current` 等于登记的那个框时才清；`clear_confirm()` 仍然无条件
/// （换实例复位时用——那时"哪个框"已经不重要了）。
#[test]
fn closing_another_dialog_leaves_our_target_alone() {
    let mut saves = SavesState::default();
    saves.bind(Some("a"));
    saves.ask_delete(7, "Doomed");

    saves.close_confirm(99);
    assert_eq!(
        saves.take_confirm(7).as_deref(),
        Some("Doomed"),
        "别的框关掉时把我们的目标清掉了"
    );

    saves.ask_delete(8, "Other");
    saves.close_confirm(8);
    assert!(
        saves.take_confirm(8).is_none(),
        "自己的框关掉了，目标还在（下一次点确定会删上一个）"
    );
}

/// **P1-1 的核心**：目标只属于登记它的那个框。别的框点主按钮**拿不走**它。
///
/// 事故形态：`Esc` 关掉删除确认框（某条关闭路径漏清）→ 崩溃卡片弹出来 →
/// 点「导出诊断包」（也是 `Confirm` 的主按钮）→ 若 `take_confirm` 无条件交付，
/// 那个世界就被静默删掉了。判据是 **id 不等就不交付、也不清**（目标还是那个框的）。
#[test]
fn a_dialog_can_only_redeem_its_own_target() {
    let mut saves = SavesState::default();
    saves.bind(Some("a"));
    saves.ask_delete(7, "Doomed");

    assert!(
        saves.take_confirm(99).is_none(),
        "别的框的主按钮拿走了删除目标"
    );
    assert_eq!(saves.confirm_dialog(), Some(7), "拿不走也不该被顺手清掉");
    assert_eq!(
        saves.take_confirm(7).as_deref(),
        Some("Doomed"),
        "自己的框反而兑不了"
    );
}

/// **P2-1**：一拍无关快照（世界还在）不许把确认目标吃掉——否则主按钮就是静默空操作。
#[test]
fn an_unrelated_snapshot_no_longer_drops_the_confirmation() {
    let mut saves = SavesState::default();
    saves.bind(Some("a"));
    saves.changed(view("a"));
    saves.ask_delete(7, "World");

    // 「打开存档文件夹」的回程 / 备份在飞时的列表刷新：世界还在快照里。
    saves.changed(view("a"));
    assert_eq!(
        saves.take_confirm(7).as_deref(),
        Some("World"),
        "无关快照把确认目标吃掉了：主按钮会变成静默空操作"
    );

    // 读盘失败同理：一条失败不该把用户刚点出来的框弄成"点了没反应"。
    saves.ask_delete(8, "World");
    saves.failed(failure("a"));
    assert_eq!(saves.take_confirm(8).as_deref(), Some("World"));
}

/// **P2-1 的另一半**：被确认的世界从快照里消失 ⇒ 目标清掉，并且**登记那个框该收了**。
///
/// 只清不登记就是"框还在、主按钮点了什么也不发生"；只登记不清就是留着一个作废的目标。
#[test]
fn the_confirmation_is_dropped_and_reported_when_the_world_vanishes() {
    let mut saves = SavesState::default();
    saves.bind(Some("a"));
    saves.changed(view("a"));
    saves.ask_delete(7, "World");

    // 快照回来时那个世界已经不在列表里（别的进程删掉了 / 改名了）。
    let mut gone = view("a");
    gone.worlds.clear();
    saves.changed(gone);

    assert!(saves.confirm.is_none(), "目标还留着一个已经不存在的世界");
    assert_eq!(
        saves.take_closed_confirm(),
        Some(7),
        "没有登记那个框 ⇒ 它会一直挡在屏幕中央、主按钮点了没反应"
    );
    assert!(saves.take_confirm(7).is_none(), "作废的目标还能被兑现");
    assert!(saves.take_closed_confirm().is_none(), "信号只发一次");
}

/// 行内草稿照原样存：非法名字不在前端过滤（core 会拒并给出具名原因）。
#[test]
fn the_rename_draft_keeps_the_typed_text_verbatim() {
    let mut saves = SavesState::default();
    saves.bind(Some("a"));
    saves.start_rename("World");
    assert_eq!(
        saves.draft.as_ref().expect("草稿").text,
        "World",
        "应预填当前名字"
    );
    saves.edit_rename("../escape".to_string());
    let draft = saves.take_rename().expect("草稿");
    assert_eq!(draft.dir, "World");
    assert_eq!(draft.text, "../escape", "输入被前端偷偷改了");
    assert!(saves.take_rename().is_none(), "取走之后草稿还在");
}
