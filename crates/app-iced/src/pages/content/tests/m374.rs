//! **M374**：`Modpack` + `Split` 档的「导入/导出区」把结果列表压成 45 / 0（11-05 登记的旧 bug）。
//!
//! ## 判据是算式，不是「看起来有结果」
//!
//! ```text
//! Split 档的列表视口 = 正文 − 列头 − Space::X2 − 尾部            （M361 定，M374 补全）
//! 修前 `Modpack` 多一个减数：− (导入区 + 分隔线 1 + 2×Space::X2)
//! 导入区 329（880 / 944 宽，`Compact`）· 289（1180 宽，`Wide`）—— 它随宽度变，不是常量
//! 一张整卡 = `head::one_card` = 三行档 76 + 行距 4 = 80
//! ```
//!
//! ```text
//! 修前 880×900：720 − 281 − 8 − 40 − (329 + 1 + 16) =  45   实测首卡带 783..827 = 45（被切）
//! 修前 944×720：540 − …                              = −135 → 0  实测一条结果带都没有
//! 修前 1180×900（导入区 289）：720 − … − (289+1+16)   =  85   实测首卡带 76 + 视口 84
//! 修后 880×900：720 − 281 − 8 − 40                   = 391   实测首卡带 437..512 = 76（整张）
//! 修后 944×720：540 − 281 − 8 − 40                   = 211
//! Split 的下限（正文 460）：460 − 281 − 8 − 40        = 131 ≥ 80
//! ```
//!
//! 修后 `Modpack` 与 `Mod` 族**共用** `super::m361::split_viewport` 这一条式子 ⇒ 判据与尺寸无关，
//! 不是「这两个尺寸刚好够」。像素对照在 `DevDocs/artifacts/modpackzero/`（`before/` / `after/`）。
//!
//! ## 为什么不选另外两条路（数都在上面那张算式里）
//!
//! * **给导入区一个高度上限**：要让列表在任何 `Split` 尺寸都 ≥ 80，上限得 ≤ 34（正文 460 那一档），
//!   而拖拽区本身就有 68 —— 这一档永远装不下（见 [`capping_the_import_block_cannot_work_at_the_split_floor`]）；
//! * **调换顺序（结果先于导入）**：算式一分不变。`Fill` 子项拿到的永远是「扣完所有非 `Fill`
//!   兄弟」之后的剩余，那块区只要还在主列里，列表就还是 45 / 0。

use super::super::layout::{resolve, BodyMode, MIN_BODY_HEIGHT};
use super::super::parts::head;
use super::super::stack::{detail_in_list_scroll, pack_in_list_scroll};
use super::m361::{split_viewport, ui};
use unlucky_design::spacing::Space;

/// 位置那一半（落点判据 / 五态覆盖 / 接线）—— 与本文件同一 mission，拆开免得撞 400 行。
mod place;
use crate::pages::frame;
use crate::pages::layout::{frame_metrics, PaneKit};

/// 导入/导出区的实测高：`Compact`（880 / 944 宽）**329**、`Wide`（1180 宽）**289**。
///
/// 从 `before/` 两张图的墨迹带反解（880/944：正文顶 156 → 搜索条顶 502，中间是导入区 + 17 的
/// 间距与分隔线；1180：156 → 462 ⇒ 289 + 17）。**不是常量**：宽档把路径行与目标实例 chips 并成
/// 一行，于是矮 40 —— 这也是「给一个硬上限」那条路走不通的另一个理由。
const IMPORT_BLOCK_COMPACT: f32 = 329.0;
const IMPORT_BLOCK_WIDE: f32 = 289.0;

/// `Modpack` + `Split` 的列表视口，**按当前排法算**。
///
/// * `in_scroll = true`（本条修好之后）：导入区在列表的滚动区里 ⇒ 它就是 `Mod` 族那条式子；
/// * `in_scroll = false`（修前的排法）：导入区是主列的非 `Fill` 兄弟 ⇒ 再扣掉
///   「导入区 + 分隔线 1 + 2×Space::X2」（下限 0 = flex 的 `max(0.0)`）。
///
/// 这个入参不是装饰：判据与全尺寸扫描都把它接到 `stack::pack_in_list_scroll` 上，所以**谁把排法
/// 改回去，判据就跟着红**（不是「算式写死在测试里，改回去照样绿」——那是 M361 r1 的坑）。
fn modpack_viewport(
    ui: &crate::theme::Ui,
    metrics: crate::pages::layout::FrameMetrics,
    height: f32,
    in_scroll: bool,
    import_block: f32,
) -> f32 {
    let viewport = split_viewport(ui, metrics, height);
    if in_scroll {
        viewport
    } else {
        (viewport - import_block - 1.0 - 2.0 * ui.space(Space::X2)).max(0.0)
    }
}

/// **判据 ①**：`Modpack` + `Split` 在 880×900 与 944×720 下的列表视口 = `Mod` 族的数（391 / 211）。
///
/// 抓得住：谁把导入区又放回列头之上（哪怕只是「只对整合包」的一行特例）—— 那个数会变成 45 / 0。
#[test]
fn the_modpack_split_case_gets_the_same_viewport_as_the_mod_family() {
    let ui = ui();
    let card = head::one_card(&ui);
    assert_eq!(card, 80.0, "一张整卡 = 三行档 76 + 行距 4");
    // 排法的判据接进算式：`false` 就退回修前的 45 / 0（下面那条断言立刻红）
    let in_scroll = pack_in_list_scroll(BodyMode::Split);

    for (width, height, expected) in [(880.0, 900.0, 391.0), (944.0, 720.0, 211.0)] {
        let metrics = frame_metrics(PaneKit::Single, width);
        assert!(
            !resolve(metrics.available).tier.has_detail(),
            "{width} 宽不在 `Compact`（这一档详情也在同一列里）"
        );
        let viewport = modpack_viewport(&ui, metrics, height, in_scroll, IMPORT_BLOCK_COMPACT);
        assert!(
            (viewport - expected).abs() < 0.5,
            "{width}×{height} 的列表视口是 {viewport}，应当与 `Mod` 族同值 {expected}"
        );
        assert!(
            viewport >= card,
            "{width}×{height} 列表只剩 {viewport}，装不下一张卡 {card}"
        );
    }
}

/// **判据 ③（全尺寸扫描）**：`Split` 档的**任何**尺寸、**任何**档位，列表视口都 ≥ 一张整卡。
///
/// 与 `tests/m361.rs` 那条扫法相同，但**不跳过** `Wide` / `Narrow`：那两档的详情在右侧独立列，
/// 可列表同样是 `Fill`，导入区同样压得下去（1180×900 的 Modpack 修前只剩 **85**）。
#[test]
fn every_split_size_keeps_a_whole_result_card_for_modpacks() {
    let ui = ui();
    let card = head::one_card(&ui);
    // 排法的判据：`Split` 档的导入区必须进滚动区 —— 下面那个数就是靠它才成立的
    assert!(
        pack_in_list_scroll(BodyMode::Split),
        "`Split` 档的导入区必须进列表的滚动区（留在主列里就是 45 / 0）"
    );

    let mut scanned = 0;
    let mut wide = 0;
    for width in (560..=1200).step_by(8) {
        let metrics = frame_metrics(PaneKit::Single, width as f32);
        let tier = resolve(metrics.available).tier;
        for height in (400..=1400).step_by(8) {
            let window_height = height as f32;
            let body = frame::body_height(&ui, metrics, window_height, true);
            if BodyMode::of(body) != BodyMode::Split {
                continue;
            }
            let viewport = modpack_viewport(
                &ui,
                metrics,
                window_height,
                pack_in_list_scroll(BodyMode::Split),
                IMPORT_BLOCK_COMPACT,
            );
            assert!(
                viewport >= card,
                "{width}×{height}（正文 {body}，{tier:?}）列表只剩 {viewport}，装不下一张卡 {card}"
            );
            scanned += 1;
            if tier.has_detail() {
                wide += 1;
            }
        }
    }

    assert!(scanned > 100, "只扫到 {scanned} 个尺寸，判据没真跑到");
    assert!(
        wide > 20,
        "有独立详情列那两档只扫到 {wide} 个尺寸 —— 它们没进覆盖范围"
    );
}

/// **反例**：把导入区放回列头之上 ⇒ 判据在两个现场都红（45 / 0），第三处只剩 85。
///
/// 这条钉的是「判据抓得住改坏」：谁把 `pack_in_list_scroll` 改成恒假（或把导入区又 `push` 回
/// 主列），这里给出的就是**那之后**的数。
#[test]
fn putting_the_import_block_back_above_the_head_turns_the_criterion_red() {
    let ui = ui();
    let card = head::one_card(&ui);

    // 880×900：补上导入区之后是 45 —— 实测首卡带 783..827 = 45（被视口底切掉）
    let metrics = frame_metrics(PaneKit::Single, 880.0);
    let before = modpack_viewport(&ui, metrics, 900.0, false, IMPORT_BLOCK_COMPACT);
    assert!(
        (before - 45.0).abs() < 0.5,
        "修前 880×900 应当是 45，得到 {before}"
    );
    assert!(before < card, "修前必须是红的：{before} < {card}");

    // 944×720：负值被 flex 的 `max(0.0)` 夹成 0（实测主列里没有任何结果带）
    let metrics = frame_metrics(PaneKit::Single, 944.0);
    let before = modpack_viewport(&ui, metrics, 720.0, false, IMPORT_BLOCK_COMPACT);
    assert!(
        before < 0.5,
        "修前 944×720 应当是 0（负值被夹断），得到 {before}"
    );

    // 1180×900（`Wide`）：导入区 289 ⇒ 修前 85（实测首卡 76 + 视口 84，墨迹带取中有 ±1 的量化）
    let metrics = frame_metrics(PaneKit::Single, 1180.0);
    assert!(resolve(metrics.available).tier.has_detail());
    let before = modpack_viewport(&ui, metrics, 900.0, false, IMPORT_BLOCK_WIDE);
    assert!(
        (before - 85.0).abs() < 0.5,
        "修前 1180×900 应当是 85，得到 {before}"
    );
}

/// **几何前提**：这块区只要还在主列的兄弟里，就**不可能**同时满足「任何 `Split` 尺寸 ≥ 一张卡」。
///
/// 算式：`导入区 ≤ 正文 − 列头 − X2 − 尾部 − 1 − 2×X2 − 80`，`Split` 的下限是正文 460 ⇒ **≤ 34**。
/// 拖拽区一项就有 68（`Compact` 实测），所以「给一个硬上限」这条路在数学上就不成立 ——
/// 这也是选「移出预算」而不是「限高」的理由，写进用例免得下一个人再试一遍。
#[test]
fn capping_the_import_block_cannot_work_at_the_split_floor() {
    let ui = ui();
    let card = head::one_card(&ui);
    let gap = ui.space(Space::X2);
    let metrics = frame_metrics(PaneKit::Single, 880.0);

    // 一份算式，三个现场：`room` = 留给导入区的最大高度（超过就装不下一张卡）
    let room = |body: f32| {
        body - head::full_head_max(&ui)
            - gap
            - ui.metrics().control_height
            - ui.metrics().divider_thickness
            - 2.0 * gap
            - card
    };

    let cap = room(MIN_BODY_HEIGHT);
    assert!(
        (cap - 34.0).abs() < 0.5,
        "`Split` 下限那一档只给 {cap}，用例文档写的是 34"
    );
    assert!(
        cap < 68.0,
        "拖拽区一项就 68 高 —— 上限 {cap} 连入口都画不下"
    );

    for (height, expected) in [(900.0, 294.0), (720.0, 114.0)] {
        let body = frame::body_height(&ui, metrics, height, true);
        assert_eq!(BodyMode::of(body), BodyMode::Split);
        let got = room(body);
        assert!(
            (got - expected).abs() < 0.5,
            "{height} 高的余量应当是 {expected}，得到 {got}"
        );
        assert!(
            got < IMPORT_BLOCK_COMPACT,
            "{height} 高的余量 {got} 竟然装得下 {IMPORT_BLOCK_COMPACT} 的导入区"
        );
    }
}

/// **档位判据不许被这条改动带跑**：详情那一路（M361 的 P1-1）一个条件都没动。
#[test]
fn the_detail_placement_rule_is_untouched_by_this_fix() {
    use super::super::layout::Tier;

    assert!(detail_in_list_scroll(Tier::Compact, BodyMode::Split));
    assert!(detail_in_list_scroll(Tier::Tight, BodyMode::Split));
    assert!(!detail_in_list_scroll(Tier::Wide, BodyMode::Split));
    assert!(!detail_in_list_scroll(Tier::Narrow, BodyMode::Split));
    assert!(!detail_in_list_scroll(Tier::Compact, BodyMode::ScrollWhole));

    // 导入区只跟「列表是不是 `Fill`」有关，与档位无关（`Wide` / `Narrow` 同一个病）
    assert!(pack_in_list_scroll(BodyMode::Split));
    assert!(!pack_in_list_scroll(BodyMode::ScrollWhole));
}
