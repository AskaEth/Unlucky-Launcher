//! `frame` 的断言：空态 / 卡片 / 页头规格与一屏字号档。
use super::pieces::{EMPTY_ICON_BLOCK, EMPTY_ICON_GLYPH};
use super::*;

use unlucky_design::color::{Role, SchemeFlavor};
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::shape::ShapeScale;
use unlucky_design::typography::Platform;

use crate::app::components::interaction::Base;
use crate::pages::layout::{self, frame_metrics, PaneKit};
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
fn empty_state_follows_the_12b_reference() {
    // 15.1 图标块 96×96、圆角 Large(16)；15.2 标题 HeadlineSmall；15.3 副文案 BodySmall
    let spec = Component::EmptyState.spec();
    assert_eq!(EMPTY_ICON_BLOCK, 96.0);
    assert_eq!(spec.shape, ShapeScale::Large);
    assert_eq!(spec.text, TypeRole::HeadlineSmall);
    assert_eq!(ui().style(spec.text).size_px, 24.0);
    assert_eq!(spec.support, Some(TypeRole::BodySmall));
    assert_eq!(ui().style(TypeRole::BodySmall).size_px, 12.0);
    // 字形 32~48（[估]）
    assert!((32.0..=48.0).contains(&EMPTY_ICON_GLYPH));
    const { assert!(EMPTY_ICON_GLYPH < EMPTY_ICON_BLOCK) };
}

/// 卡片圆角取**规格卡**的 16（M229 换肤），组件表 `OutlinedCard` 的 12 只作为对照保留。
#[test]
fn card_uses_the_spec_corner_not_the_component_table() {
    let ui = ui();
    assert_eq!(ui.card_shape(), ShapeScale::Large);
    assert_eq!(ui.radius_dp(ui.card_shape()).top_left, 16.0);
    // 组件表的官方值仍是 12（表不动，差异登记在 `inkcat` 模块与 M229 报告里）。
    assert_eq!(Component::OutlinedCard.spec().shape, ShapeScale::Medium);
    assert_eq!(ui.radius_dp(ShapeScale::Medium).top_left, 12.0);
}

#[test]
fn page_header_uses_the_two_documented_heights() {
    let ui = ui();
    assert_eq!(ui.metrics().header_height(false), 56.0);
    assert_eq!(ui.metrics().header_height(true), 84.0);
}

/// 一屏字号 ≤4 档（rubric D4）。规格表来源：12b §16.4 / §15 / 3.x OutlinedCard。
#[test]
fn every_screen_keeps_at_most_four_text_sizes() {
    let ui = ui();

    let header = ui.emphasized(Component::TopAppBar.spec().text).size_px;
    let row_title = ui.style(Component::ListItem.spec().text).size_px;
    let row_support = ui
        .style(Component::ListItem.spec().support.unwrap())
        .size_px;
    let action = ui.style(Component::TextButton.spec().text).size_px;
    let body = ui.style(TypeRole::BodyMedium).size_px;
    let empty_title = ui.style(Component::EmptyState.spec().text).size_px;

    let screens: [(&str, &[f32]); 3] = [
        // 实例页（list-detail 与窄窗单列）：页头 + 行标题 + 行副文案 + 操作/正文
        ("instances", &[header, row_title, row_support, action, body]),
        // 设置页：页头 + 分组项/操作 + 行标签 + 行值 + 空态标题
        (
            "settings",
            &[header, action, row_support, body, empty_title],
        ),
        // 下载 / 日志：页头 + 空态标题 + 空态副文案 + 操作
        ("empty-pages", &[header, empty_title, row_support, action]),
    ];

    for (name, sizes) in screens {
        let mut unique = sizes.to_vec();
        unique.sort_by(f32::total_cmp);
        unique.dedup();
        assert!(
            unique.len() <= 4,
            "{name} 一屏用了 {} 档字号 {unique:?}，超过 rubric D4 的 4 档",
            unique.len()
        );
        assert!(unique[0] >= 12.0, "{name} 出现小于 12px 的字号");
        assert!(unique[unique.len() - 1] <= 24.0, "{name} 字号过大");
    }
}

/// 版面几何是页面唯一宽度来源：两列版面的正文列必须窄于可用宽度。
#[test]
fn two_pane_body_is_narrower_than_the_available_width() {
    let wide = frame_metrics(PaneKit::ListDetail, 1180.0);
    assert!(wide.column < wide.available);
    assert!(wide.secondary.is_some());
}

/// **`body_height` 用真的 `Ui::metrics` + `FrameMetrics` 算**（M161 复审 P2-2：这条函数此前
/// 一条测试都没有，而④的「最小窗口」用例是拿 56/88/32 反推的——三个数全错）。
///
/// 抓得住：①`titlebar_height` / `header_height` / `gutter` 任何一个被改（chrome 变了，
/// 下面的等式立刻红）；②把两行页头当单行算（`has_subtitle` 被忽略）；③把页边距只减一遍。
#[test]
fn body_height_subtracts_the_real_shell_chrome() {
    let ui = ui();
    let metrics = frame_metrics(PaneKit::Single, 880.0);
    let metrics_wide = frame_metrics(PaneKit::Single, 1180.0);

    let chrome = |two_line: bool| {
        ui.metrics().titlebar_height + ui.metrics().header_height(two_line) + 2.0 * metrics.gutter
    };

    // 真值来自 token：自绘标题栏 48 + 两行页头 84 + 上下页边距 2×24 = 180。
    assert_eq!(
        chrome(true),
        48.0 + 84.0 + 2.0 * 24.0,
        "chrome 与 design token 不一致（titlebar_height / page_header_tall_height / Expanded gutter）"
    );
    assert_eq!(body_height(&ui, metrics, 560.0, true), 560.0 - chrome(true));
    // 单行页头少 28（84 − 56 = `page_header_tall_height` − `page_header_height`）。
    assert_eq!(
        body_height(&ui, metrics, 560.0, false),
        560.0 - chrome(false)
    );
    assert!(
        body_height(&ui, metrics, 560.0, true) < body_height(&ui, metrics, 560.0, false),
        "带副标题的页头更高，正文可用高必须更小"
    );
    // 窄窗（Compact < 600）页边距更小 ⇒ 同样窗口高下正文更高。
    let compact = frame_metrics(PaneKit::Single, 560.0);
    assert_eq!(compact.class, crate::pages::layout::WidthClass::Compact);
    assert!(
        compact.gutter < metrics_wide.gutter,
        "Compact 档的页边距应小于 Expanded（16 < 24）"
    );
    assert!(
        body_height(&ui, compact, 560.0, true) > body_height(&ui, metrics_wide, 560.0, true),
        "页边距变小 ⇒ 正文可用高变大"
    );
    // 窗口比外壳还矮时不返回负数（不许把版面算成 NaN/负值）。
    assert_eq!(body_height(&ui, metrics, 100.0, true), 0.0);
}

/// **卡片导出的底 = 卡片真正画出来的底色**（M237）。
///
/// 抓得住：①`BASE` 被换成「看着像」的角色（`Surface` / `SurfaceContainer` /
/// `SurfaceContainerHighest` 都能通过「是个容器色」这种含糊的判据）；②`card()` 改画别的材质层
/// 而常量没跟上（两边漂一半，卡片里的禁用控件又会与卡底脱节）。
///
/// 抓不住：面板 α < 1 时的**半透明**合成——`Ui::layer` 会把角色乘 α，而 [`Base`] 只装得下
/// 一个 `Role`。CPU 验收档 α 恒 1，两者逐值相同；GPU 档的这条差已登记在 `pieces::BASE` 上。
///
/// 判据是**页面级像素**（截图量出来的那份，`pages/settings/tests/java_shot.rs`）的前置条件：
/// 这一条红了，那边的「叠卡片底」就是拿一个错的值在比。
#[test]
fn the_card_base_is_the_colour_the_card_actually_paints() {
    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let ui = Ui::with_platform(
            flavor,
            Platform::Windows,
            Families {
                text: "Segoe UI",
                display: "Segoe UI",
                mono: "Consolas",
            },
        );

        assert_eq!(
            BASE.role(),
            MaterialLayer::Card.surface_role(),
            "{flavor:?}: 卡片底必须是 `card()` 画的那一层的角色"
        );
        assert_eq!(
            ui.layer(MaterialLayer::Card),
            ui.color(BASE.role()),
            "{flavor:?}: `card()` 画的颜色与 `BASE` 求出来的不是同一个值"
        );
        // 卡片底 ≠ 画布底：相等的话「卡片里的禁用控件叠哪一层」永远看不出区别（判据恒绿）
        assert_ne!(BASE, Base::CANVAS, "{flavor:?}: 卡片底不能是画布底");
        assert_ne!(
            ui.color(BASE.role()),
            ui.color(Role::Surface),
            "{flavor:?}: 卡片色与画布色必须可分"
        );
    }
}

/// **源码结构**：`card()` 用 [`MaterialLayer::Card`] 画底，`BASE` 从**同一个**层取角色。
///
/// 为什么还要一条扫源码的：iced 的 `Element` 没有内省接口（容器样式是闭包），「画的是哪一层」
/// 在运行期拿不到；而这件事本身就是契约——容器画什么，就导出什么。换个写法（先存进局部变量）
/// 会绕过它，那属于重构后要一起改判据的正常代价（与 `pages/tests.rs` 里 `frame::body` 那两条同源）。
#[test]
fn the_card_base_and_the_card_paint_share_one_layer() {
    const PIECES: &str = include_str!("pieces.rs");

    let body = layout::source::function_body(PIECES, "pub fn card<'a>(");
    assert!(
        body.contains("ui.layer(MaterialLayer::Card)"),
        "`frame::card` 的底色必须取 `MaterialLayer::Card`（`BASE` 就是从它推出来的）：\n{body}"
    );

    let base = layout::source::statement(PIECES, "pub const BASE");
    assert!(
        base.contains("Base::of(") && base.contains("MaterialLayer::Card"),
        "`frame::BASE` 必须从 `card()` 画的那一层取角色：{base}"
    );
}
