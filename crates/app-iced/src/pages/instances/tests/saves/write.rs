//! 存档**写操作**的页面模型判据（M247）——从 `tests/saves.rs` 拆出（那边越过 400 行）。
//!
//! 三条都在 `from_state` 这一层钉：写失败的主句（哪一行 + 哪个动作 + 什么原因）、
//! 「游戏在运行」不是「写盘失败」（不留重试按钮）、以及草稿 / 在飞 / 回执变成页面字段。

use super::super::super::saves::{failure_line, from_state, op_line};
use super::super::super::Texts;
use super::{bound_state, view};
use unlucky_i18n::Lang;
use unlucky_presenter::dto::{SavesFailureKind, SavesFailureOp, SavesFailureView, SavesOped};
use unlucky_presenter::state::SavesOp;

/// 写操作失败：**不画重试**（重试按钮会去重新列目录，那是错的动作），并且主句带**哪个世界**。
///
/// 抓得住：三种写失败被折叠成同一句「读取 saves/ 失败」——用户看不出是哪一行出了什么事。
#[test]
fn write_failures_are_named_per_operation_and_never_retryable() {
    let cases = [
        (
            SavesFailureOp::Rename,
            SavesFailureKind::WriteConflict,
            "重命名 Better World 失败：同名世界已存在",
        ),
        (
            SavesFailureOp::Delete,
            SavesFailureKind::WriteMissing,
            "删除 Better World 失败：那个世界已不在",
        ),
        (
            SavesFailureOp::Backup,
            SavesFailureKind::WriteTooLarge,
            "备份 Better World 失败：这个世界太大，一个 zip 装不下",
        ),
    ];
    for (op, kind, expected) in cases {
        let failure = SavesFailureView {
            instance: "a".to_string(),
            file: "Better World".to_string(),
            op: Some(op),
            kind,
            detail: String::new(),
        };
        let line = failure_line(&failure, Texts::of(Lang::ZhCn.catalog()));
        assert_eq!(line, expected);

        let mut state = bound_state("a");
        state.changed(view("a"));
        state.failed(failure);
        let saves = from_state(&state, Lang::ZhCn.catalog());
        assert!(!saves.retryable, "{op:?} 失败了还画着重试按钮");
        assert!(saves.error.is_some());
        assert_eq!(saves.worlds.len(), 1, "{op:?} 失败把列表清了");
    }
}

/// 「游戏在跑」不是「写盘失败」：主句要直说先退出，而且**不画重试**。
///
/// 抓得住：把这条折进 `WriteFailed` —— 用户会看到「删除 A 失败：写盘失败」，然后以为是权限问题，
/// 反复重试；真正该做的事（退出游戏）一个字都没说。
#[test]
fn a_running_game_says_exit_first_not_write_failed() {
    let cases = [
        (
            SavesFailureOp::Rename,
            "重命名 Better World 失败：游戏正在运行（先退出再操作）",
        ),
        (
            SavesFailureOp::Delete,
            "删除 Better World 失败：游戏正在运行（先退出再操作）",
        ),
        (
            SavesFailureOp::Backup,
            "备份 Better World 失败：游戏正在运行（先退出再操作）",
        ),
    ];
    for (op, expected) in cases {
        let failure = SavesFailureView {
            instance: "a".to_string(),
            file: "Better World".to_string(),
            op: Some(op),
            kind: SavesFailureKind::GameRunning,
            detail: String::new(),
        };
        assert_eq!(
            failure_line(&failure, Texts::of(Lang::ZhCn.catalog())),
            expected
        );
        assert!(
            !expected.contains("写盘失败"),
            "「游戏在跑」被说成了「写盘失败」"
        );

        let mut state = bound_state("a");
        state.changed(view("a"));
        state.failed(failure);
        let saves = from_state(&state, Lang::ZhCn.catalog());
        assert!(!saves.retryable, "{op:?}：在飞被拒还画重试按钮");
        assert_eq!(saves.worlds.len(), 1, "{op:?}：被拒把列表清了");
    }
    // 英文那一侧同一条判据（主语不同、意思相同）。
    let failure = SavesFailureView {
        instance: "a".to_string(),
        file: "Better World".to_string(),
        op: Some(SavesFailureOp::Delete),
        kind: SavesFailureKind::GameRunning,
        detail: String::new(),
    };
    let en = failure_line(&failure, Texts::of(Lang::En.catalog()));
    assert_eq!(
        en,
        "Could not delete Better World: the game is running (exit it first)"
    );
}

/// 三样交互态（草稿 / 在飞 / 回执）都要变成页面字段：改名中的那一行、置灰的那一行、成功那句话。
#[test]
fn draft_pending_and_receipt_become_page_fields() {
    let mut state = bound_state("a");
    state.changed(view("a"));
    state.start_rename("Better World");
    state.edit_rename("Renamed".to_string());
    let saves = from_state(&state, Lang::En.catalog());
    let editing = saves.editing.as_ref().expect("草稿没进页面模型");
    assert_eq!(editing.dir, "Better World");
    assert_eq!(editing.text, "Renamed");
    assert!(!saves.busy_with("Better World"), "还在打字就算在飞了");

    state.mark_pending(SavesOp::Backup {
        dir: "Better World".to_string(),
    });
    let saves = from_state(&state, Lang::En.catalog());
    assert!(saves.editing.is_none(), "发请求时草稿该收起来");
    assert!(saves.busy_with("Better World"));
    assert!(!saves.busy_with("Other"));

    // 回执：host 写在快照里，下一次普通 List 就没了。
    let mut done = view("a");
    done.last_op = Some(SavesOped::BackedUp {
        dir: "Better World".to_string(),
        file: "Better World-20240501-100000.zip".to_string(),
        path: "D:/ULL/.ULL/data/exports/saves/Better World-20240501-100000.zip".to_string(),
        bytes: 2_048,
        skipped: 2,
    });
    state.changed(done);
    let saves = from_state(&state, Lang::ZhCn.catalog());
    let notice = saves.notice.as_deref().expect("回执没进页面模型");
    assert!(notice.contains("2.0 KB"), "回执要带大小：{notice}");
    assert!(
        notice.contains("Better World-20240501-100000.zip"),
        "{notice}"
    );
    assert!(
        notice.contains("跳过 2 个符号链接"),
        "跳过的链接要报出来：{notice}"
    );
    assert!(saves.busy.is_none(), "回程之后还在飞");
}

/// 三种回执的措辞（重命名 / 删除 / 备份）。
#[test]
fn every_receipt_says_what_happened() {
    let texts = Texts::of(Lang::ZhCn.catalog());
    assert_eq!(
        op_line(
            &SavesOped::Renamed {
                from: "A".to_string(),
                to: "B".to_string()
            },
            texts
        ),
        "已把 A 改名为 B。"
    );
    assert_eq!(
        op_line(
            &SavesOped::Deleted {
                dir: "A".to_string()
            },
            texts
        ),
        "已删除 A。"
    );
    let backed_up = op_line(
        &SavesOped::BackedUp {
            dir: "A".to_string(),
            file: "A.zip".to_string(),
            path: "D:/x/A.zip".to_string(),
            bytes: 0,
            skipped: 0,
        },
        texts,
    );
    assert!(
        backed_up.starts_with("已把 A 备份到 D:/x/A.zip"),
        "{backed_up}"
    );
    assert!(
        !backed_up.contains("符号链接"),
        "没跳过就别提链接：{backed_up}"
    );
}
