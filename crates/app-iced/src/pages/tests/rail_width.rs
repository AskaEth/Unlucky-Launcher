//! **M301 的几何判据**：页面可用宽跟着**这一帧的轨宽**走（折叠 −64 / 展开 −88 / 差值正好 24）。
//!
//! 这一组都打在「当帧采样」这一层（`layout::rail`），也就是产品里外壳写的那个值：采样写什么，
//! 页面就扣什么。四条任务判据的落点：
//!
//! | 判据 | 用例 |
//! |---|---|
//! | ①折叠态可用宽 = 窗口 − 64 ②展开态 = 窗口 − 88 ③差正好 24 | [`the_page_width_follows_the_collapsed_rail`] |
//! | 无外壳通道回落 88（harness / 组件 demo / 单测） | [`no_shell_channel_falls_back_to_the_expanded_token`] |
//! | 第二帧必须回到 88（不粘滞上一帧的采样） | [`the_sampled_width_is_not_sticky`] |
//! | 门槛跟着活值走（两列版面的成立线也降 24） | [`collapsing_lowers_the_pane_threshold_by_the_same_24`] |
//! | 唯一写入口 = 外壳，且排在页面树之前 | [`only_the_shell_samples_the_rail_width`]、[`the_shell_samples_before_the_page_tree_is_built`] |

use std::path::{Path, PathBuf};

use unlucky_design::spacing::LayoutMetrics;

use super::super::frame;
use super::super::layout::{self, block, frame_metrics, PageLayout, PaneKit, WidthClass};

/// 两个档位一律**从令牌取**（本文件不许再出现 88.0 / 64.0 这种字面量）。
fn ends() -> (f32, f32) {
    (
        layout::rail_width_collapsed(),
        layout::rail_width_expanded(),
    )
}

/// 采一次样、跑一段几何、复位（复位用 `forget`：回到「本线程未采样」，不把值留给后面的断言）。
fn with_rail<T>(width: f32, run: impl FnOnce() -> T) -> T {
    layout::note_rail_width(width);
    let out = run();
    layout::rail::forget_rail_width();
    out
}

/// **判据③的对照组**：没有外壳的通道必须按展开档 88 扣宽。
///
/// **抓得住**：`None` 的回落值被改（比如回落成折叠档，或改成 panic）；
/// **抓不住**：产品路径（外壳采样之后）的宽度——那是下面几条与外壳接线判据的事。
#[test]
fn no_shell_channel_falls_back_to_the_expanded_token() {
    let (_, expanded) = ends();
    layout::rail::forget_rail_width();
    assert_eq!(layout::rail_width(), expanded, "未采样 ⇒ 展开档");

    let window = 1180.0;
    let metrics = frame_metrics(PaneKit::Single, window);
    let gutter = WidthClass::Expanded.gutter();
    assert_eq!(
        metrics.available,
        window - expanded - 2.0 * gutter,
        "无外壳通道里页面按展开档扣宽（1180 − 88 − 48 = 1044）"
    );
    assert_eq!(
        metrics.available,
        window - LayoutMetrics::DESKTOP.nav_rail_width - 2.0 * gutter,
        "回落值就是 nav_rail_width 令牌"
    );
}

/// **判据①②③**：折叠态 = 窗口 − 64、展开态 = 窗口 − 88、两者差**正好** 24。
///
/// 两侧都用同一个窗口（1180，验收尺寸），于是 diff 里窗口项与页边距项自己约掉——剩下的只能是
/// 轨宽差。**抓得住**：有人把 `rail_width()` 改回常量 88（折叠态那条立刻红），或者把
/// `available_width` 的减法换回硬编码；**抓不住**：外壳到底有没有真的写这个值（下一条 + 接线判据）、
/// 真画出来的像素（`DevDocs/artifacts/m301/` 的折叠态全窗图）。
#[test]
fn the_page_width_follows_the_collapsed_rail() {
    let (collapsed, expanded) = ends();
    let window = 1180.0;
    let gutter = WidthClass::Expanded.gutter();

    let open = with_rail(expanded, || frame_metrics(PaneKit::Single, window));
    let folded = with_rail(collapsed, || frame_metrics(PaneKit::Single, window));

    // ① 折叠态：窗口 − 64 − 两侧页边距
    assert_eq!(
        folded.available,
        window - collapsed - 2.0 * gutter,
        "折叠态可用宽必须是窗口 − 64 − 48（1180 − 64 − 48 = 1068）"
    );
    assert_ne!(
        folded.available,
        window - expanded - 2.0 * gutter,
        "折叠态不许再按 88 扣宽——那正是 M300 留下的 24px 空白带"
    );
    // ② 展开态：窗口 − 88 − 两侧页边距
    assert_eq!(
        open.available,
        window - expanded - 2.0 * gutter,
        "展开态回到窗口 − 88 − 48（1044）"
    );
    // ③ 差值正好 24 = 88 − 64
    assert_eq!(folded.available - open.available, 24.0, "两态差值正好 24");
    assert_eq!(expanded - collapsed, 24.0, "差值来自两个令牌本身");

    // 折叠真的多腾出 24px：折宽 = 轨 + 左页边距 + 块宽(+偏移) + 右页边距，右缘贴死窗口
    assert_eq!(
        collapsed + gutter + block::body_offset(folded) + frame::body_width(folded) + gutter,
        window,
        "折叠态正文块右缘必须贴到窗口右缘（右侧不许留 24px 空白带）：64 + 24 + 1068 + 24 = 1180"
    );
    assert_eq!(
        frame::body_width(folded) - frame::body_width(open),
        24.0,
        "块宽也必须跟着长 24（不是只有 available 这个字段变了）"
    );
}

/// 采样**不粘滞**：写完折叠档、再写展开档，第二次必须回到 88。
///
/// **抓得住**：只有一个方向的写入（折叠写、展开不写）；`forget` 之后回到回落值；
/// **抓不住**：外壳有没有每帧都调用写入口（那是 `only_the_shell_samples_the_rail_width` +
/// `the_shell_samples_before_the_page_tree_is_built` 的事）。
#[test]
fn the_sampled_width_is_not_sticky() {
    let (collapsed, expanded) = ends();
    layout::rail::forget_rail_width();

    layout::note_rail_width(collapsed);
    assert_eq!(layout::rail_width(), collapsed, "第一帧：折叠档");

    layout::note_rail_width(expanded);
    assert_eq!(
        layout::rail_width(),
        expanded,
        "第二帧必须回到展开档，不许粘住 64"
    );

    layout::rail::forget_rail_width();
    assert_eq!(
        layout::rail_width(),
        expanded,
        "复位后回到回落值（= 展开档）"
    );
}

/// 两列版面的**门槛**也跟着活值走（少了 24 的轨宽，成立线也低 24）。
///
/// 设置页（supporting-pane）：展开门槛 720 / 折叠 696 ⇒ 700 宽的窗口在展开态是单列、折叠态是两列。
/// 实例页（list-detail）没有这条翻转带：它的几何门槛 816 低于官方 expanded 断点 840，档位是更严的
/// 那一条——这条断言把「24px 改动不会让实例页在 840 线上反复翻版」写成判据。
///
/// **抓得住**：门槛被钉成常量（折叠后仍要求 720）；**抓不住**：翻转瞬间的重排观感（要出动画）。
#[test]
fn collapsing_lowers_the_pane_threshold_by_the_same_24() {
    let (collapsed, expanded) = ends();
    let open_min = with_rail(expanded, layout::supporting_pane_min_window);
    let folded_min = with_rail(collapsed, layout::supporting_pane_min_window);
    assert_eq!(
        open_min - folded_min,
        24.0,
        "支撑栏门槛同样差 24（720 vs 696）"
    );

    // 700 宽：展开态够不到 720 ⇒ 单列；折叠态跨过 696 ⇒ 两列
    assert_eq!(
        with_rail(expanded, || frame_metrics(PaneKit::Supporting, 700.0)
            .layout),
        PageLayout::SingleColumn
    );
    assert_eq!(
        with_rail(collapsed, || frame_metrics(PaneKit::Supporting, 700.0)
            .layout),
        PageLayout::SupportingPane,
        "少了 24 的轨宽，700 宽的窗口就放得下两列了"
    );

    // 实例页：816 的几何门槛在 840 的档位之下，两态在 840 上都是 list-detail（没有翻转带）
    for rail in [expanded, collapsed] {
        assert_eq!(
            with_rail(rail, || frame_metrics(PaneKit::ListDetail, 840.0).layout),
            PageLayout::ListDetail,
            "840 窗宽下两态都成立两列：档位（840）比几何门槛（816 / 792）更严"
        );
    }
}

/// **唯一写入口**：产品代码里只有外壳的 `app/view/rail.rs` 采样（其余是定义点与用例）。
///
/// 为什么扫源码：这条判据的本质是「不许有第二个写入口」——类型系统管不住一个 `pub(crate)` 函数。
/// 仓内已有同样的先例（`layout::source`、`tests/no_hardcoded_tokens.rs` 都靠扫文本守形态）。
///
/// **抓得住**：谁在 update 侧 / 页面里偷写一份轨道宽度（那会与外壳漂成两个数）；
/// **抓不住**：外壳是不是在**每一帧**都写（下一条盯顺序）。
#[test]
fn only_the_shell_samples_the_rail_width() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&root, &mut files);
    assert!(
        files.len() > 50,
        "只扫到 {} 个源文件：扫描逻辑坏了",
        files.len()
    );

    let mut writers = Vec::new();
    for path in files {
        let text = std::fs::read_to_string(&path).expect("读源文件");
        if !text.contains("note_rail_width(") {
            continue;
        }
        let relative = path
            .strip_prefix(&root)
            .expect("相对路径")
            .to_string_lossy()
            .replace('\\', "/");
        // 定义点 + 它自己的单测；以及任何用例文件（用例**应该**能摆采样，那是它们的输入）。
        if relative == "pages/layout/rail.rs" || is_test_path(&relative) {
            continue;
        }
        writers.push(relative);
    }

    assert_eq!(
        writers,
        vec!["app/view/rail.rs".to_string()],
        "轨宽的写入口只许有一个：外壳 `app/view/rail.rs` 的 `rail_account()`"
    );
}

/// **顺序契约**：外壳的采样必须排在页面树之前（`app/view.rs` 里两条语句的先后）。
///
/// 为什么是源码判据而不是数值判据：iced 的 `Element` 没有几何内省接口，「同一帧里谁先跑」只能
/// 从装配代码看。写反了 = 页面用上一帧的轨宽扣宽（动画最后一帧会停在 87 点几）。
#[test]
fn the_shell_samples_before_the_page_tree_is_built() {
    let view = include_str!("../../app/view.rs");
    let sampled = view
        .find("let account = self.rail_account();")
        .expect("`App::view` 里必须有外壳采样这一行");
    let page = view
        .find("page_with_settings_transition(")
        .expect("`App::view` 里必须建页面树");
    assert!(
        sampled < page,
        "外壳采样必须排在页面树之前：先写后读，页面才与轨同一拍"
    );
}

/// 递归收集 `.rs`（与 `tests/no_hardcoded_tokens.rs` 同一手法）。
fn rust_files(dir: &Path, collected: &mut Vec<PathBuf>) {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|error| panic!("读不动 {}：{error}", dir.display()));
    for entry in entries {
        let path = entry.expect("目录项读失败").path();
        if path.is_dir() {
            rust_files(&path, collected);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            collected.push(path);
        }
    }
}

/// 用例文件：`*/tests.rs` 或 `*/tests/*.rs`。
fn is_test_path(relative: &str) -> bool {
    relative.ends_with("/tests.rs") || relative.contains("/tests/")
}

/// **组件版**导航轨（M78 的 `NavigationRail`）声明的宽度必须与布局系统同值。
///
/// 注意这不是产品路径的实现（M90 r3 订正）：产品页的 rail 由 `shell/nav.rs` 画，组件版只服务
/// `pages::app`（harness 出口图）与单测。这条守的是「组件与布局系统没分叉」，**不是**「产品 rail
/// 没问题」——两套实现的漂移风险见 `pages/mod.rs` 偏离表 #10。
///
/// M301：组件版只有 88 / 240 两态（`pages/nav.rs` 的模块文档写了为什么不做第三档），所以这里比
/// 的是**展开档**；本线程没有外壳采样 ⇒ `rail_width()` 正是展开档。
///
/// **抓得住**：`NavPaneGeometry::rail().width` 与 `layout::rail_width_expanded()` 分叉；
/// **抓不住**：容器圆角/药丸摆放的像素级正确性（组件自己的 probe 管），也抓不住 `shell/nav.rs`。
#[test]
fn the_real_rail_agrees_with_the_layout_system() {
    let ui = super::ui();
    let geometry = crate::app::components::navigation::geometry::NavPaneGeometry::of(
        &ui,
        crate::app::components::navigation::NavLayout::Rail,
    );
    let expanded = layout::rail_width_expanded();
    assert!(
        (geometry.width - expanded).abs() < f32::EPSILON,
        "真组件 rail 宽 {} 与布局系统的展开档 {expanded} 分叉了",
        geometry.width
    );
    assert!(
        (layout::rail_width() - expanded).abs() < f32::EPSILON,
        "没有外壳采样时，页面扣宽也必须等于展开档"
    );
}
