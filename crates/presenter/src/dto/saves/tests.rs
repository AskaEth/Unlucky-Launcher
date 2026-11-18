//! 存档 IPC 面的用例：**IPC round-trip**（生成物必须能反序列化回来）。

use super::*;

fn view() -> SavesListView {
    SavesListView {
        instance: "inst".to_string(),
        dir: "/tmp/run/saves".to_string(),
        exists: true,
        worlds: vec![WorldView {
            dir: "World".to_string(),
            icon: true,
            modified_ms: Some(1_700_000_000_000),
        }],
        screenshots_dir: "/tmp/run/screenshots".to_string(),
        screenshots_exists: true,
        screenshots: vec![ScreenshotView {
            file: "2024-05-01_10.00.00.png".to_string(),
            bytes: 1024,
            modified_ms: None,
            // M251：像素真的能过 IPC（4 字节 = 1×1 RGBA），这是「不落盘直接交给壳层」的前提。
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

/// 计数与 IPC round-trip（AGENTS §6：跨界类型必须能序列化）。
#[test]
fn view_counts_and_round_trips() {
    let view = view();
    assert_eq!(view.world_count(), 1);
    assert_eq!(view.screenshot_count(), 1);
    assert_eq!(view.skipped_count(), 1);
    assert!(!view.is_empty());

    let text = serde_json::to_string(&view).expect("序列化");
    assert_eq!(
        serde_json::from_str::<SavesListView>(&text).expect("反序列化"),
        view
    );
}

/// 五条指令都要能过 IPC（含写操作三条——它们带世界名，名字里可能有空格与中文）。
#[test]
fn commands_round_trip() {
    for command in [
        SavesCommand::List("inst".to_string()),
        SavesCommand::OpenFolder {
            id: "inst".to_string(),
        },
        SavesCommand::RenameWorld {
            id: "inst".to_string(),
            dir: "Better World".to_string(),
            to: "更好的世界".to_string(),
        },
        SavesCommand::DeleteWorld {
            id: "inst".to_string(),
            dir: "Better World".to_string(),
        },
        SavesCommand::BackupWorld {
            id: "inst".to_string(),
            dir: "Better World".to_string(),
            dest: Some("D:/backups".to_string()),
        },
        SavesCommand::BackupWorld {
            id: "inst".to_string(),
            dir: "Better World".to_string(),
            dest: None,
        },
    ] {
        let text = serde_json::to_string(&command).expect("序列化");
        assert_eq!(
            serde_json::from_str::<SavesCommand>(&text).expect("反序列化"),
            command
        );
    }
}

/// 成功回执也要能过 IPC（备份那一支字段最多：路径 / 字节 / 跳过数）。
#[test]
fn op_receipts_round_trip() {
    let cases = [
        SavesOped::Renamed {
            from: "Old".to_string(),
            to: "New".to_string(),
        },
        SavesOped::Deleted {
            dir: "Old".to_string(),
        },
        SavesOped::BackedUp {
            dir: "Old".to_string(),
            file: "Old-20240501-100000.zip".to_string(),
            path: "D:/exports/Old-20240501-100000.zip".to_string(),
            bytes: 4096,
            skipped: 2,
        },
    ];
    for op in cases {
        let mut view = view();
        view.last_op = Some(op);
        let text = serde_json::to_string(&view).expect("序列化");
        assert_eq!(
            serde_json::from_str::<SavesListView>(&text).expect("反序列化"),
            view
        );
    }
}

/// 两个目录都不存在 ⇒ 空（`is_empty` 只看内容，不看 `exists`：一个目录有空世界也算非空）。
#[test]
fn empty_means_no_worlds_and_no_screenshots() {
    let mut view = view();
    view.worlds.clear();
    view.screenshots.clear();
    assert!(view.is_empty(), "没有世界也没有截图就该算空");
    view.screenshots.push(ScreenshotView {
        file: "a.png".to_string(),
        bytes: 0,
        modified_ms: None,
        preview: ScreenshotPreview::NotGenerated,
    });
    assert!(!view.is_empty());
}

/// 预览的三种状态都要能过 IPC，且**失败原因逐条带得回来**（M251 · D54 第 6 条）。
#[test]
fn previews_round_trip_in_every_state() {
    let failure = ScreenshotThumbFailure {
        kind: ScreenshotThumbKind::TooLarge,
        detail: "9000000 > 8388608 字节".to_string(),
    };
    for preview in [
        ScreenshotPreview::NotGenerated,
        ScreenshotPreview::Ready(ThumbnailView {
            width: 2,
            height: 1,
            pixels: vec![1, 2, 3, 4, 5, 6, 7, 8],
        }),
        ScreenshotPreview::Failed(failure),
    ] {
        let text = serde_json::to_string(&preview).expect("序列化");
        assert_eq!(
            serde_json::from_str::<ScreenshotPreview>(&text).expect("反序列化"),
            preview
        );
    }
}

/// 「像素长度与宽高对得上」这条判据本身（壳层建 `Handle` 前要它成立）。
#[test]
fn thumbnail_consistency_catches_a_short_buffer() {
    let good = ThumbnailView {
        width: 2,
        height: 2,
        pixels: vec![0; 16],
    };
    assert!(good.is_consistent());
    let short = ThumbnailView {
        pixels: vec![0; 15],
        ..good.clone()
    };
    assert!(!short.is_consistent(), "少一个字节也必须判不一致");
}

/// 只有 `Ready` 才有像素可画（另外两支都是占位，界面不许把它们当成「黑图」）。
#[test]
fn only_ready_hands_out_pixels() {
    assert!(ScreenshotPreview::Ready(ThumbnailView {
        width: 1,
        height: 1,
        pixels: vec![0, 0, 0, 255],
    })
    .thumbnail()
    .is_some());
    assert!(ScreenshotPreview::NotGenerated.thumbnail().is_none());
    assert!(ScreenshotPreview::Failed(ScreenshotThumbFailure {
        kind: ScreenshotThumbKind::Unreadable,
        detail: "access denied".to_string(),
    })
    .thumbnail()
    .is_none());
}
