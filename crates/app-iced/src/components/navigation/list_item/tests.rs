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

/// **选中行也吃 hover / pressed**（M296）：底色三态两两不同，内容色不变。
///
/// 抓得住：有人把选中分支写回「选中就不叠层」（三色相等 ⇒ 三条 `assert_ne!` 一起红）。
/// 抓不住：图上差几个色阶（`DevDocs/artifacts/selstate` 的裁图判据）。
#[test]
fn a_selected_row_still_reacts_to_hover_and_press() {
    let ui = ui();
    let pick = |state| palette(&ui, Base::CANVAS, state, true);
    let (rest, hover, pressed) = (
        pick(Interaction::Enabled),
        pick(Interaction::Hovered),
        pick(Interaction::Pressed),
    );
    assert_eq!(rest.fill, Some(ui.color(Role::SecondaryContainer)));
    assert_ne!(hover.fill, rest.fill, "选中 + hover 必须与静息不同");
    assert_ne!(pressed.fill, rest.fill, "选中 + pressed 必须与静息不同");
    assert_ne!(hover.fill, pressed.fill, "8% 与 10% 必须能区分");
    assert_eq!(
        hover.fill,
        Some(interaction::selected_fill(
            &ui,
            Interaction::Hovered,
            Role::OnSecondaryContainer,
            Role::SecondaryContainer
        )),
        "叠层必须经唯一入口求值（不是本地算术）"
    );
    for state in [hover, pressed] {
        assert_eq!(state.headline, rest.headline, "叠层不许改文本色");
        assert_eq!(state.trailing, rest.trailing, "叠层不许改 trailing 色");
    }
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

/// **未选中行也吃 A8 的时间轴**（M314 / P27-2：未选中支从 `overlay_color` 换到 `layer_color`）。
///
/// 两档判据：
/// 1. **端点逐位不变**：没有取样控件时与 [`interaction::overlay_color`] 逐位相等 ⇒ 静息帧与
///    到位帧的像素与改造前完全相同（`--force-state` 的取证图不受影响）；
/// 2. **过渡中间帧存在**：有取样控件时中间帧既不是端点、也不是「没有状态层」的那个底，且不透明。
///
/// 抓得住：把未选中支改回 `overlay_color`（它不看 `layer::current()`，中间帧 == 端点 ⇒
/// 第 2 档的 `assert_ne` 红）。
/// 抓不住：观感与真机像素（那要出图）；50ms 腿上的中间值差几级色阶只有图能判。
#[test]
fn an_unselected_row_fades_its_state_layer_instead_of_snapping() {
    use crate::app::components::interaction::layer;
    use unlucky_design::motion::MotionScale;

    layer::reset();
    let ui = ui();

    // ① 端点：没有取样控件（`--force-state` / gallery）时逐位等于旧口径。
    let hovered_end = interaction::overlay_color(
        &ui,
        Interaction::Hovered,
        Role::OnSurface,
        Base::CANVAS.role(),
    );
    assert_eq!(
        palette(&ui, Base::CANVAS, Interaction::Hovered, false).fill,
        hovered_end,
        "端点必须逐位等于 `overlay_color`（静息 / 到位帧的像素不许漂）"
    );

    // ② 中间帧：状态层走在两态之间。
    let mut state = layer::Layer::new();
    state.retarget(Interaction::Hovered, MotionScale::STANDARD);
    layer::advance_by(16.0);
    let mid = layer::with_current(state, || {
        palette(&ui, Base::CANVAS, Interaction::Hovered, false).fill
    })
    .expect("过渡中必须有颜色");
    assert_eq!(mid.a, 1.0, "中间帧也必须不透明（D27）");
    assert_ne!(
        Some(mid),
        hovered_end,
        "中间帧不许等于终点色 —— 相等就等于硬切（未选中支被改回 `overlay_color` 时红在这里）"
    );
    assert_ne!(
        mid,
        ui.color(Base::CANVAS.role()),
        "中间帧也不许还停在「没有状态层」的那个底上（起点那一侧）"
    );
    layer::reset();
}

/// 行的圆角**读组件表**（`Component::ListItem.spec().shape`），不是在这里另写一个常量。
///
/// 抓得住：渲染路径用的 `Border.radius` = 组件表里 `ListItem` 那一行的 `shape` 折算出的 dp；
/// 表值不是 `None`（直角），而 `Small` = 8。
/// 抓不住：有人把 `row_border` 换成一个**数值恰好相同**的字面常量（`ShapeScale::Small`）——
/// 见 `render_path_reads_the_table` 的源码结构断言，以及 M287 的变异图（改表值 ⇒ 图跟着变）。
#[test]
fn row_corner_comes_from_the_component_table() {
    let ui = ui();
    let spec = Component::ListItem.spec();
    assert_eq!(spec.shape, ShapeScale::Small, "组件表的 ListItem 档位");
    assert_eq!(spec.micro_radius, None, "ListItem 不走 D32 的微圆角层");
    assert_eq!(
        row_border(&ui),
        Border {
            radius: ui.radius_dp(spec.shape),
            ..Border::default()
        },
        "行的圆角必须就是组件表那一档折算出来的"
    );
    assert_eq!(row_border(&ui).radius.top_left, 8.0, "Small = 8dp");
    assert_ne!(
        row_border(&ui),
        Border {
            radius: ui.radius_dp(ShapeScale::None),
            ..Border::default()
        },
        "M287 之前这里是 `ShapeScale::None`（直角）：退回去会红"
    );
}

/// 源码结构（`Element` 没有内省接口，见 `pages::layout::source` 的同一条纪律）：
/// 渲染路径必须**从组件表取**圆角，文件里不许再出现硬编码的圆角档。
#[test]
fn render_path_reads_the_table() {
    let source = include_str!("../list_item.rs");
    assert!(
        source.contains("Component::ListItem.spec().shape"),
        "行的圆角必须读组件表（`Component::ListItem.spec().shape`）"
    );
    for hardcoded in [
        "ShapeScale::None",
        "ShapeScale::Small",
        "ShapeScale::ExtraSmall",
        "ShapeScale::Medium",
        "ShapeScale::Large",
    ] {
        assert!(
            !source.contains(hardcoded),
            "{hardcoded} 是硬编码圆角档：ListRow 的圆角只许从组件表取"
        );
    }
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
