//! 纯函数级断言：几何对令牌、节奏对 A1、角色对变体。
//!
//! 这里**只测数值与结构**（M68 的教训：函数对不代表图对）。对着真截图量像素的是
//! [`super::probe`]，两者缺一不可——本文件保证「数字对」，probe 保证「画出来是那个数字」。

use iced::Element;

use unlucky_design::color::{Role, Scheme, SchemeFlavor, SCRIM_ALPHA};
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::typography::Platform;

use crate::theme::{to_iced, Families, Ui};

use super::super::interaction::{self, Base, Interaction};
use super::dialog::scrim;
use super::panel::{DialogGeometry, EmptyStateGeometry, SnackbarGeometry};
use super::progress::Progress;
use super::snackbar::{Placement, Snackbar};
use super::{circular, dialog, empty_state, linear, snackbar, Dialog, EmptyState};

mod bands;
mod progress;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

pub(super) fn ui(flavor: SchemeFlavor) -> Ui {
    Ui::with_platform(flavor, Platform::Windows, FAMILIES)
}

// ---- §14 Snackbar -----------------------------------------------------------

/// 14.3 / 14.1 / 宽度上限：规格卡 52 / 72 / 16，上限 600。
#[test]
fn snackbar_heights_widths_and_radius_are_the_tokens() {
    let geometry = SnackbarGeometry::of(&ui(SchemeFlavor::Light));
    // M229 换肤：高与圆角取规格卡（52 / 16），官方是 48 / `CornerExtraSmall(4)`。
    assert_eq!(geometry.height_for(1), 52.0, "规格卡 340×52");
    assert_eq!(
        geometry.height_for(2),
        72.0,
        "双行 = 单行 + 20（同官方 delta）"
    );
    assert_eq!(geometry.radius, 16.0, "规格卡 rx=16（官方 4，登记差异）");
    assert_eq!(geometry.max_width, 600.0, "SnackbarKt.ContainerMaxWidth");
    assert_eq!(geometry.min_width, 344.0, "G17-d [估] 最小宽");
    assert!(geometry.min_width < geometry.max_width, "上下限必须有区间");
    assert_eq!(geometry.margin, 12.0, "G17-c [规格] 外边距 12");
    assert_eq!(geometry.pad_h, 16.0, "A1 HorizontalSpacing");
    assert_eq!(geometry.pad_v, 6.0, "A1 SnackbarVerticalPadding");
}

// ---- §7 Dialog --------------------------------------------------------------

/// 7.4：官方区间 **280..560**，本项目取 **480**，且 480 落在区间内。
#[test]
fn dialog_geometry_sits_inside_the_official_bounds() {
    let geometry = DialogGeometry::of(&ui(SchemeFlavor::Light));
    assert_eq!(geometry.min_width, 280.0, "A1 DialogMinWidth");
    assert_eq!(geometry.max_width, 560.0, "A1 DialogMaxWidth");
    assert_eq!(geometry.width, 480.0, "G17-e 本项目取值");
    assert!(
        geometry.min_width <= geometry.width && geometry.width <= geometry.max_width,
        "自选宽必须落在官方区间内"
    );
}

/// 复审 P2-2：`max(280)` 那条支以前一次都没被执行过（只有定义 + 一个调用点）。
///
/// 抓得住：有人把下限去掉（200 → 200）、或把 `min/max` 顺序写反（200 → 480 之外的值）。
/// 抓不住：渲染时是否真的画了 280（那需要真窗口；gallery 的 360 那格由 probe 的逐卡宽度兜）。
#[test]
fn dialog_width_respects_the_official_minimum() {
    let geometry = DialogGeometry::of(&ui(SchemeFlavor::Light));
    assert_eq!(
        geometry.effective_width(200.0),
        280.0,
        "可用宽 200 → 仍画官方下限 280"
    );
    assert_eq!(geometry.effective_width(360.0), 360.0, "窄宿主取可用宽");
    assert_eq!(
        geometry.effective_width(800.0),
        480.0,
        "宽宿主夹到项目值 480"
    );
}

/// 7.1 / 7.3 / 7.6：圆角 28、内距 24 / 16 / 24、动作 8 / 12。
#[test]
fn dialog_padding_and_action_gaps_are_the_tokens() {
    let geometry = DialogGeometry::of(&ui(SchemeFlavor::Light));
    assert_eq!(geometry.radius, 28.0, "7.1 CornerExtraLarge");
    assert_eq!(geometry.padding, 24.0, "7.3 DialogPadding");
    assert_eq!(geometry.title_gap, 16.0, "7.3 TitlePadding bottom");
    assert_eq!(geometry.body_gap, 24.0, "7.3 TextPadding bottom");
    assert_eq!(geometry.action_gap, 8.0, "7.6 ButtonsMainAxisSpacing");
    assert_eq!(geometry.action_cross, 12.0, "7.6 ButtonsCrossAxisSpacing");
}

/// 7.7 + D27：scrim = 黑@0.32 叠在 canvas 上之后的**不透明**实色。
#[test]
fn scrim_is_scrimalpha_precomposited_onto_the_canvas() {
    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let ui = ui(flavor);
        // 期望值按**品牌色板**（= `Ui` 注入的那一份）：M229 复审 F1 之前这里用的是
        // `Scheme::baseline`，Ink 板上会得到偏蓝的遮罩（数值见 `DevDocs/artifacts/m229/README.md` §6）。
        let scheme = unlucky_design::color::BrandBoard::for_flavor(flavor).scheme();
        let canvas = scheme.get(MaterialLayer::Canvas.surface_role());
        let expected = to_iced(scheme.scrim().over(canvas));
        assert_eq!(
            scrim(&ui),
            expected,
            "{flavor:?}：scrim 必须逐位等于「黑@0.32 叠 canvas（品牌板）」"
        );
        // F1 的回归门：不许回到 tone 派生的 baseline 上算。
        let baseline = Scheme::baseline(flavor);
        assert_ne!(
            expected,
            to_iced(
                baseline
                    .scrim()
                    .over(baseline.get(MaterialLayer::Canvas.surface_role()))
            ),
            "{flavor:?}：遮罩退回了 baseline（Ink 下会偏蓝）"
        );
        assert_eq!(
            scrim(&ui).a,
            1.0,
            "D27：CPU 档不出现半透明（预合成后必须不透明）"
        );
        assert_ne!(
            scrim(&ui),
            ui.layer(MaterialLayer::Canvas),
            "遮罩必须与画布有可见差异"
        );
        assert!((SCRIM_ALPHA - 0.32).abs() < f32::EPSILON, "令牌口径 α=0.32");
    }
}

// ---- §15 空态（项目自定） ----------------------------------------------------

/// 15.1：图标块 96 / 圆角 16 / 图标 32（**全部 `[估]`**，官方没有这个组件）。
#[test]
fn empty_state_geometry_is_the_project_defined_set() {
    let geometry = EmptyStateGeometry::of(&ui(SchemeFlavor::Light));
    assert_eq!(geometry.block, 96.0);
    assert_eq!(geometry.block_radius, 16.0, "ShapeScale::Large");
    // 复审 C：登记值 32（`IconSize::Empty`）。这条守「登记值 = 交给 `view_at` 的 px」——
    // 它抓不住「盒子 32 而 svg 仍钉 24」那种情况（那是渲染层的事，probe 量墨迹 22..34 兜）。
    assert_eq!(
        geometry.icon, 32.0,
        "盒内图标 32（`view_at` 按 32 烘焙 stroke）"
    );
    // **复审 P2-1 的回归断言**：三个间距必须精确等于令牌档位。
    // 早先这里只写 `> 0.0`，所以「Column::spacing(8) 与显式 gap(16/24) 双算成 32/40」
    // 完全藏得住（渲染实测 264 vs 登记 232）。
    assert_eq!(geometry.gap_title, 16.0, "块↔标题 = Space::X4");
    assert_eq!(geometry.gap_body, 8.0, "标题↔副文案 = Space::X2");
    assert_eq!(geometry.gap_action, 24.0, "副文案↔CTA = Space::X6");
    assert_eq!(geometry.block, 96.0);
    assert_eq!(
        super::empty_state::BLOCK_LAYER,
        MaterialLayer::Overlay,
        "块底色是 SurfaceContainerHigh（材质层 Overlay）"
    );
}

// ---- 结构 / 角色 ------------------------------------------------------------

/// 五个入口都能在**任意**消息类型下构建（契约 §2：不 import `app::Message`）。
#[test]
fn every_component_builds_for_a_foreign_message_type() {
    #[derive(Clone, Debug, PartialEq)]
    enum Other {
        Yes,
        No,
    }

    let ui = ui(SchemeFlavor::Light);
    let _: Element<'_, Other> = linear(&ui, Progress::determinate(0.5));
    let _: Element<'_, Other> = circular(&ui, Progress::indeterminate(0.3));
    let _: Element<'_, Other> = snackbar(
        &ui,
        Snackbar::new("已保存")
            .placement(Placement::Top)
            .two_lines()
            .action("撤销", Other::No)
            .dismiss(Other::No),
    );
    let _: Element<'_, Other> = dialog(
        &ui,
        Dialog::new("确认删除？", "此操作不可撤销。")
            .double(("取消", Other::No), ("删除", Other::Yes)),
    );
    let _: Element<'_, Other> = empty_state(
        &ui,
        EmptyState::new(
            unlucky_design::icons::IconKind::Instances,
            "还没有实例",
            "建一个开始游戏",
        )
        .action("新建", Other::Yes),
    );
    // 无动作的三种形态也要能构建（DialogActions::None / 无 action 的 Snackbar）
    let _: Element<'_, Other> = dialog(&ui, Dialog::new("已是最新版本", "没有可用更新。"));
    let _: Element<'_, Other> = snackbar(&ui, Snackbar::new("已复制"));
    let _: Element<'_, Other> = empty_state(
        &ui,
        EmptyState::new(
            unlucky_design::icons::IconKind::Logs,
            "没有日志",
            "启动一次游戏后这里会有内容",
        ),
    );
}

/// 状态层必须来自 `interaction`（不是组件里自己乘 α）：hover 层不透明且与容器色不同。
#[test]
fn snackbar_state_layers_come_from_the_interaction_module() {
    let ui = ui(SchemeFlavor::Light);
    let hover = interaction::overlay_color(
        &ui,
        Interaction::Hovered,
        Role::InversePrimary,
        Role::InverseSurface,
    )
    .expect("hover 有状态层");
    assert_eq!(hover.a, 1.0, "状态层必须是实色（CPU/GPU 一致）");
    assert_ne!(
        hover,
        ui.color(Role::InverseSurface),
        "叠了 8% 必须看得出来"
    );

    // 禁用内容走 token 口径（38%），不是「变灰一点」
    let disabled =
        interaction::disabled_content_over_role(&ui, Base::CANVAS, Role::OnSurface, false);
    assert_ne!(disabled, ui.color(Role::OnSurface));
    assert_eq!(disabled.a, 1.0);
}
