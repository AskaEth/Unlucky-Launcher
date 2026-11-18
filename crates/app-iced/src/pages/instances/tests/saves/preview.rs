//! 缩略图墙的**页面模型判据**（M251）——从 `tests/saves.rs` 拆出（那边加完句柄复用的两条之后越过
//! 400 行硬线，与 `write.rs` 同一拆法）。
//!
//! 这一族只钉「快照里的三种预览状态 → 页面模型」与「句柄复用」，**不**画像素：
//! `Element` 没有内省接口，画出来对不对只能看 `DevDocs/artifacts/m251/` 的图（README 里写明哪张
//! 是排版、哪张是端到端真解码）。

use super::super::super::saves::clear_handles;
use super::super::super::saves::{from_state, Saves, ShotPreview};
use super::super::super::Texts;
use super::{bound_state, view, STAMP};
use unlucky_i18n::Lang;
use unlucky_presenter::dto::{
    ScreenshotPreview, ScreenshotThumbFailure, ScreenshotThumbKind, ScreenshotView, ThumbnailView,
};

/// 句柄 id 的取样口（只有 `Ready` 有句柄；别的一律 panic，免得断言在错误的状态上通过）。
fn handle_id(saves: &Saves) -> iced::widget::image::Handle {
    match &saves.screenshots[0].preview {
        ShotPreview::Ready { handle } => handle.clone(),
        other => panic!("第一格应当是缩略图，实际 {other:?}"),
    }
}

/// **缩略图墙的三种格子**（M251 · D54 第 6 条）：有像素 / 没轮到 / 具名失败，副文案各说各的。
///
/// 抓得住：①有像素却不画（`Ready` 被吞成占位）；②「没生成」与「失败」被合并成同一句
/// （用户没法分辨"刷新一下可能就有了"与"得自己去修文件"）；③失败原因没上屏（原文丢掉）。
/// 抓不住：像素画得对不对（`Element` 没有内省接口，只能靠 `DevDocs/artifacts/m251/` 的图）。
#[test]
pub(super) fn every_preview_state_gets_its_own_caption() {
    let texts = Texts::of(Lang::ZhCn.catalog());
    let mut snapshot = view("a");
    snapshot.screenshots = vec![
        ScreenshotView {
            file: "ok.png".to_string(),
            bytes: 2_048,
            modified_ms: Some(STAMP),
            preview: ScreenshotPreview::Ready(ThumbnailView {
                width: 2,
                height: 1,
                pixels: vec![1, 2, 3, 4, 5, 6, 7, 8],
            }),
        },
        ScreenshotView {
            file: "pending.png".to_string(),
            bytes: 4_096,
            modified_ms: Some(STAMP - 1),
            preview: ScreenshotPreview::NotGenerated,
        },
        ScreenshotView {
            file: "huge.png".to_string(),
            bytes: 9_437_184,
            modified_ms: Some(STAMP - 2),
            preview: ScreenshotPreview::Failed(ScreenshotThumbFailure {
                kind: ScreenshotThumbKind::TooLarge,
                detail: "9437184 > 8388608 字节".to_string(),
            }),
        },
    ];
    let mut state = bound_state("a");
    state.changed(snapshot);
    let saves = from_state(&state, Lang::ZhCn.catalog());

    assert_eq!(saves.thumbnails(), 1, "只有第一格有像素");
    assert!(matches!(
        saves.screenshots[0].preview,
        ShotPreview::Ready { .. }
    ));
    assert_eq!(
        saves.screenshots[1].preview,
        ShotPreview::Missing {
            note: texts.saves_preview_none().to_string()
        }
    );
    assert_eq!(
        saves.screenshots[2].preview,
        ShotPreview::Missing {
            note: texts
                .saves_preview_reason(ScreenshotThumbKind::TooLarge)
                .to_string()
        }
    );
    // 副文案三格一样是「大小 · 时间」：原因在框里，不把墙变得比图还吵（原文留在 DTO 里）。
    assert!(
        saves.screenshots[1].detail.starts_with("4.0 KB"),
        "没生成的那格也要有大小与时间：{:?}",
        saves.screenshots[1].detail
    );
    assert!(
        saves.screenshots[2].detail.starts_with("9.0 MB"),
        "失败的那格同样保留元数据：{:?}",
        saves.screenshots[2].detail
    );
}

/// 像素长度与宽高对不上时**不许**建句柄（`Handle::from_rgba` 不校验长度 ⇒ 画出来是花屏）。
///
/// 抓得住：把 `is_consistent()` 那道判断删掉（这时会走 `Ready`，`thumbnails()` 变 1 而断言要 0）。
#[test]
pub(super) fn inconsistent_pixels_are_shown_as_a_failure_not_a_broken_image() {
    let mut snapshot = view("a");
    snapshot.screenshots = vec![ScreenshotView {
        file: "short.png".to_string(),
        bytes: 10,
        modified_ms: Some(STAMP),
        preview: ScreenshotPreview::Ready(ThumbnailView {
            width: 4,
            height: 4,
            pixels: vec![0; 15],
        }),
    }];
    let mut state = bound_state("a");
    state.changed(snapshot);
    let saves = from_state(&state, Lang::ZhCn.catalog());
    assert_eq!(saves.thumbnails(), 0, "坏像素不该被当成图");
    assert_eq!(
        saves.screenshots[0].preview,
        ShotPreview::Missing {
            note: Texts::of(Lang::ZhCn.catalog())
                .saves_preview_reason(ScreenshotThumbKind::Decode)
                .to_string()
        }
    );
}

/// **句柄复用（M251 复审要求）**：同一张图两次 `from_state` 必须拿到**同一个 `Handle` id**。
///
/// 为什么这条是判据而不是细节：`Handle::from_rgba` 每次生成新 id，而 iced 的渲染器按 id 缓存
/// 纹理——id 一变就把像素重传一遍。`from_state` 挂在 `refresh_view` 上（每个 UI 事件都跑），
/// 一屏 60 张就是「每次事件重传 ≈6MB」。id 相同 ⇒ 纹理只上传一次。
///
/// 抓得住：把 `handle_for` 换回 `Handle::from_rgba(...)`（两次 id 必不相同）、键里漏掉 mtime/大小
/// （文件变了却复用旧句柄，见下一条）。
/// 抓不住：渲染器到底有没有复用纹理（那是 iced 内部，只能由「id 相同」推断）。
#[test]
pub(super) fn the_same_screenshot_reuses_one_handle() {
    clear_handles();
    let mut state = bound_state("a");
    state.changed(view("a"));

    let first = from_state(&state, Lang::ZhCn.catalog());
    let second = from_state(&state, Lang::ZhCn.catalog());
    assert_eq!(
        handle_id(&first).id(),
        handle_id(&second).id(),
        "同一张图在两次 refresh_view 里必须复用同一个句柄（否则每帧重传纹理）"
    );
}

/// 文件被覆盖（mtime 变）⇒ **必须换一个新句柄**（缓存键里含 mtime 的另一半证据）。
///
/// 抓得住：句柄键只按 `(目录, 文件名)`（截图被覆盖后仍画旧像素）。
#[test]
pub(super) fn a_changed_screenshot_gets_a_new_handle() {
    clear_handles();
    let mut state = bound_state("a");
    state.changed(view("a"));
    let first = from_state(&state, Lang::ZhCn.catalog());

    let mut changed = view("a");
    changed.screenshots[0].modified_ms = Some(STAMP + 1);
    let mut state = bound_state("a");
    state.changed(changed);
    let second = from_state(&state, Lang::ZhCn.catalog());

    assert_ne!(
        handle_id(&first).id(),
        handle_id(&second).id(),
        "mtime 变了就是另一张图，必须换句柄"
    );
}
