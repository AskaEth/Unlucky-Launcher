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
