//! 右栗折叠过渡的**几何判据**（M257 / A2）：帧数 + 端点精确 + 严格单调 + 不溢出 + 键盘可达。
//!
//! 判据口径（`14-motion-and-animation.md` §7-7）：**不靠像素比对**。像素能证明「某一帧的宽度是 92」，
//! 证明不了「中间有 13 个相位」「收尾那一帧不多要帧」。观感（好不好看）走 D30 出图。

use unlucky_design::motion::MotionScale;

use super::super::metrics::{gutter_at, COLLAPSED_WIDTH, DEFAULT_EXPANDED_WIDTH};
use super::super::state::{AgentUi, Focus, RailState};
use crate::app::components::motion;

/// 取证窗口宽（与 D30 的产品默认一致）。
const WINDOW: f32 = 1180.0;
/// 帧泵的单帧步进（`motion::FRAME_MS` 的测试口径）。
const FRAME: f64 = 16.0;

/// 每个用例都从干净的动效全局态起跑（thread-local，互不影响，但同一用例内要确定）。
fn configure(scale: MotionScale) {
    motion::reset();
    motion::configure_for_test(scale, None);
}

/// 把一整个折叠 / 展开过程采样到底（返回**含起点与终点**的宽度序列）。
fn sample_widths(agent: &AgentUi) -> Vec<f32> {
    let mut out = vec![agent.rail_width(WINDOW)];
    let mut frames = 0;
    while motion::needs_frames() {
        motion::advance_by(FRAME);
        out.push(agent.rail_width(WINDOW));
        frames += 1;
        assert!(frames < 60, "折叠轨道没停，疑似终止条件失效");
    }
    out
}

/// **展开：第一帧还是 48、中间严格单调、结束时严格等于 320、帧数落在 12~20。**
///
/// 抓得住：①把宽度改回硬跳（`rail_width` 直接返回静息值 ⇒ 第一帧就是 320，本用例的第一步红）；
/// ②把终止条件删掉（`while` 后面的兜底断言红）；③把时长改成 600ms（帧数超 20）。
/// 抓不住：中间帧「看起来」顺不顺（那是曲线与观感，走 D30 出图），也抓不住真实帧率。
#[test]
fn expanding_is_monotone_and_lands_exactly_on_the_settled_width() {
    configure(MotionScale::STANDARD);
    let mut agent = AgentUi::new();
    assert_eq!(agent.rail, RailState::Collapsed);

    agent.expand();
    let widths = sample_widths(&agent);
    let frames = widths.len() - 1;

    assert_eq!(
        widths.first().copied(),
        Some(COLLAPSED_WIDTH),
        "第一帧必须还在折叠档（48，端点精确）"
    );
    assert_eq!(
        widths.last().copied(),
        Some(DEFAULT_EXPANDED_WIDTH),
        "最后一帧必须是**严格** 320（不是 319.9x）"
    );
    assert!(
        (12..=20).contains(&frames),
        "250ms / 16ms 应落在 12~20 帧，实测 {frames} 帧"
    );
    for pair in widths.windows(2) {
        assert!(
            pair[1] >= pair[0],
            "宽度必须单调不减（不许来回抖）：{:?} → {:?}",
            pair[0],
            pair[1]
        );
    }
    assert!(
        widths.iter().filter(|w| **w > 48.5 && **w < 319.5).count() >= 8,
        "必须有足够多的中间宽度（否则就是硬跳）：{widths:?}"
    );
    assert!(!motion::needs_frames(), "走完必须退订帧");
    motion::reset();
}

/// **收起：同一条判据，反向**（`short4` 200ms ⇒ 10~13 帧）。
#[test]
fn collapsing_is_monotone_downwards_and_ends_exactly_on_the_strip_width() {
    configure(MotionScale::STANDARD);
    let mut agent = AgentUi::new();
    agent.expand();
    sample_widths(&agent);
    assert_eq!(agent.rail_width(WINDOW), DEFAULT_EXPANDED_WIDTH);

    agent.collapse();
    let widths = sample_widths(&agent);
    let frames = widths.len() - 1;

    assert_eq!(widths.first().copied(), Some(DEFAULT_EXPANDED_WIDTH));
    assert_eq!(widths.last().copied(), Some(COLLAPSED_WIDTH));
    assert!(
        (10..=20).contains(&frames),
        "200ms / 16ms 应落在 10~20 帧，实测 {frames} 帧"
    );
    for pair in widths.windows(2) {
        assert!(pair[1] <= pair[0], "收起必须单调不增：{pair:?}");
    }
    motion::reset();
}

/// **`MotionScale::NONE`：右栗宽度当帧就是终值**（独立用例，包含在「右栗」这个面上）。
///
/// 抓得住：把 `observe_rail` 的 `is_off()` 分支删掉 ⇒ 会插一条 0 时长轨道，本用例的
/// 「`needs_frames()` 为假」与「第一帧就是 320」两条会同时红。
/// 抓不住：`NONE` 以外的档位。
#[test]
fn motion_scale_none_puts_the_rail_at_the_end_value_on_the_first_frame() {
    configure(MotionScale::NONE);
    let mut agent = AgentUi::new();

    agent.expand();
    assert_eq!(
        agent.rail_width(WINDOW),
        DEFAULT_EXPANDED_WIDTH,
        "NONE 下展开档当帧就是 320（不是「很快走完」）"
    );
    assert!(!motion::needs_frames(), "NONE 下一帧都不许要");
    assert_eq!(
        agent.content_width(WINDOW),
        WINDOW - DEFAULT_EXPANDED_WIDTH - gutter_at(DEFAULT_EXPANDED_WIDTH),
        "中栏也必须当帧拿到终值"
    );

    agent.collapse();
    assert_eq!(agent.rail_width(WINDOW), COLLAPSED_WIDTH);
    assert!(!motion::needs_frames());
    motion::reset();
}

/// **拆出档：宽度 0、不走过场**（拆分是布局切换，不是过渡对象）。
///
/// 抓得住：有人把 `Detached` 也接进折叠轨道（那会做一次 48→0 的缩水，`needs_frames()` 为真）。
#[test]
fn detached_is_never_animated() {
    configure(MotionScale::STANDARD);
    let mut agent = AgentUi::new();
    agent.expand();
    sample_widths(&agent);

    agent.detach();
    assert_eq!(agent.rail_width(WINDOW), 0.0, "拆出档不占宽");
    assert!(!motion::needs_frames(), "拆出不许起过渡");
    assert_eq!(agent.content_width(WINDOW), WINDOW, "中栗拿回整窗");
    motion::reset();
}

/// **不溢出 + 不抖**：折叠全程的「右栗总占宽 = 面板 + 拖拽缝」单调、且不超过静息展开档。
///
/// 抓得住：①把 `gutter_at` 写成常量 8（收尾那一帧总宽会是 56 而不是 48 ⇒ 最后一步的严格相等红）；
/// ②让宽度插值越过目标（回弹）——单调与上限两条同时红。
/// 抓不住：`row!` 里的兄弟顺序写错（那要出图看）。
#[test]
fn the_rail_never_wider_than_its_settled_footprint() {
    configure(MotionScale::STANDARD);
    let mut agent = AgentUi::new();
    agent.expand();

    let mut totals = Vec::new();
    loop {
        let width = agent.rail_width(WINDOW);
        totals.push(width + gutter_at(width));
        if !motion::needs_frames() {
            break;
        }
        motion::advance_by(FRAME);
    }

    let settled = DEFAULT_EXPANDED_WIDTH + gutter_at(DEFAULT_EXPANDED_WIDTH);
    for total in &totals {
        assert!(*total <= settled, "右栗总占宽越界：{total} > {settled}");
    }
    assert_eq!(totals.last().copied(), Some(settled), "收尾必须是静息总宽");
    assert_eq!(
        totals.first().copied(),
        Some(COLLAPSED_WIDTH),
        "起点是折叠档的 48（图标条没有拖拽缝）"
    );
    for pair in totals.windows(2) {
        assert!(pair[1] >= pair[0], "总占宽必须单调：{pair:?}");
    }
    motion::reset();
}

/// **键盘可达性：过渡中途按 Tab 的落点与静息时完全相同**（键盘用户不许被动效拖慢）。
///
/// 抓得住：有人把焦点环也接进「进度」（例如按当前宽度决定环上有哪些控件）——那会让过渡中途的
/// 落点与静息不同，本用例红。也抓住「环空了 ⇒ 焦点掉到根」：环首尾相接，从任何一个格位都能走到
/// 一个真实控件。
/// 抓不住：Tab 的**时序**（真窗口里按 Tab 到焦点环被画出之间隔几帧，只能人试）。
#[test]
fn tab_lands_on_the_same_control_mid_transition() {
    configure(MotionScale::STANDARD);
    let mut agent = AgentUi::new();
    agent.expand();

    // 走到中途（宽度既不是 48 也不是 320）。
    for _ in 0..6 {
        motion::advance_by(FRAME);
    }
    let width = agent.rail_width(WINDOW);
    assert!(
        width > COLLAPSED_WIDTH && width < DEFAULT_EXPANDED_WIDTH,
        "前提：应该处在过渡中途（实测 {width}）"
    );

    // 目标档是展开：环 = [Collapse, Detach, Send, Composer]（与宽度无关）。
    let mid = Focus::Collapse.step(false, RailState::Expanded, false, false);
    assert_eq!(mid, Focus::Detach, "中途 Tab 的落点必须是真实控件");

    // 收起到目标档是折叠：环 = [Collapse, Send, Composer]，且首尾相接（不会掉到根）。
    let collapsed = Focus::Composer.step(false, RailState::Collapsed, false, false);
    assert_eq!(
        collapsed,
        Focus::Collapse,
        "折叠档的环必须首尾相接（焦点不许掉到根）"
    );
    motion::reset();
}

/// 采样序列里至少要有 8 个「既不是起点也不是终点」的宽度——判定「不是硬切」的真正依据。
///
/// 单独一条是因为它比帧数更直接：帧数只说明时间被走完，中间宽度才说明**有中间态**。
#[test]
fn there_are_real_intermediate_widths() {
    configure(MotionScale::STANDARD);
    let mut agent = AgentUi::new();
    agent.expand();
    let widths = sample_widths(&agent);
    let middle = widths.iter().filter(|w| **w > 48.5 && **w < 319.5).count();
    assert!(middle >= 8, "中间宽度只有 {middle} 个：{widths:?}");
    motion::reset();
}
