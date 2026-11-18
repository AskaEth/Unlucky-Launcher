//! M222 D30 r1（S1）的装配判据：选项区变高时怎么分高、菜单占几行。
//!
//! 单独一个文件是守 400 行硬线（`tests.rs` 已经装了 M105 那批兼容性/确认键的用例）。
//! 这里的数字来自出图实测：1180×760 下版本行高 46.33px、选项区从 204px 长到 372px 时
//! 版本列表从 6.0 行被压到 2.4 行。

use unlucky_i18n::Lang;
use unlucky_presenter::dto::{LoaderKind, VersionKind};
use unlucky_presenter::state::AppState;

use super::model::{Loader, OptionsView};
use super::tests::{draft, row, state_with};

/// S1 的前置：加载器版本菜单**只按条目数占高**（一行为 64、两行为 112），不空撑两行。
///
/// 抓得住：菜单高度改回「固定两行」（LiteLoader 只有 1 条版本时白占 48px，正好把上面版本
/// 列表挤掉一行）；或者有人把留白算丢（高度不是 64/112 这两个整行值，菜单项会被截半行）。
#[test]
fn version_menu_height_counts_whole_items_only() {
    use super::view::version_menu_height;
    assert_eq!(version_menu_height(0), 64.0, "空表也按一行占（不塌成 0）");
    assert_eq!(version_menu_height(1), 64.0, "一条版本 = 一行项 + 上下留白");
    assert_eq!(version_menu_height(2), 112.0, "两条 = 两行");
    assert_eq!(
        version_menu_height(50),
        112.0,
        "再多也只露两行（余下靠菜单内滚动）"
    );
}

/// S1（D30 r1）的装配判据：**带加载器版本列表**的选项区在三栏档与列表共享高度。
///
/// 实测背景（1180×760）：选项区从 204px 长到 372px 时，版本列表被压成 2.4 行（110px），
/// 底缘切在第三行图标上；改成分高之后列表回到 4.6 行、面板自己可滚。
///
/// M227 起 chips 档单独一支（`Narrow`）：同样的「先保预算」思路，但钉死区只留实例名，
/// 因为窄档 459px 装不下「钉住菜单 + 3 整行列表」（算术见 `rows/tail.rs::CHIPS_PANEL_BUDGET`）。
///
/// 抓得住：有人把 arm 2 删掉（`TailShare::of` 只判 chips 档）——三栏档列表又会掉回 2 行；
/// 或者把它放宽成「任何选项区都共享」（原版没有版本列表时不该共享，那会把 6 行列表砍成 4 行）。
#[test]
fn tall_options_panel_shares_height_at_every_tier() {
    use crate::pages::download::install::PanelState;
    use crate::pages::download::rows::TailShare;
    use crate::pages::layout::VersionRailTier;

    let version_row = row("1.12.2", VersionKind::Release);
    let with_versions = state_with(
        LoaderKind::LiteLoader,
        "1.12.2",
        Ok(vec!["1.12.2-SNAPSHOT"]),
    );
    let tall = OptionsView::of(
        &version_row,
        &with_versions,
        &draft(Loader::LiteLoader),
        Lang::En.catalog(),
    );
    assert!(tall.has_loader_versions(), "LiteLoader 有版本列表");
    for tier in [
        VersionRailTier::Wide,
        VersionRailTier::Narrow,
        VersionRailTier::Compact,
        VersionRailTier::Chips,
    ] {
        let expected = if tier == VersionRailTier::Chips {
            TailShare::Narrow
        } else {
            TailShare::Shared
        };
        assert_eq!(
            TailShare::of(tier, &PanelState::Options(&tall)),
            expected,
            "{tier:?}：带版本列表的选项区必须与列表共享高度"
        );
    }

    // 原版没有版本列表 ⇒ 仍然钉死（三栏档不该无谓共享）。
    let plain_state = AppState::new();
    let plain = OptionsView::of(
        &version_row,
        &plain_state,
        &draft(Loader::Vanilla),
        Lang::En.catalog(),
    );
    assert!(!plain.has_loader_versions());
    for tier in [VersionRailTier::Wide, VersionRailTier::Compact] {
        assert_eq!(
            TailShare::of(tier, &PanelState::Options(&plain)),
            TailShare::Pinned,
            "{tier:?}：原版选项区没有版本列表，钉死"
        );
    }

    // 不兼容（有 problem）时列表不画 ⇒ 也不该共享。
    let blocked_state = state_with(LoaderKind::LiteLoader, "1.13", Ok(Vec::new()));
    let blocked = OptionsView::of(
        &row("1.13", VersionKind::Release),
        &blocked_state,
        &draft(Loader::LiteLoader),
        Lang::En.catalog(),
    );
    assert!(!blocked.has_loader_versions(), "不兼容时没有版本列表");
}
