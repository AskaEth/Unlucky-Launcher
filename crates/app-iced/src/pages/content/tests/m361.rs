//! **M361**（P1-1）：`Compact` / `Tight`（无独立详情列）+ `Split` 档把结果列表挤成 0 高。
//!
//! ## 判据是算式，不是「看起来有结果」
//!
//! ```text
//! 主列   = 正文 − 列头（`head::chrome`，含到首卡的那条间距）
//! 列表   = 主列 − 尾部 − 一条 Space::X2
//!        ≥ 460 − 281 − 40 − 8 = 131   （Split 的下限：正文正好 460、列头满额、尾部是按钮）
//! 一张卡 = `head::one_card` = 三行档 76 + 行距 4 = 80
//! ```
//!
//! 131 ≥ 80 ⇒ 在 Split 档的任何尺寸都至少放得下一张整卡（不靠某一档尺寸）。
//!
//! ## 记录更正（r1 P2-1 → M374）：这条公式曾经**不覆盖 `Modpack`**
//!
//! 复审 r1 指出：`Modpack` 在 Split 档把「导入/导出」整块排在**列头之前**（`pack_before_results`
//! 对它恒为真），那块（实测 329 高）不在 `chrome` 的账上，所以它的式子是
//! `正文 − chrome − X2 − 尾部 − (导入区 + 分隔线 1 + 2×X2)`：`880×900 = 45`、`944×720 = −135 → 0`。
//! 那是 11-05 就登记、当时仍开着的旧 bug，本条按登记「没修」并用一条用例钉住它。
//!
//! **M374 把它修了**：导入区在 Split 档落进列表的滚动区 ⇒ 两个族共用上面这一条公式。
//! 判据、反例与像素对照搬到了 [`super::m374`]；`stack.rs` 的「未修」措辞同轮改成修好后的口径。
//!
//! 像素证据与修前/修后对照图在 `DevDocs/artifacts/listzero/`（880×900 修前视口 **45**、
//! 944×720 修前 **0**；负对照 1024×760 = **251**）。

use unlucky_design::spacing::Space;

use super::super::layout::{resolve, BodyMode};
use super::super::parts::head;
use super::super::stack::detail_in_list_scroll;
use crate::pages::frame;
use crate::pages::layout::{frame_metrics, FrameMetrics, PaneKit};
use crate::theme::Ui;

pub(super) fn ui() -> Ui {
    use unlucky_design::color::SchemeFlavor;
    use unlucky_design::typography::Platform;

    use crate::theme::{Families, Ui};

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

/// `Split` 档的列表视口：`正文 − 列头 − 尾部 − 一条 Space::X2`（详情已进滚动区，不占高度）。
///
/// **M374 之后这是两个族共用的一条式子**（`Mod` / 资源包 / 光影 / 整合包）：整合包的导入区也进了
/// 滚动区，不再是主列的减数。`tests/m374.rs` 直接调它，不另抄一份（抄两份必然漂）。
pub(super) fn split_viewport(ui: &Ui, metrics: FrameMetrics, window_height: f32) -> f32 {
    let body = frame::body_height(ui, metrics, window_height, true);
    body - head::full_head_max(ui) - ui.metrics().control_height - ui.space(Space::X2)
}

/// 修前的列表视口：详情块（高随数据变）挤在**同一列**里，占掉 `分隔线 + 详情 + 3 条间距`。
fn viewport_before(ui: &Ui, metrics: FrameMetrics, window_height: f32, detail: f32) -> f32 {
    let body = frame::body_height(ui, metrics, window_height, true);
    body - head::full_head_max(ui)
        - ui.metrics().control_height
        - ui.metrics().divider_thickness
        - detail
        - 3.0 * ui.space(Space::X2)
}

/// **判据（`Mod` / 资源包 / 光影）**：`Split` 档 + 无独立详情列 ⇒ 列表视口 ≥ 一张整卡。
///
/// 逐尺寸扫一遍（宽 560~1200 × 高 400~1400）。`Modpack` 的同一套扫描在
/// [`super::m374`]（M374 之后两个族共用 `split_viewport` 这一条式子，扫的尺寸范围也一致）。
///
/// 抓得住：谁把详情又塞回同一列、谁抬高列头、谁给尾部加高、谁把 `Split` 的下限调低。
#[test]
fn every_squeezed_split_size_keeps_a_whole_result_card_for_mods() {
    let ui = ui();
    let card = head::one_card(&ui);
    let mut squeezed = 0;

    // 宽 560~1200（含 `Tight` / `Compact` 两档）、高 400~1400（含两种正文档的边界）
    for width in (560..=1200).step_by(8) {
        let metrics = frame_metrics(PaneKit::Single, width as f32);
        if resolve(metrics.available).tier.has_detail() {
            continue; // 有独立详情列的档不在本条范围（详情不在同一列）
        }
        for height in (400..=1400).step_by(8) {
            let window_height = height as f32;
            let body = frame::body_height(&ui, metrics, window_height, true);
            if BodyMode::of(body) != BodyMode::Split {
                continue;
            }
            let viewport = split_viewport(&ui, metrics, window_height);
            assert!(
                viewport >= card,
                "{width}×{height}（正文 {body}）列表只剩 {viewport}，装不下一张卡 {card}"
            );
            squeezed += 1;
        }
    }

    assert!(
        squeezed > 100,
        "扫到的尺寸只有 {squeezed} 个，判据没真跑到（门槛/档位可能被改没了）"
    );
}

/// **P2-2 判据**：`Select` 之后详情在视野内 —— 算式成立在「详情是滚动内容的**最后一块**」之上。
///
/// `snap_to(END)` 把视口落到内容末尾 ⇒ 视野 = 内容最后 `viewport` px。于是：
///
/// ```text
/// 尾块 = 分隔线 1 + X2 + 详情（实测 329）= 338
/// 视口 ≥ 338 ⇒ 整块详情都在（880×900：391 ✔）
/// 视口 < 338 ⇒ 看到详情下半截，而「装入」按钮是详情块的最后一项（不是 16px 的原因行）⇒
///              只要视口 ≥ control_height 40 就必在（944×720：211 ✔；Split 下限 131 ✔）
/// ```
#[test]
fn selecting_a_result_puts_the_detail_in_view() {
    let ui = ui();
    let measured_detail = 329.0;
    let tail = ui.metrics().divider_thickness + ui.space(Space::X2) + measured_detail;
    let reason_line = unlucky_design::typography::TypeRole::BodySmall
        .style()
        .line_height_px;
    // 「装入」按钮（40）+ 一条原因行（16）—— 详情块末尾那两样必须在视野里
    let bottom_block = ui.metrics().control_height + reason_line;

    assert!(
        bottom_block < tail,
        "前提：末尾那两样比整块详情矮，才有「看到下半截」这种情形"
    );

    // **机制接线**（真调用，不是字符串）：这个任务不是空任务
    assert_eq!(
        super::super::snap_to_detail().units(),
        1,
        "`snap_to_detail` 必须是 1 条 widget 操作（0 = 点了结果什么都不动）"
    );

    for (width, height, whole) in [(880.0, 900.0, true), (944.0, 720.0, false)] {
        let metrics = frame_metrics(PaneKit::Single, width);
        let viewport = split_viewport(&ui, metrics, height);
        assert!(
            viewport >= bottom_block,
            "{width}×{height} 的视口 {viewport} 连详情末尾的装入按钮都装不下"
        );
        assert_eq!(
            viewport >= tail,
            whole,
            "{width}×{height}：整块详情在不在视野内与算式不符（视口 {viewport}，尾块 {tail}）"
        );
    }

    // **结构**：三样块在**同一个**滚动区里，且详情必须是**最后一块**（`snap_to(END)` 要落在详情
    // 上）；Id 也只挂在带详情的那一路。`Element` 不可内省，只有源码这一条抓手。
    let parts = include_str!("../parts.rs");
    let at = |needle: &str| {
        parts
            .find(needle)
            .unwrap_or_else(|| panic!("`parts.rs` 里少了 `{needle}`：滚动区那几块拼不起来了"))
    };
    assert!(
        at("blocks.push(rows.into())") < at("blocks.extend(tail)"),
        "接在结果行之后的块必须真的排在结果行之后（同一个滚动区）"
    );
    assert!(
        at("blocks.extend(tail)") < at("blocks.extend(detail)"),
        "详情必须是滚动内容的**最后一块**：M374 的导入区排在它前面，`snap_to(END)` 才落在详情上"
    );
    assert!(
        parts.contains("if has_detail {") && parts.contains("view.id(RESULTS_SCROLL_ID)"),
        "Id 必须只挂在带详情的那一路（`Wide` / `Narrow` 挂上的话，点结果会莫名滚动列表）"
    );
}

/// 反例：**把详情放回同一列**（修前的排法）⇒ 同一个判据在两个现场都红。
///
/// 详情高 329 是**实测值**（880×900 那张图的墨迹带：标题 506 → 装入按钮底 827，中间 4 条
/// `Space::X3`、1 条分隔线，加回到这 329）。它不是常数：标题/简介行数、版本条数、目标实例、
/// 禁用原因都会改它 —— 修前的式子里有一个**没有上界**的减数，这才是根因。
#[test]
fn putting_the_detail_back_in_the_same_column_turns_the_criterion_red() {
    let ui = ui();
    let card = head::one_card(&ui);
    let measured_detail = 329.0;

    // 880×900：修前实测视口 45px（墨迹带 437..481，卡被切在标题下）
    let metrics = frame_metrics(PaneKit::Single, 880.0);
    let before = viewport_before(&ui, metrics, 900.0, measured_detail);
    assert!(
        (before - 45.0).abs() < 0.5,
        "修前 880×900 的算式应当落在实测的 45px 上，得到 {before}"
    );
    assert!(before < card, "修前必须是红的：{before} < {card}");

    // 944×720：修前算式给负数 ⇒ flex 的 `max(0.0)` 夹成 0（实测主列里没有任何高度 ≥60 的条带）
    let metrics = frame_metrics(PaneKit::Single, 944.0);
    let before = viewport_before(&ui, metrics, 720.0, measured_detail);
    assert!(
        before <= 0.0,
        "修前 944×720 应当是 0（负值被夹断），得到 {before}"
    );

    // 同一批尺寸，修后（详情进滚动区）都过
    for width in [880.0, 944.0] {
        let metrics = frame_metrics(PaneKit::Single, width);
        for height in [720.0, 900.0] {
            let after = split_viewport(&ui, metrics, height);
            assert!(after >= card, "修后 {width}×{height} 只有 {after}");
        }
    }
}

/// 排法的判据：只有「没有独立详情列」**且**「列表自己滚」两条同时成立才改排法。
///
/// 少一个都不该动：`Wide`/`Narrow` 的详情在右侧独立列，`ScrollWhole` 的列表按自然高度铺开
/// （外层滚动条兜住，详情再高也挤不出 0）。多改一格都会动到 1180×900 / M358 那几张基线图。
#[test]
fn only_the_missing_detail_column_in_split_mode_moves_the_detail_into_the_list() {
    use super::super::layout::Tier;

    assert!(detail_in_list_scroll(Tier::Compact, BodyMode::Split));
    assert!(detail_in_list_scroll(Tier::Tight, BodyMode::Split));
    // 有独立详情列：详情不进列表的滚动区（进去了 = 详情被滚走，Wide 档的基线会变）
    assert!(!detail_in_list_scroll(Tier::Wide, BodyMode::Split));
    assert!(!detail_in_list_scroll(Tier::Narrow, BodyMode::Split));
    // `ScrollWhole`：列表是 `Shrink`，本来就挤不出 0（M161 起的排法，动它 = 改最小窗的图）
    assert!(!detail_in_list_scroll(Tier::Compact, BodyMode::ScrollWhole));
    assert!(!detail_in_list_scroll(Tier::Tight, BodyMode::ScrollWhole));
}

/// 结构性反例：`view.rs` 必须把详情的作用域交给列表。
///
/// 删掉那一行（回到修前的 `.push(divider).push(detail)`）⇒ 本条红 —— 上面两条算式用例
/// 抓不住「排法」本身（算式写在本文件里，改回去它们照样绿），所以这一条钉调用点。
#[test]
fn the_content_view_routes_the_detail_through_the_shared_rule() {
    let view = include_str!("../view.rs");
    assert!(
        view.contains("stack::detail_in_list_scroll(rails.tier, mode)"),
        "详情块的作用域必须走 `stack::detail_in_list_scroll`（M361 的 P1-1 会复发）"
    );
}
