//! 自更新视图模型的单测（从 `pages/update.rs` 拆出，那边贴着 400 行硬线）。

use super::*;

use unlucky_i18n::Lang;

/// 提示条的正文**同时**给体积与说明摘要（mission：提示条含版本号、体积、说明摘要）。
#[test]
fn the_available_banner_carries_both_size_and_notes() {
    for lang in [Lang::En, Lang::ZhCn] {
        let body = available_body(lang.catalog(), 21_500_000, "修掉启动崩溃");
        assert!(body.contains("20.5 MB"), "{lang:?} 缺体积：{body}");
        assert!(body.contains("修掉启动崩溃"), "{lang:?} 缺说明：{body}");
    }
    // 发布没写说明时只留体积，不留一个空尾巴。
    let bare = available_body(Lang::En.catalog(), 1024, "  ");
    assert!(bare.contains("1.0 KB"), "{bare}");
    assert!(!bare.contains('·'), "没有说明时不该留分隔符：{bare}");
}

/// 视图模型的提示条判据与 presenter 同序（就位 > 进行中 > 失败 > 新版本）。
#[test]
fn the_banner_priority_matches_the_presenter() {
    let mut update = Update {
        status: UpdateStatus::Available {
            version: "0.2.3".to_string(),
            size: 1,
            notes: String::new(),
            forced: false,
        },
        ..Update::default()
    };
    assert!(matches!(update.banner(), Some(Banner::Available { .. })));

    update.skipped = Some("0.2.3".to_string());
    assert!(update.banner().is_none(), "忽略之后不再打扰");

    // 忽略的是**那一个版本**：同一次运行里冒出 0.2.4 仍然提示（与 presenter 同一语义）。
    update.status = UpdateStatus::Available {
        version: "0.2.4".to_string(),
        size: 1,
        notes: String::new(),
        forced: false,
    };
    assert!(
        matches!(update.banner(), Some(Banner::Available { version, .. }) if version == "0.2.4"),
        "0.2.3 被忽略不该压住 0.2.4"
    );

    update.skipped = None;
    update.applying = true;
    assert!(matches!(update.banner(), Some(Banner::Applying)));

    update.ready = Some("0.2.3".to_string());
    assert!(matches!(update.banner(), Some(Banner::Ready { .. })));
}
