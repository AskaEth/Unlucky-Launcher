//! 「存档区」（M235）的**模型**判据：快照 → 页面要画的东西。
//!
//! 为什么只测一个纯函数：iced 0.14 的 `Element` 没有内省接口（与 `tests/launch_args.rs`
//! 同一取舍），画出来的行在测试里拿不到。所以把"快照 → 行/计数/失败线"全部压在
//! [`saves::from_state`] 里，在这里逐条钉住——页面那一支只负责把这些字段画出来。
//!
//! 守门形状：`detail.rs` 必须调 `saves::section(ui, saves, …)`（`instances/tests.rs` 的源码
//! 结构断言盯着 `detail_column` 里 MODS 与 SAVES 的先后；这里守模型）。

use unlucky_presenter::dto::{
    SavesArea, SavesFailureKind, SavesFailureView, SavesListView, SavesSkipKind, SavesSkipView,
    ScreenshotPreview, ScreenshotView, ThumbnailView, WorldView,
};
use unlucky_presenter::state::SavesState;

use super::super::saves::{failure_line, from_state};
use super::super::Texts;
use unlucky_i18n::Lang;

/// 2024-05-01 10:00 UTC（UTC 毫秒）。
const STAMP: u64 = 1_714_557_600_000;

fn view(instance: &str) -> SavesListView {
    SavesListView {
        instance: instance.to_string(),
        dir: "C:/mc/saves".to_string(),
        exists: true,
        worlds: vec![WorldView {
            dir: "Better World".to_string(),
            icon: true,
            modified_ms: Some(STAMP),
        }],
        screenshots_dir: "C:/mc/screenshots".to_string(),
        screenshots_exists: true,
        screenshots: vec![ScreenshotView {
            file: "2024-05-01_10.00.00.png".to_string(),
            bytes: 2_048,
            modified_ms: None,
            // M251：一像素的缩略图——这条用例只关心「格子/计数/副文案」的映射，不关心解码。
            preview: ScreenshotPreview::Ready(ThumbnailView {
                width: 1,
                height: 1,
                pixels: vec![0x2E, 0x7D, 0x4F, 0xFF],
            }),
        }],
        skipped: vec![SavesSkipView {
            area: SavesArea::Screenshots,
            file: "notes.txt".to_string(),
            reason: SavesSkipKind::NotAnImage,
        }],
        last_op: None,
    }
}

/// 已绑定实例的视图态（`bind` 是 presenter 内部的 `pub(super)`，这里直接写字段——
/// 它本来就是公开状态的一部分，与 `AppState::bind_saves` 走的是同一条路）。
fn bound_state(instance: &str) -> SavesState {
    SavesState {
        bound: Some(instance.to_string()),
        ..SavesState::default()
    }
}

/// 快照 → 行：世界带「有图标」，截图带大小；时间**按 UTC 切**，取不到就说"时间未知"。
#[test]
fn the_snapshot_becomes_rows_with_stamps_and_badges() {
    let mut state = bound_state("a");
    state.changed(view("a"));
    let saves = from_state(&state, Lang::ZhCn.catalog());

    assert!(!saves.loading);
    assert!(!saves.retryable, "成功的快照不该带重试");
    assert_eq!(saves.worlds.len(), 1);
    assert_eq!(saves.worlds[0].name, "Better World");
    let zh_icon = Texts::of(Lang::ZhCn.catalog()).saves_world_icon();
    assert!(
        saves.worlds[0].detail.contains("2024-05-01 10:00 UTC"),
        "世界的副文案要带 UTC 时间：{:?}",
        saves.worlds[0].detail
    );
    assert!(
        saves.worlds[0].detail.contains(zh_icon),
        "有 icon.png 的世界要带标记：{:?}",
        saves.worlds[0].detail
    );

    assert_eq!(saves.screenshots.len(), 1);
    assert_eq!(
        saves.thumbnails(),
        1,
        "带了像素的截图必须变成一格真图（不是占位）"
    );
    assert!(
        saves.screenshots[0].detail.contains("2.0 KB"),
        "截图的副文案要带大小：{:?}",
        saves.screenshots[0].detail
    );
    assert!(
        saves.screenshots[0]
            .detail
            .contains(Texts::of(Lang::ZhCn.catalog()).saves_time_unknown()),
        "mtime 缺失时要说『时间未知』，不许拿 0 或当前时间糊：{:?}",
        saves.screenshots[0].detail
    );

    // 计数与跳过：3 个世界 / 12 张截图这种句子在这一层拼好。
    assert_eq!(saves.count, "1 个世界 · 1 张截图");
    assert_eq!(saves.skipped, 1);
    assert_eq!(saves.skipped_note, "跳过 1 项");
}

/// 首帧（已绑定、还没有快照）是 `loading`；没有绑定则什么都不算"已加载"。
#[test]
fn a_bound_instance_without_a_snapshot_is_loading() {
    let state = bound_state("a");
    let saves = from_state(&state, Lang::En.catalog());
    assert!(saves.loading);
    assert!(!saves.loaded());

    let idle = from_state(&SavesState::default(), Lang::En.catalog());
    assert!(!idle.loading, "不在详情页不该画『正在读取』");
    assert!(!idle.loaded());
}

/// 两个目录都不存在 ⇒ `missing`（画"还没有目录"那句，而不是"这里空空的"）。
/// 目录在但里面什么都没有 ⇒ `nothing_yet`。
#[test]
fn missing_directories_and_empty_ones_are_different_states() {
    let mut state = bound_state("a");
    let mut gone = view("a");
    gone.exists = false;
    gone.screenshots_exists = false;
    gone.worlds.clear();
    gone.screenshots.clear();
    gone.skipped.clear();
    state.changed(gone);
    let saves = from_state(&state, Lang::En.catalog());
    assert!(saves.missing);
    assert!(!saves.nothing_yet(), "缺目录不该被当成『目录在但是空的』");

    let mut state = bound_state("a");
    let mut empty = view("a");
    empty.worlds.clear();
    empty.screenshots.clear();
    empty.skipped.clear();
    state.changed(empty);
    let saves = from_state(&state, Lang::En.catalog());
    assert!(!saves.missing);
    assert!(saves.nothing_yet());
}

/// 失败：读盘失败**可重试**，打开文件夹失败**不可重试**（重试按钮会去重新列目录，那是错的动作）。
///
/// 抓得住：把两种失败合成一条「重试」；也抓得住"失败顺手把列表清掉"（用户看不见上一次读到的东西）。
#[test]
fn only_list_failures_offer_a_retry() {
    for (kind, retryable) in [
        (SavesFailureKind::DirUnusable, true),
        (SavesFailureKind::InstanceUnavailable, true),
        (SavesFailureKind::OpenFailed, false),
    ] {
        let mut state = bound_state("a");
        state.changed(view("a"));
        state.failed(SavesFailureView {
            instance: "a".to_string(),
            file: String::new(),
            op: None,
            kind,
            detail: "Access is denied.".to_string(),
        });
        let saves = from_state(&state, Lang::En.catalog());
        assert!(!saves.loading);
        assert_eq!(saves.retryable, retryable, "{kind:?} 的重试口径错了");
        assert!(saves.error.is_some(), "{kind:?} 没有画出失败");
        assert_eq!(saves.worlds.len(), 1, "{kind:?} 把旧列表清掉了");
        assert_eq!(saves.screenshots.len(), 1);
    }
}

/// **只拿到失败、一份快照都没有**时，不许被画成"这里还没有世界"。
///
/// 抓得住：`nothing_yet()` 只看"两个列表都空"——那样"读不了"会顶着两句"这里空空的"，
/// 用户会把权限问题读成"我没有存档"。
#[test]
fn a_failure_without_a_snapshot_is_not_an_empty_list() {
    let mut state = bound_state("a");
    state.failed(SavesFailureView {
        instance: "a".to_string(),
        file: String::new(),
        op: None,
        kind: SavesFailureKind::DirUnusable,
        detail: "Access is denied.".to_string(),
    });
    let saves = from_state(&state, Lang::En.catalog());
    assert!(saves.error.is_some());
    assert!(!saves.has_view, "没有快照却记成了有");
    assert!(!saves.nothing_yet(), "纯失败态被画成了空列表");
    assert_eq!(saves.count, "", "没有快照不该画计数");
}

/// 失败主句跟着语言与大类走，OS 原文只进括号（M161 的文案纪律）。
#[test]
fn the_failure_line_follows_the_language_and_keeps_the_detail_in_brackets() {
    let failure = SavesFailureView {
        instance: "a".to_string(),
        file: String::new(),
        op: None,
        kind: SavesFailureKind::DirUnusable,
        detail: "Access is denied.".to_string(),
    };
    let zh = failure_line(&failure, Texts::of(Lang::ZhCn.catalog()));
    assert!(zh.starts_with("读取 saves/ 失败："), "{zh}");
    assert!(zh.ends_with("（Access is denied.）"), "{zh}");

    let en = failure_line(&failure, Texts::of(Lang::En.catalog()));
    assert!(en.starts_with("Could not read saves/:"), "{en}");
    assert!(en.contains("Access is denied."), "{en}");

    // 没有原文时不留一对空括号。
    let bare = SavesFailureView {
        detail: String::new(),
        ..failure
    };
    let text = failure_line(&bare, Texts::of(Lang::ZhCn.catalog()));
    assert!(!text.contains('（'), "空原文留下了一对括号：{text}");
}

/// 时间换算：毫秒 → `YYYY-MM-DD HH:MM UTC`（跨年/闰日各钉一条，别只信一条）。
#[test]
fn millisecond_stamps_are_rendered_in_utc() {
    let stamp = |ms: u64| {
        let mut state = bound_state("a");
        let mut view = view("a");
        view.worlds[0].modified_ms = Some(ms);
        state.changed(view);
        from_state(&state, Lang::En.catalog()).worlds[0]
            .detail
            .clone()
    };
    // 1970-01-01 00:00 UTC（纪元）
    assert!(stamp(0).starts_with("1970-01-01 00:00 UTC"), "{}", stamp(0));
    // 2024-05-01 10:00 UTC
    assert!(
        stamp(STAMP).starts_with("2024-05-01 10:00 UTC"),
        "{}",
        stamp(STAMP)
    );
    // 2024-02-29 23:59 UTC（闰日）
    assert!(
        stamp(1_709_251_140_000).starts_with("2024-02-29 23:59 UTC"),
        "{}",
        stamp(1_709_251_140_000)
    );
}

mod preview;
mod write;
