//! `shell/nav.rs` 的单测（M299 从 `nav.rs` 纯搬移，守 400 行硬线）。
//!
//! 判据与注释一字未动，只是**换文件**：`nav.rs` 加了一条「选中药丸跟态走」之后到 413 行。
//! 模块路径仍是 `shell::nav::tests`（`nav.rs` 里改成 `mod tests;`），`super::*` 拿到的还是
//! `shell::nav`。

use super::*;
use unlucky_design::color::SchemeFlavor;
use unlucky_design::spacing::LayoutMetrics;
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

#[test]
fn indicator_matches_the_reference_56x32() {
    let (width, height) = indicator_size(LayoutMetrics::DESKTOP);
    assert_eq!((width, height), (56.0, 32.0), "12b 5.4 的参考值");
}

/// **选中药丸也跟态走**，且走的是全仓唯一的那个入口（M299）。
///
/// 抓得住：①把选中支改回「选中就不叠层」（`Some(ui.color(SecondaryContainer))`）⇒ hover /
/// pressed 都等于静息，前两条红；②把 hover 的 α 抄成 pressed 的 10% ⇒ `hover != pressed` 红；
/// ③把选中支的合成底写成画布色（导航轨自己画的是 `MaterialLayer::Canvas`，但选中项的
/// 状态层要叠在 `SecondaryContainer` 上）⇒ 后两条逐值断言红。
/// 抓不住：像素（`DevDocs/artifacts/m299/shell-*-hover|pressed.png`）与**几何**（56×32 由
/// `state_layer_box` / `indicator_size` 那两条钉住，本次一个像素都没动）；也抓不住「这一处
/// 自己抄了一份**正确**的叠层」——那是 `selection/tests/single_entry.rs` 的接线扫描管的事。
#[test]
fn the_selected_pill_reacts_to_hover_and_press() {
    use unlucky_design::color::StateLayer;

    let ui = ui();
    let fill = |state| pill_color(&ui, state, true).expect("选中项静息也有底");
    let rest = fill(Interaction::Enabled);
    let hover = fill(Interaction::Hovered);
    let pressed = fill(Interaction::Pressed);

    assert_eq!(
        rest,
        ui.color(Role::SecondaryContainer),
        "选中静息 = 实色容器"
    );
    assert_ne!(hover, rest, "选中 + hover 必须与静息不同");
    assert_ne!(pressed, rest, "选中 + pressed 必须与静息不同");
    assert_ne!(hover, pressed, "8% 与 10% 必须能区分");
    assert_eq!(
        hover,
        ui.state_over(
            StateLayer::Hover,
            Role::OnSecondaryContainer,
            Role::SecondaryContainer
        ),
        "叠的那一层必须逐位等于令牌求值（不是本地算术）"
    );
    assert_eq!(
        pressed,
        ui.state_over(
            StateLayer::Pressed,
            Role::OnSecondaryContainer,
            Role::SecondaryContainer
        )
    );
    // 未选中支**没变**：静息不画药丸，三态仍是各自的状态层
    assert_eq!(pill_color(&ui, Interaction::Enabled, false), None);
    assert!(pill_color(&ui, Interaction::Hovered, false).is_some());
}

#[test]
fn nav_item_height_is_indicator_plus_gap_plus_label() {
    let ui = ui();
    let (_, indicator_height) = indicator_size(ui.metrics());
    // 32 + 4 + 16
    assert_eq!(nav_item_height(&ui, indicator_height), 52.0);
}

#[test]
fn every_nav_item_has_a_label_and_an_icon() {
    assert!((4..=6).contains(&NavItem::ALL.len()));
    for item in NavItem::ALL {
        assert!(!item.label(Lang::En).is_empty(), "{item:?} 没有文案");
        assert!(!item.icon().icon().path.is_empty(), "{item:?} 没有图标");
    }
}

/// **M108**：导航栏上这一项叫 `Versions`（88px 轨放不下全名 `Version management`）。
///
/// 文案本身走目录（M106 的 `label_key()` + `label(lang)`），这条只守**取值**与**宽度余量**：
///
/// - **抓得住**：① 有人把 `Key::NavDownload` 的英文改回「下载」或塞回 18 字符的全名
///   （88px 轨上会截断/换行，实测全名 ≈101px）；② 中文被「为了对称」砍成两个字。
/// - **抓不住**：真画出来的截断（那要出图，见 M108 报告里的产品通道与 pages_demo 图）；
///   也抓不住 240 展开抽屉（那一档在产品里还不可达）。
#[test]
fn the_download_item_is_labelled_version_management() {
    // 88px 轨的余量：中文 4 字 ≈48px、英文短名 ≈45px，都该有明显的余量
    assert_eq!(NavItem::Download.label(Lang::En), "Versions");
    assert_eq!(NavItem::Download.label(Lang::ZhCn), "版本管理");
    for item in NavItem::ALL {
        let label = item.label(Lang::En);
        assert!(
            !label.to_ascii_lowercase().contains("download"),
            "{item:?} 的英文标签还在说下载：{label}"
        );
        assert!(
            label.chars().count() <= 12,
            "{item:?} 的标签太长，88px 的轨放不下：{label}"
        );
    }
}

// ---------------------------------------------------------------- M300 折叠档

use iced::advanced::widget::tree::Tag;
use iced::advanced::widget::Tree;
use iced::widget::tooltip::Position;
use iced::widget::{text, tooltip};

use super::{nav_width, rail_item, ItemView};

/// 一个元素在控件树里的标签，以及「这棵树里有几个文字节点 / 几个 tooltip 子树」。
///
/// 为什么要走**真控件树**而不是断言 `ItemView::labels_visible()`：后者是判据本身，断言它等于
/// 「断言我自己写的那个 bool 是真的」。真树能抓住「折叠分支里又把 label 推回去了」，因为那时
/// 文字节点会多出来。
fn tree_of(el: &iced::Element<'_, Message>) -> Tree {
    Tree::new(el.as_widget())
}

/// 文字节点的标签（`text` 与包着它的 `container` 同 tag，见 iced 的 `container::tag` 委托）。
fn text_tag() -> Tag {
    let probe: iced::Element<'static, Message> = text("probe").into();
    tree_of(&probe).tag
}

/// tooltip 子树的标签。
fn tooltip_tag() -> Tag {
    let probe: iced::Element<'static, Message> =
        tooltip(text("probe"), text("tip"), Position::Right).into();
    tree_of(&probe).tag
}

/// 数「tooltip 子树之外」的文字节点：tooltip 自己带一个气泡文案，它在折叠态**应该**存在
/// （那是可读名），所以不能算进「标签」的口径里。
fn text_nodes_outside_tooltips(tree: &Tree, text: Tag, tip: Tag) -> usize {
    if tree.tag == tip {
        return 0;
    }
    let here = usize::from(tree.tag == text);
    here + tree
        .children
        .iter()
        .map(|child| text_nodes_outside_tooltips(child, text, tip))
        .sum::<usize>()
}

fn tooltip_count(tree: &Tree, tip: Tag) -> usize {
    let here = usize::from(tree.tag == tip);
    here + tree
        .children
        .iter()
        .map(|child| tooltip_count(child, tip))
        .sum::<usize>()
}

/// **折叠态：文字标签不进控件树，但每个图标都有可读名（tooltip）。**
///
/// 抓得住：①折叠分支里把 label 又推回去（`text_nodes_outside_tooltips` 从 0 变 1）；
/// ②折叠态只剩图标却没有可读名（`tooltip_count` 从 1 变 0 —— 那正是用户点名的
/// 「一列无名符号」）；③展开态被顺手塞上 tooltip（展开态那个计数从 0 变 1）。
/// 抓不住：tooltip 真弹出来长什么样、以及文字**内容**对不对（气泡里的文案由 `Key` 目录守，
/// 像素由 `DevDocs/artifacts/m300/` 两张图守）。
#[test]
fn the_collapsed_item_keeps_no_label_but_gives_the_icon_a_name() {
    let ui = ui();
    let item = NavItem::Instances;
    let expanded = rail_item(
        &ui,
        item,
        ItemView {
            selected: true,
            compact: false,
            collapsed: false,
            width: ui.metrics().nav_rail_width,
        },
        None,
        Lang::En,
    );
    let collapsed = rail_item(
        &ui,
        item,
        ItemView {
            selected: true,
            compact: false,
            collapsed: true,
            width: ui.metrics().nav_collapsed_width,
        },
        None,
        Lang::En,
    );

    let (text, tip) = (text_tag(), tooltip_tag());
    let expanded_tree = tree_of(&expanded);
    let collapsed_tree = tree_of(&collapsed);

    assert!(
        text_nodes_outside_tooltips(&expanded_tree, text, tip) > 0,
        "前提：展开态必须画标签（否则这条用例什么都没测到）"
    );
    assert_eq!(
        text_nodes_outside_tooltips(&collapsed_tree, text, tip),
        0,
        "折叠态不许把文字标签放进控件树"
    );
    assert_eq!(
        tooltip_count(&collapsed_tree, tip),
        1,
        "折叠态的图标必须有可读名（tooltip）"
    );
    assert_eq!(
        tooltip_count(&expanded_tree, tip),
        0,
        "展开态有可见标签，不需要 tooltip"
    );
}

/// **折叠态里每个图标仍然可辨识**：五项图标两两不同。
///
/// 抓得住：有人把某一项的图标换成同一个（或把 `icon()` 写成常量）⇒ 折叠态立刻变成「一列同样的
/// 符号」。抓不住：24px 下肉眼能否分辨（那要出图）与 tooltip 的文案（`Key` 目录守）。
#[test]
fn the_five_icons_stay_distinguishable_when_the_labels_are_gone() {
    let mut paths = Vec::new();
    for item in NavItem::ALL {
        let path = item.icon().icon().path;
        assert!(!path.is_empty(), "{item:?} 没有图标");
        assert!(
            !paths.contains(&path),
            "{item:?} 的图标与前面某项重复：折叠态会变成一列同样的符号"
        );
        paths.push(path);
    }
    assert_eq!(paths.len(), NavItem::ALL.len());
}

/// **宽度是两档之间的插值**，两端**逐位**等于令牌。
///
/// 抓得住：①把折叠档写成 48（旧值）⇒ 端点断言红，且 `spacing.rs` 的「容得下胶囊」断言也会红；
/// ②插值写成非线性的（例如再套一条缓动）⇒ 中点断言红（曲线在帧泵那一侧采过样了）；
/// ③进度不做钳位（动效中间帧越界一次就把轨画成负数宽）。
#[test]
fn the_rail_width_interpolates_between_the_two_tokens() {
    let metrics = ui().metrics();
    assert_eq!(nav_width(metrics, 1.0), metrics.nav_rail_width, "展开端");
    assert_eq!(
        nav_width(metrics, 0.0),
        metrics.nav_collapsed_width,
        "折叠端"
    );
    assert_eq!(
        nav_width(metrics, 0.5),
        (metrics.nav_rail_width + metrics.nav_collapsed_width) / 2.0,
        "中点必须是线性的"
    );
    // 钳位：越界的进度不许把轨画到两端之外
    assert_eq!(nav_width(metrics, 1.7), metrics.nav_rail_width);
    assert_eq!(nav_width(metrics, -0.3), metrics.nav_collapsed_width);
    // 折叠档必须容得下 56 宽的胶囊（两个端点各一次，含动效中间帧的单调性）
    let pill = indicator_size(metrics).0;
    assert!(pill <= nav_width(metrics, 0.0), "折叠端容不下胶囊");
    for step in 0..=10 {
        let progress = step as f32 / 10.0;
        assert!(
            nav_width(metrics, progress) >= pill,
            "进度 {progress} 时轨比胶囊还窄"
        );
    }
}
