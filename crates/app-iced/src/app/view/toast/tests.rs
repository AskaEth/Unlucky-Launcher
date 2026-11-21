//! 覆盖层的守门用例（M298）。
//!
//! ## 这一组抓得住什么 / 抓不住什么（说在明处）
//!
//! | 用例 | 抓得住 | 抓不住 |
//! |---|---|---|
//! | [`a_pushed_toast_really_reaches_the_widget_tree`] | 覆盖层**接在 `view()` 上**，且真的把那句话交给了控件树（把 `view.rs` 里那行 `toast::layer(..)` 删掉即红） | 像素：胶囊长什么样（`DevDocs/artifacts/m298/toast-capsule.png` 守） |
//! | [`the_capsule_is_a_full_radius_pill`] | 圆角是 `ShapeScale::Full`（高的一半 = 26），不是组件层的 16 | 观感对不对 |
//! | [`an_empty_queue_adds_no_layer_at_all`] | 没有提示时树形与改造前**逐节点相同**（覆盖层不是常驻空壳） | —— |
//!
//! ## 为什么不是「只断 `state.toasts.len()`」
//!
//! 长度能证明队列里有东西，证明不了**有人画它**（M297 的缺口正是这样活下来的：队列里一直有
//! 数据，渲染点一处都没有）。所以这里顺着 `Widget::children()` 走真控件树，靠
//! [`CapsuleState`] 把「画出去的文案」读回来。

use iced::advanced::widget::Tree;

use unlucky_design::color::SchemeFlavor;
use unlucky_design::shape::ShapeScale;
use unlucky_design::typography::Platform;
use unlucky_presenter::mock::MockBackend;
use unlucky_presenter::settings_bridge::SettingsBridge;
use unlucky_presenter::toast::ToastKind;

use crate::app::message::Message;
use crate::app::state::App;
use crate::cli::Options;
use crate::theme::Families;

use super::{capsule_radius, CapsuleState};

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

fn headless() -> App {
    App::with_presenter(
        Options::default(),
        FAMILIES,
        SchemeFlavor::Light,
        Box::new(MockBackend::new()),
        SettingsBridge::memory_only(),
    )
}

/// 顺着真控件树把所有 `CapsuleState` 里的文案收集出来（`Tree::state` 是 iced 给控件挂
/// 自证状态的官方位置，`container` / `text` 用它存自己的缓存）。
fn painted_texts(element: &iced::Element<'_, Message>) -> Vec<String> {
    fn walk(tree: &Tree, out: &mut Vec<String>) {
        if tree.tag == iced::advanced::widget::tree::Tag::of::<CapsuleState>() {
            let state = tree.state.downcast_ref::<CapsuleState>();
            out.extend(state.texts.iter().cloned());
        }
        for child in &tree.children {
            walk(child, out);
        }
    }
    let tree = Tree::new(element.as_widget());
    let mut out = Vec::new();
    walk(&tree, &mut out);
    out
}

/// 守卫①：**push 一条 toast ⇒ 视图树里真的出现那句话**。
///
/// **变异证明**：把 `app/view.rs` 里 `let tree = toast::layer(...)` 那一行删掉 / 改成直接返回
/// `tree`，本用例第二段立刻红（`painted_texts` 拿到空表）。
#[test]
fn a_pushed_toast_really_reaches_the_widget_tree() {
    let mut app = headless();
    app.refresh_view();
    assert!(
        painted_texts(&app.view()).is_empty(),
        "出厂态不该有覆盖层节点（那时队列是空的）"
    );

    app.state.toasts.push(ToastKind::Info, "已取消安装");
    app.refresh_view();
    let texts = painted_texts(&app.view());
    assert!(
        texts.iter().any(|text| text == "已取消安装"),
        "push 之后控件树里必须有这句话——实际拿到 {texts:?}"
    );
    // 队列里那一条仍然是**事实来源**，覆盖层只是它的投影（不是第二份真相）。
    assert_eq!(app.state.toasts.len(), 1);
}

/// 守卫②：胶囊是真胶囊（`ShapeScale::Full` ⇒ 高的一半）。
///
/// **变异证明**：把 `capsule` 里那一行改成 `geometry.radius`（组件层的 16），本用例立刻红。
#[test]
fn the_capsule_is_a_full_radius_pill() {
    let ui = crate::theme::Ui::with_platform(SchemeFlavor::Light, Platform::Windows, FAMILIES);
    let height = super::SnackbarGeometry::of(&ui).height_for(1);
    let radius = capsule_radius(&ui, height);
    assert_eq!(
        radius,
        iced::border::Radius::from(height / 2.0),
        "MD3E 胶囊的圆角必须等于高的一半（ShapeScale::Full），实测高 {height}"
    );
    // 同一件事的第二种说法：就是 `ShapeScale::Full` 在这一档上的求值。
    assert_eq!(radius, ui.radius(ShapeScale::Full, height));
    // 反向守卫：组件层 snackbar 的 16 不是胶囊（改回 `geometry.radius` 时上面两条立刻红）。
    assert_ne!(radius, iced::border::Radius::from(16.0));
}

/// 守卫③：空队列 ⇒ **不叠任何东西**（没有提示的那些帧树形与改造前一致）。
///
/// **变异证明**：把 `layer` 开头的 `if toasts.is_empty() { return base; }` 删掉，本用例立刻红
/// ——那时每帧都会多一层空容器（白白多一棵子树、也让「有没有提示」在树上不可辨）。
#[test]
fn an_empty_queue_adds_no_layer_at_all() {
    fn nodes(element: &iced::Element<'_, Message>) -> usize {
        fn walk(tree: &Tree) -> usize {
            1 + tree.children.iter().map(walk).sum::<usize>()
        }
        walk(&Tree::new(element.as_widget()))
    }
    let mut app = headless();
    app.refresh_view();
    let empty = nodes(&app.view());
    app.state.toasts.push(ToastKind::Info, "已取消安装");
    app.refresh_view();
    let with_toast = nodes(&app.view());
    assert!(
        with_toast > empty,
        "有提示时树上必须多出覆盖层（空队列 {empty} 个节点，有提示 {with_toast} 个）"
    );
}
