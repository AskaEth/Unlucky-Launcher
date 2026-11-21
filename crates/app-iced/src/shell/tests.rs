//! `shell` 的断言（M100 从 `shell/mod.rs` 搬出来：那边在 HEAD 上已经 **399 行**，
//! 加一个 `ForcedState::Rest` 分支就必破 400 行硬线）。
//!
//! 搬移**不改一行断言**：只去掉了 `mod tests { }` 那一层缩进，外加 M100 新增的两条
//! 取证不变量（见文件末尾）。
//!
//! 站位：本文件是 `crate::shell::tests`，`use super::*;` 就够。
use super::*;
use unlucky_design::color::SchemeFlavor;
use unlucky_design::component_map::Component;
use unlucky_design::typography::Platform;

use crate::theme::Families;

fn ui(flavor: SchemeFlavor) -> Ui {
    Ui::with_platform(
        flavor,
        Platform::Windows,
        Families {
            text: "Segoe UI",
            display: "Segoe UI",
            mono: "Consolas",
        },
    )
}

#[test]
fn a_screen_only_uses_four_text_sizes_at_most() {
    let ui = ui(SchemeFlavor::Dark);
    let card = Component::OutlinedCard.spec();
    let sizes = [
        ui.emphasized(TypeRole::TitleLarge).size_px,
        ui.emphasized(TypeRole::LabelLarge).size_px,
        ui.emphasized(card.text).size_px,
        ui.style(card.support.unwrap_or(card.text)).size_px,
        ui.style(TypeRole::LabelMedium).size_px,
    ];
    let mut unique = sizes.to_vec();
    unique.sort_by(f32::total_cmp);
    unique.dedup();
    assert!(
        unique.len() <= 4,
        "rubric D4：一屏字号档 ≤4（实际 {} 档 {unique:?}）",
        unique.len()
    );
    assert!(unique[0] >= 12.0, "最小档不下 12px（caption ≥11）");
    assert!(unique[unique.len() - 1] <= 28.0, "列表页标题不得 >28px");
}

#[test]
fn flavor_slug_is_stable_for_filenames() {
    assert_eq!(crate::shot::flavor_slug(&ui(SchemeFlavor::Light)), "light");
    assert_eq!(crate::shot::flavor_slug(&ui(SchemeFlavor::Dark)), "dark");
}

/// **交互模式不许被取证开关污染**（M100 的头号不变量）。
///
/// 抓得住：`overridden` 的 `None` 分支被改坏——例如把「跟控件自己的状态走」写成了固定
/// `Active`/`Hovered`（那会让**用户窗口里所有按钮**不再反映 hover/pressed，是产品事故）。
/// 抓不住：**默认值被顺手设成 `Some(Rest)`** 这类错误——`overridden(None, …)` 根本不会被调用，
/// 本条照样绿；那一条由 `cli::tests::screenshots_default_to_the_rest_state` 抓（它断言
/// 交互模式 `force_state == None`）。两条各守一半，别拿一条当另一条的证据。
#[test]
fn interactive_mode_still_hovers() {
    use iced::widget::button::Status;

    for status in [Status::Hovered, Status::Pressed, Status::Active] {
        assert_eq!(
            overridden(None, status),
            status,
            "交互模式（None）必须跟控件自己的状态走，不许替用户钉死"
        );
    }
}

/// **取证图的常态钉死**：`Rest` 把控件压成 `Active`，不管它自己的运行期状态是什么。
///
/// 抓得住：谁把 `Rest` 接成「保留现状」（那等于没钉：系统指针 hover 照样漏进图——M99 复审实测
/// 连跑 8 次翻盘 1 次就是这里没钉住）。
/// 抓不住：① 不经过 `overridden` 的控件（标题栏以外的裸 `iced::button`，例如设置页「外观」区）；
/// ② 窗口到底还收不收鼠标事件（那是 harness 的 passthrough，看 `[caps] passthrough_ext=`）；
/// ③ **`Rest` 与「真 Disabled」撞语义**：`components::buttons::assemble()` 把「无 `on_press`」的
/// 按钮默认画成 Disabled，而 `Rest => Enabled` 会把它画成可用。当前零影响（8 处 `forced_state(`
/// 调用点与 shell 的 caption / nav 全带消息），但将来若有人加一个**无消息**的按钮又被 `Rest` 罩住，
/// 那张取证图会画出「一个本应禁用的按钮」而这条断言照绿。
#[test]
fn rest_suppresses_cursor_hover() {
    use iced::widget::button::Status;

    for status in [Status::Hovered, Status::Pressed, Status::Active] {
        assert_eq!(
            overridden(Some(ForcedState::Rest), status),
            Status::Active,
            "Rest 必须压掉 {status:?}"
        );
    }
    // 明细态照旧（D30 的状态矩阵图靠它们）
    assert_eq!(
        overridden(Some(ForcedState::Hover), Status::Active),
        Status::Hovered
    );
    assert_eq!(
        overridden(Some(ForcedState::Pressed), Status::Active),
        Status::Pressed
    );
}

/// M86：标题栏 caption 图标缩到 20 后，**容器一个数都没动**（46×48），图标也还在框里。
#[test]
fn caption_icon_shrinks_without_touching_the_container() {
    let ui = ui(SchemeFlavor::Light);
    let metrics = ui.metrics();
    let caption = icons::CAPTION_PX;
    assert_eq!(metrics.titlebar_button_width, 46.0, "容器宽不许跟着图标改");
    assert_eq!(metrics.titlebar_height, 48.0, "容器高不许跟着图标改");
    assert!(
        caption < metrics.titlebar_button_width,
        "图标 {caption} 必须小于按钮宽 {}",
        metrics.titlebar_button_width
    );
    assert!(
        caption < metrics.titlebar_height,
        "图标 {caption} 必须小于按钮高 {}",
        metrics.titlebar_height
    );
    // 官方口径 24 仍在令牌里（`IconSize::Toolbar`），偏离只发生在 caption 这一个入口。
    assert!(
        caption < unlucky_design::icons::IconSize::Toolbar.px(),
        "caption 图标必须小于官方 24（缩小是用户点名的）"
    );
}

/// **M379 判据**：标题栏标记的边长来自 `Component::TopAppBar.metrics()`（令牌），落在接受区间
/// 18~22，且正好等于标题排版（`LabelLarge` emphasize）的行高 —— 基线与中轴对齐因此是**结构性**
/// 的，不靠壳里再补一个偏移量。
///
/// 抓得住：①有人把边长改回壳里的裸字面量（那就拿不到这条断言，因为值只能从令牌来读）；
/// ②改到 18 以下 / 22 以上（区间外，肉眼就压不住或盖过标题）；③把 `gap` / `pad_start` 拍成
/// 奇数值（与 4px 栅格脱钩）。
/// 抓不住：真画出来好不好看、缩下来糊不糊 —— 那要出图，见 `DevDocs/artifacts/titleicon/`。
#[test]
fn title_logo_size_comes_from_the_top_bar_token() {
    let ui = ui(SchemeFlavor::Light);
    let bar = Component::TopAppBar
        .metrics()
        .expect("M379：TopAppBar 必须带 ComponentMetrics");

    assert!(
        (18.0..=22.0).contains(&bar.icon),
        "标记边长 {} 不在接受区间 18~22",
        bar.icon
    );
    assert_eq!(
        bar.icon,
        ui.emphasized(TypeRole::LabelLarge).line_height_px,
        "标记边长必须等于标题的行高（基线对齐的结构性保证）"
    );
    assert!(
        bar.icon < ui.metrics().titlebar_height,
        "标记不许顶满标题栏高 {}",
        ui.metrics().titlebar_height
    );
    assert_eq!(
        bar.gap % unlucky_design::spacing::GRID,
        0.0,
        "间距在 4px 栅格上"
    );
}

/// **M379 结构判据**：标记必须画在**已有拖动热区的里层**，且全文件只出现一次。
///
/// 标题栏是拖动面。`iced_widget` 的 `image` 不实现 `on_event`（对鼠标事件 `Ignored`），所以
/// 「放进 `mouse_area(..).on_press(DragWindow)` 内部」是点图标也能拖窗的前提。
///
/// 抓得住：谁把 `title_icon::view()` 提到 `mouse_area` 外面当第二个兄弟（那一块就成了点击死区），
/// 或者又塞了第二个标记进去。
/// 抓不住：真拖不拖得动（要真开窗手测；`DevDocs/artifacts/titleicon/README.md` 记了手测清单）。
#[test]
fn the_title_logo_lives_inside_the_drag_area() {
    let source = include_str!("mod.rs");

    assert_eq!(
        source.matches("title_icon::view()").count(),
        1,
        "标题栏里只许有一处标记"
    );

    let start = source
        .find("let drag_area = mouse_area(")
        .expect("拖动热区的构造点在 shell/mod.rs 里消失了");
    let rest = &source[start..];
    let end = rest
        .find(".on_press(Message::DragWindow)")
        .expect("拖动热区必须仍然接 `Message::DragWindow`");
    let drag = &rest[..end];

    assert!(
        drag.contains("title_icon::view()"),
        "标记必须在 `mouse_area(..).on_press(DragWindow)` 的里层，否则点图标拖不动窗"
    );
    assert!(
        drag.contains("label("),
        "标题文字必须与标记同在那个拖动热区里"
    );
}

/// **M283 几何判据**：状态层（hover / pressed）画的矩形 == 药丸 [`nav::indicator_size`]（56×32），
/// **不是**按钮的轨宽 × 项高（88×52）。
///
/// 抓得住：谁把状态层改回「铺满整条轨宽」（旧的 `button(stack).style(..)` 铺底写法）——那条路
/// 下这块矩形就是 88×52，两个 `assert_ne` 立刻红。
/// 抓不住：真画出来的像素（那要出图，见 `DevDocs/artifacts/navpill/`）；也抓不住颜色对不对
/// （那归 `interaction` 的 `layer_color` 单测）。
#[test]
fn the_state_layer_is_drawn_on_the_pill_not_across_the_rail() {
    use unlucky_design::shape::ShapeScale;

    use super::nav::{indicator_size, nav_item_height, state_layer_box};

    let ui = ui(SchemeFlavor::Light);
    let metrics = ui.metrics();
    let pill = state_layer_box(metrics);

    assert_eq!(
        pill,
        indicator_size(metrics),
        "状态层与选中指示器必须是同一块矩形"
    );
    assert_eq!(pill, (56.0, 32.0), "12b 5.4 的指示器尺寸");
    assert_ne!(
        pill.0, metrics.nav_rail_width,
        "状态层不许铺满轨宽（{}）——那正是用户点名的方形加深",
        metrics.nav_rail_width
    );
    assert_ne!(
        pill,
        (metrics.nav_rail_width, nav_item_height(&ui, pill.1)),
        "状态层也不许铺满整个按钮（轨宽 × 项高 = 88×52）"
    );

    let radius = ui.radius(ShapeScale::Full, pill.1);
    assert_eq!(
        (radius.top_left, radius.bottom_right),
        (pill.1 / 2.0, pill.1 / 2.0),
        "Full ⇒ r=16：药丸四角必须是空的，不能有 hover 色"
    );
}

/// **M283 结构判据**：状态层不许回到按钮上（扫源码，不渲染）。
///
/// `button(stack).style(..)` 的闭包里出现 `Background::Color` 就是「同一个元素并存两种形状」
/// 回来了——选中画 56×32 圆角药丸、hover 却铺 88×52 直角底（用户原话：父容器整个方形区域加深）。
///
/// 抓得住：①有人把 `button::Status` 分支接回按钮的 `style`；②`interaction::tracked` 被套到
/// 强制态上（取证图会出现过渡中间帧）；③药丸尺寸改回按钮尺寸（`state_layer_box` 的调用点消失）。
/// 抓不住：颜色对不对、真画出来是圆的还是方的——前者归 `layer_color` 的单测，后者归出图。
#[test]
fn the_rail_never_paints_its_state_layer_on_the_button() {
    let sources = [
        ("shell/nav.rs", include_str!("nav.rs")),
        ("shell/nav/account.rs", include_str!("nav/account.rs")),
    ];

    for (name, source) in sources {
        let mut buttons = 0;
        for (start, _) in source.match_indices("button(stack)") {
            let rest = &source[start..];
            let end = rest
                .find(".on_press(")
                .unwrap_or_else(|| panic!("{name}：`button(stack)` 之后找不到 `.on_press(`"));
            let builder = &rest[..end];
            assert!(
                !builder.contains("Background::Color"),
                "{name}：按钮的 style 闭包里又铺底了 —— 状态层必须落在药丸/头像上"
            );
            assert!(
                builder.contains("background: None"),
                "{name}：按钮的 `background` 必须恒为 `None`"
            );
            buttons += 1;
        }
        assert_eq!(buttons, 1, "{name}：期望正好一处 `button(stack)`");

        // `tracked` 只许在 `forced.is_none()` 时套（强制态 = 取证，不许有过渡中间帧）
        let lines = source.lines().collect::<Vec<_>>();
        let mut tracked = 0;
        for (index, line) in lines.iter().enumerate() {
            if !line.contains("interaction::tracked(") {
                continue;
            }
            tracked += 1;
            let guard = index.saturating_sub(2);
            assert!(
                lines[guard..index]
                    .iter()
                    .any(|line| line.contains("forced.is_none()")),
                "{name}:{}：`tracked` 必须只在 `forced.is_none()` 时套（取证图不许有过渡）",
                index + 1
            );
        }
        assert_eq!(tracked, 1, "{name}：期望正好一处 `interaction::tracked(`");
    }

    // 另一半：药丸/头像那一侧必须真的经 `layer_color` 求值，且药丸尺寸取自几何定义点
    let nav = include_str!("nav.rs");
    let account = include_str!("nav/account.rs");
    assert!(
        nav.contains("pill_color(ui, state, selected)"),
        "nav.rs：药丸底色必须经 pill_color 求值"
    );
    assert!(
        account.contains("avatar_color(ui, state,"),
        "account.rs：头像底色必须经 avatar_color 求值"
    );
    for (name, source) in [("nav.rs", nav), ("account.rs", account)] {
        assert!(
            source.contains("interaction::layer_color"),
            "{name}：状态层必须经 layer_color 求值（没有取样控件时逐位退回 overlay_color）"
        );
    }
    assert!(
        nav.contains("state_layer_box(ui.metrics())"),
        "nav.rs：药丸尺寸必须取自 `state_layer_box`（几何判据钉的就是它）"
    );
}
