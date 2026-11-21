//! **磁贴网格的动效接线**（M461）：格子真的按泵的采样淡入——不是「泵在跑、网格照旧硬跳」。
//!
//! ## 为什么要有这一条（泵的算术在 `components/motion/tests/grid.rs`）
//!
//! 泵的帧数 / 端点 / 零帧判据证明**时间轴**对，证明不了**画它的那一段**读了没有：
//! `grid::cell` 里那行 `motion::rows::frame(..)` 删掉之后，泵照样跑满 22 帧、`samples()`
//! 照样涨，而屏幕上一帧都不变——那正是 M455 交付时的现场（网格直接 `chunks(columns)` 分行）。
//! `Element` 没有几何内省，「这一格被压到了多少」只能由画的那一段自己说 ⇒ 取样口
//! [`super::probe`]（与 M455 的 [`super::footprint`] 同一种做法）。
//!
//! ## 变异与实测红（每条只改一处，跑 `cargo test --lib grid` 再复原）
//!
//! | 变异 | 改哪里 | 实测红在哪句 |
//! |---|---|---|
//! | 网格不接泵（回到 M455 的硬跳） | `grid.rs::cell` 里把 `motion::rows::frame(..)` 换成恒 `1.0` | `the_grid_actually_fades_while_the_pump_runs`（`fading == 0`、可见度和 = 6）——**实测红** |
//! | 重放从 `1.0` 起（静息行也走 `1 → 1`） | `pump/layout.rs` 的 `reenter_from` 恒返 `1.0` | `the_grid_actually_fades_while_the_pump_runs`（可见度和恒 6）——**实测红** |
//! | 关掉动效还画遮罩 | `pump/layout.rs` 的 `replay` 删掉 `off` 分支 | `no_scale_renders_every_cell_at_rest`（`sum` 不再是 6）——**实测红** |
//! | 静息帧也常驻一层遮罩 | `veil` 的 `(visibility < 1.0).then(..)` → `Some(..)` | `a_resting_grid_draws_no_veil` 的 `fading == 0`（本轮**未**单独跑：逐像素等价由 `DevDocs/artifacts/m461/shots.log` 的 `differing=0/896800` 另证） |
//!
//! ## 抓不住
//!
//! 出图观感（那要真机截图的中间帧，见 `DevDocs/artifacts/m461/`）；也抓不住遮罩的颜色是否
//! 与脚下画布一致（`MaterialLayer::Canvas`）——那条由出图的静息基线逐像素盯着
//! （400 ms 末帧 vs 静息：`differing=0/896800`）。

use unlucky_design::color::SchemeFlavor;
use unlucky_design::motion::MotionScale;
use unlucky_design::typography::Platform;
use unlucky_presenter::prefs::{InstanceView, TileSize};

use crate::app::components::motion;
use crate::pages::layout::{self, PaneKit};
use crate::theme::{Families, Ui};

use super::probe::{self, Last};
use super::*;

/// 一帧的步长（16 ms）：泵的采样闸门就是它（`components/motion/pump/mod.rs` 的 `FRAME_MS`）。
///
/// 为什么在这里写一个数而不是引用那个常量：页面层**不许**碰时长（`app/subscription.rs` 的
/// `pages_cannot_reach_for_a_duration` 连用例一起扫）。这里要的不是"选一档时长"，而是"把时间轴
/// 往前拨一帧"——步长与泵对齐这件事由泵自己的用例钉着（`components/motion/tests/grid.rs`）。
const STEP_MS: f64 = 16.0;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

fn ui() -> Ui {
    Ui::with_platform(SchemeFlavor::Light, Platform::Windows, FAMILIES)
}

/// 六条实例（Medium = 4 列 ⇒ 两排 4 + 2）。
fn block() -> Instances {
    Instances {
        rows: (0..6)
            .map(|index| InstanceRow {
                id: format!("i{index}"),
                name: format!("demo-{index}"),
                supporting: "1.20.4 · Fabric 0.16.5".to_string(),
                status: None,
            })
            .collect(),
        detail: None,
        scan: ScanPhase::Ready,
        leaving: Vec::new(),
    }
}

fn ids() -> Vec<&'static str> {
    vec!["i0", "i1", "i2", "i3", "i4", "i5"]
}

/// 渲染一遍磁贴网格（只建树不画），读回画的那一段记下的动效取样。
fn render(block: &Instances) -> Last {
    let ui = ui();
    let metrics = layout::frame_metrics(PaneKit::ListDetail, 1180.0);
    let texts = Texts::of(unlucky_i18n::Lang::En.catalog());
    let view = View {
        mode: InstanceView::Tiles,
        tile_size: TileSize::Medium,
        query: "",
    };
    let element = tiles(&ui, metrics, block, None, texts, view, 500.0);
    drop(element);
    probe::last().expect("画过一屏网格之后必须有取样（`grid::probe::note` 的接线）")
}

/// **判据（泵在跑 ⇒ 格子真的在淡）**：起跑后第 112 ms 那一帧，六格都还没到 1、其中三格在中间态。
///
/// 为什么是 112 ms（7 × 16 ms）：错位档 0/50/100/150（`Short1`，封顶 3）⇒ 前 3 格已经在走
/// （第 0 格 56%、第 1 格 31%、第 2 格 6% 的腿），后 3 格还没轮到（起点严格 `0`）。这是
/// `DevDocs/artifacts/m461/` 那张 176 ms 图之外的**算术版中间帧**——同一段时间轴，不必起窗口。
///
/// 断言分两层：
/// 1. **逐格对账**：取样口累加的可见度之和，必须**逐位等于**用例自己按
///    `motion::rows::frame(..)` 求出来的和（网格读的就是泵的采样，不是自己编一个数）；
/// 2. **有中间态**：`fading == 6`（一个都没到静息）、`sum` 既不等于 0 也不等于 6。
///
/// 抓得住：网格没接泵（`sum` 恒 6、`fading` 恒 0）、只接了其中几格（对不上第 1 层）、
/// 或者起点写成 1（`min_visibility` 不再等于泵对「还没轮到那格」给的 0）。
/// 抓不住：淡出的观感与遮罩颜色（出图基线 + `Artifacts` 那张中间帧）。
#[test]
fn the_grid_actually_fades_while_the_pump_runs() {
    motion::reset();
    motion::configure_for_test(MotionScale::STANDARD, None);
    motion::rows::observe(motion::rows::List::Instances, &ids());
    assert!(
        motion::rows::relayout(motion::rows::List::Instances),
        "切布局必须起跑（接线断了这里就是 false）"
    );
    for _ in 0..7 {
        motion::advance_by(STEP_MS);
    }

    let sample = render(&block());
    assert_eq!(sample.cells, 6, "六条实例就是六个格子");

    // 第 1 层：逐格对账（这一层才是「接线」，不是「大概在动」）。
    let expected: f32 = ids()
        .iter()
        .map(|id| motion::rows::frame(motion::rows::List::Instances, id).visibility)
        .sum();
    assert_eq!(
        sample.sum, expected,
        "网格用的必须是泵对每个 id 的采样之和（取样 {sample:?}）"
    );
    assert_eq!(
        sample.min_visibility,
        motion::rows::frame(motion::rows::List::Instances, "i3").visibility,
        "还没轮到的那格（错位档 3）起点必须是泵给的 0"
    );

    // 第 2 层：真的有中间态（否则「对账」对一个恒 6 的和也会过）。
    assert_eq!(
        sample.fading, 6,
        "这一帧六格都不该在静息（取样 {sample:?}）"
    );
    assert!(
        sample.sum > 0.0 && sample.sum < 6.0,
        "必须有格子落在 (0,1)：可见度之和 {}",
        sample.sum
    );
    motion::reset();
}

/// **静息帧一个遮罩都不画**：没有轨道时六格的可见度严格 `1.0`（出图与 M455 逐像素相同）。
///
/// 抓得住：`veil` 写成 `Some(..)`（动画起止两侧都会常驻一层与画布同色的 quad——出图基线会红，
/// 而那正是 D27 的 α 纪律要堵的）。
#[test]
fn a_resting_grid_draws_no_veil() {
    motion::reset();
    motion::configure_for_test(MotionScale::STANDARD, None);
    motion::rows::observe(motion::rows::List::Instances, &ids());

    let sample = render(&block());
    assert_eq!(sample.cells, 6);
    assert_eq!(sample.fading, 0, "静息帧不许画任何遮罩");
    assert_eq!(sample.min_visibility, 1.0, "静息可见度必须**严格**是 1");
    assert_eq!(sample.sum, 6.0, "六格之和必须逐位等于 6（每格都是 1）");
    motion::reset();
}

/// **关掉动效 ⇒ 格子当场是终态**（不是「跑一遍 0 时长动画」）。
///
/// 抓得住：`pump/layout.rs::replay` 里删掉 `off` 那一支（NONE 也插轨道时，这一帧的
/// `min_visibility` 会 `< 1`、`fading > 0`）。
#[test]
fn no_scale_renders_every_cell_at_rest() {
    motion::reset();
    motion::configure_for_test(MotionScale::NONE, None);
    motion::rows::observe(motion::rows::List::Instances, &ids());
    assert!(
        !motion::rows::relayout(motion::rows::List::Instances),
        "NONE 档不许起轨道"
    );

    let sample = render(&block());
    assert_eq!(sample.fading, 0, "NONE 档不许画遮罩");
    assert_eq!(sample.min_visibility, 1.0, "NONE 档当场就是终态");
    assert_eq!(sample.sum, 6.0, "NONE 档六格之和逐位等于 6");
    motion::reset();
}
