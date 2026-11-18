//! `tabs` 的单元测试（split 拆出来的：单文件 ≤400 行，AGENTS.md §3-1）。

use super::*;

use unlucky_design::color::SchemeFlavor;
use unlucky_design::typography::Platform;

use crate::theme::Families;

use super::super::super::interaction::{self, Base};

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

/// 6.5：选中色 Primary 用 `Primary`，Secondary 用 `OnSurface`（**不是** Primary）。
#[test]
fn active_colors_follow_the_variant() {
    let ui = ui();
    assert_eq!(
        content_color(&ui, Base::CANVAS, true, Interaction::Enabled, true),
        ui.color(Role::Primary)
    );
    assert_eq!(
        content_color(&ui, Base::CANVAS, false, Interaction::Enabled, true),
        ui.color(Role::OnSurface),
        "6.5：Secondary 的选中项是 OnSurface"
    );
    assert_ne!(
        content_color(&ui, Base::CANVAS, false, Interaction::Enabled, true),
        ui.color(Role::Primary),
        "把 Secondary 的选中色写成 Primary 是最常见的串线"
    );
}

/// 未选中静息 `OnSurfaceVariant`；hover / focus / pressed 提到 `OnSurface`。
#[test]
fn inactive_states_lift_to_on_surface() {
    let ui = ui();
    for primary in [true, false] {
        assert_eq!(
            content_color(&ui, Base::CANVAS, primary, Interaction::Enabled, false),
            ui.color(Role::OnSurfaceVariant)
        );
        for state in [
            Interaction::Hovered,
            Interaction::Focused,
            Interaction::Pressed,
        ] {
            assert_eq!(
                content_color(&ui, Base::CANVAS, primary, state, false),
                ui.color(Role::OnSurface),
                "{state:?} 必须提到 OnSurface"
            );
        }
    }
}

/// 6.2：Primary 只有选中项有 3px `Primary` 条；禁用项降档。
#[test]
fn primary_bar_only_on_the_selected_tab() {
    let ui = ui();
    assert_eq!(
        bar_color(&ui, Base::CANVAS, Interaction::Enabled, false),
        None
    );
    assert_eq!(
        bar_color(&ui, Base::CANVAS, Interaction::Enabled, true),
        Some(ui.color(Role::Primary))
    );
    assert_eq!(
        bar_color(&ui, Base::CANVAS, Interaction::Disabled, true),
        Some(interaction::disabled_content_over_role(
            &ui,
            Base::CANVAS,
            Role::OnSurface,
            false
        )),
        "禁用项「点不动却亮着」是错的"
    );
}

/// 6.6：Secondary 的分隔线整行一条，颜色 `SurfaceVariant`（不是 §22 的 `OutlineVariant`）。
#[test]
fn secondary_line_is_surface_variant() {
    let ui = ui();
    assert_eq!(secondary_line_color(&ui), ui.color(Role::SurfaceVariant));
    assert_ne!(
        secondary_line_color(&ui),
        ui.color(Role::OutlineVariant),
        "OutlineVariant 是 §22 分隔线的角色，Tabs 不用它"
    );
}

/// 禁用是**修饰器**：只清消息，label / 选中态 / 图标全部保留。
#[test]
fn disabled_only_clears_the_message() {
    let tab = Tab::new("Mods", ())
        .icon(unlucky_design::icons::IconKind::Settings)
        .selected()
        .disabled();
    assert!(tab.on_press.is_none());
    assert!(tab.icon.is_some());
    assert!(tab.selected);
    assert_eq!(tab.label, "Mods");
}

/// 指示条宽度 = tab 宽 = 文本宽 + 2×16（这条是「跟随选中项宽度」的公式面）。
#[test]
fn indicator_width_is_the_tab_width() {
    let geometry = TabsGeometry::of(&ui());
    let tab_width = |text_width: f32| text_width + geometry.pad * 2.0;
    assert_eq!(tab_width(0.0), 32.0, "空文本 = 2×16");
    // 两段不同长度的文案 ⇒ 指示条宽度必然不同（渲染层断言用它打红）
    let short = tab_width(40.0);
    let long = tab_width(120.0);
    assert_eq!(long - short, 80.0);
    assert!(long > short);
}

/// 6.1：带图标的 Tabs 容器高 64（`IconAndLabelTextContainerHeight`），纯文字 48。
#[test]
fn icon_tabs_use_the_taller_container() {
    let ui = ui();
    assert_eq!(TABS_ICON_HEIGHT, 64.0);
    assert!(TABS_ICON_HEIGHT > TabsGeometry::of(&ui).height);
}

/// **交给 `button` 的那个样式闭包必须吃 `status`**（M299）。
///
/// 这条用例调的是 [`tab_style`] 返回的**闭包本身**，不是它里面的助手——M299 r1 复审实测过：
/// 只测助手的那一版，把「闭包外先算 fill、闭包写回 `|_theme, _status|`」放回来仍然全绿，
/// 于是本次在 tabs 上唯一的代码修复等于没有判据（这就是 P2-1）。
///
/// 抓得住：①闭包里把 status 丢掉（`forced.unwrap_or(Enabled)` / 用 `Status::Active` 先算好）⇒
/// hover / pressed 都等于静息，前三条红；②hover 与 pressed 用同一个 α ⇒ 第四条红；③`forced`
/// 不再优先（取证图会跟着指针漂）⇒ 后两条红。
/// 抓不住：像素。gallery 的 tabs 五态行拍的是 **`forced` 支**（矩阵每格 `.state(..)`），
/// 它与本函数同源但不是 `status` 支；`status` 支的真像素要真实指针（取证通道开了指针穿透，
/// 本机无桌面会话 ⇒ 拍不到）。报告里把这条写成「图证明的是哪一层」。
#[test]
fn the_style_closure_consumes_the_live_status() {
    let ui = ui();
    let theme = ui.theme();
    // 这里拿到的是**闭包**（不是颜色）：`tab_cell` 交给 `button` 的就是它。
    let fill =
        |forced, status| match tab_style(&ui, Base::CANVAS, forced, ui.color(Role::OnSurface))(
            &theme, status,
        )
        .background
        {
            Some(Background::Color(color)) => Some(color),
            Some(other) => panic!("背景必须是实色，得到 {other:?}"),
            None => None,
        };

    let rest = fill(None, button::Status::Active);
    let hover = fill(None, button::Status::Hovered);
    let pressed = fill(None, button::Status::Pressed);

    assert_eq!(rest, None, "静息：tab 容器透明（状态层不叠）");
    assert!(hover.is_some(), "hover 必须叠状态层（此前这里恒为 None）");
    assert!(
        pressed.is_some(),
        "pressed 必须叠状态层（此前这里恒为 None）"
    );
    assert_ne!(hover, pressed, "8% 与 10% 必须能区分");
    assert_eq!(
        hover,
        interaction::overlay_color(
            &ui,
            Interaction::Hovered,
            Role::OnSurface,
            Base::CANVAS.role()
        ),
        "叠的那一层必须逐位等于令牌求值"
    );

    // forced（取证 / 矩阵）**优先**：它不许被真实 status 盖掉
    assert_eq!(
        fill(Some(Interaction::Enabled), button::Status::Hovered),
        rest,
        "forced=静息时必须压掉观测到的 hover"
    );
    assert_eq!(
        fill(Some(Interaction::Pressed), button::Status::Active),
        pressed,
        "forced=pressed 时必须画出 pressed"
    );

    // 文字色是**静息内容色**（A1 的内容提档没做，见 findings），与 status 无关
    let ink = ui.color(Role::OnSurface);
    for status in [
        button::Status::Active,
        button::Status::Hovered,
        button::Status::Pressed,
    ] {
        assert_eq!(
            tab_style(&ui, Base::CANVAS, None, ink)(&theme, status).text_color,
            ink,
            "text_color 必须原样透传（{status:?}）"
        );
    }
}

/// 3 个 tab 的树能建出来（含选中 / 禁用 / 带图标三条路径）。
#[test]
fn view_builds_a_tree() {
    let ui = ui();
    let _: Element<'_, ()> = tabs(
        &ui,
        Tabs::primary()
            .tab(Tab::new("General", ()).selected())
            .tab(Tab::new("Mods", ()).icon(unlucky_design::icons::IconKind::Settings))
            .tab(Tab::new("Logs", ()).disabled()),
    );
    let _: Element<'_, ()> = tabs(
        &ui,
        Tabs::secondary()
            .tab(Tab::new("Release", ()).selected())
            .tab(Tab::new("Snapshot", ())),
    );
}
