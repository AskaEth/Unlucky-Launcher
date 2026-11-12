//! `list_item` 的单元测试（split 拆出来的：单文件 ≤400 行，AGENTS.md §3-1）。

use super::*;

use unlucky_design::color::SchemeFlavor;
use unlucky_design::typography::Platform;

use crate::theme::Families;

fn ui() -> Ui {
    Ui::with_platform(
        SchemeFlavor::Light,
        Platform::Windows,
        Families {
            text: "Segoe UI",
            display: "Segoe UI",
            mono: "Consolas",
        },
    )
}

/// 槽位决定行数：1 / 2 / 3（三行是 overline + headline + supporting）。
#[test]
fn line_count_follows_the_slots() {
    let one = ListItem::<()>::new("Instances", ());
    assert_eq!(one.lines(), 1);
    let two = ListItem::<()>::new("Instances", ()).supporting("3 versions");
    assert_eq!(two.lines(), 2);
    let three = ListItem::<()>::new("Instances", ())
        .overline("RECENT")
        .supporting("3 versions");
    assert_eq!(three.lines(), 3);
    // overline 单独存在也是两行（overline + headline）
    let overline_only = ListItem::<()>::new("Instances", ()).overline("RECENT");
    assert_eq!(overline_only.lines(), 2);
}

/// **禁用是修饰器**：只清消息，槽位与选中态全部保留（复审 P2-1 那条）。
#[test]
fn disabled_only_clears_the_message() {
    let item = ListItem::new("Instances", ())
        .leading(unlucky_design::icons::IconKind::Instances)
        .trailing(unlucky_design::icons::IconKind::Settings)
        .supporting("3 versions")
        .selected()
        .disabled();
    assert!(item.on_press.is_none(), "禁用必须清消息");
    assert!(item.leading.is_some(), "禁用不许删前置图标");
    assert!(item.trailing.is_some(), "禁用不许删后置图标");
    assert!(item.selected, "禁用不许删选中态");
    assert!(item.supporting.is_some(), "禁用不许删槽位");
    assert_eq!(item.lines(), 2);
}

/// 禁用内容角色是 **`OnSurface`**@38%（A1 ListTokens），不是默认的 `OnSurfaceVariant`。
#[test]
fn disabled_content_uses_on_surface() {
    let ui = ui();
    let palette = palette(&ui, Base::CANVAS, Interaction::Disabled, false);
    let expected =
        interaction::disabled_content_over_role(&ui, Base::CANVAS, Role::OnSurface, false);
    assert_eq!(palette.headline, expected);
    assert_eq!(palette.leading, expected);
    assert_eq!(palette.trailing, expected);
    assert_ne!(
        palette.headline,
        interaction::disabled_content_over_role(&ui, Base::CANVAS, Role::OnSurfaceVariant, false),
        "走默认的 OnSurfaceVariant 就会偏浅一档（这条钉住 A1 的角色）"
    );
    assert!(palette.fill.is_none(), "禁用不叠状态层容器");
}

/// 选中态：`SecondaryContainer` + `OnSecondaryContainer`，trailing 是 `Primary`（4.8）。
#[test]
fn selected_row_is_secondary_container() {
    let ui = ui();
    let palette = palette(&ui, Base::CANVAS, Interaction::Enabled, true);
    assert_eq!(palette.fill, Some(ui.color(Role::SecondaryContainer)));
    assert_eq!(palette.headline, ui.color(Role::OnSecondaryContainer));
    assert_eq!(palette.trailing, ui.color(Role::Primary), "4.8");
}

/// 4.7：hover / pressed 只叠状态层，**内容色不变**。
#[test]
fn interaction_states_keep_the_content_colors() {
    let ui = ui();
    let resting = palette(&ui, Base::CANVAS, Interaction::Enabled, false);
    for state in [
        Interaction::Hovered,
        Interaction::Focused,
        Interaction::Pressed,
    ] {
        let active = palette(&ui, Base::CANVAS, state, false);
        assert_eq!(active.headline, resting.headline, "{state:?} 不许改文本色");
        assert_eq!(active.leading, resting.leading, "{state:?} 不许改图标色");
        assert!(active.fill.is_some(), "{state:?} 必须叠状态层");
    }
    assert_eq!(resting.fill, None, "静息态不叠状态层");
}

/// 三档行高与 `LM.row_height` 的关系**钉死**（有人按 A1 改回 56 时这里会红）。
#[test]
fn heights_are_pinned_to_the_row_height_token() {
    let ui = ui();
    let geometry = ListItemGeometry::of(&ui);
    assert_eq!(geometry.heights[0], ui.metrics().row_height);
    assert_eq!(geometry.heights, [44.0, 60.0, 76.0]);
    assert_ne!(
        geometry.heights[0], 56.0,
        "56 是 A1 的值；本项目按 12b §4.1 取 44（改回去会破坏页面排版）"
    );
}

/// 元素构造不 panic（`Element` 树的形状也一并过一遍）。
#[test]
fn view_builds_a_tree() {
    let ui = ui();
    let _: Element<'_, ()> = list_item(
        &ui,
        ListItem::new("Instances", ())
            .overline("RECENT")
            .supporting("3 versions")
            .leading(unlucky_design::icons::IconKind::Instances)
            .trailing(unlucky_design::icons::IconKind::Settings)
            .selected(),
    );
    let _: Element<'_, ()> = list_item(&ui, ListItem::new("Logs", ()).disabled());
}
