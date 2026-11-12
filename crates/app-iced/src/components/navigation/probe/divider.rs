//! 分隔线守卫（6.6）：**Secondary 的分隔线必须是一条连续直线**。
//!
//! 逐 tab 分段画会在相邻段之间留缝（视觉 r1 P2-2 实测 `25..101 | 103..191 | 193..265`，
//! 交界像素偏亮）。这条守卫被复审三轮打过三次，每次都是「假绿」，所以下面把**每个判据
//! 挡的是哪一次假通过**写清楚——改之前先读一遍：
//!
//! | 判据 | 挡的假通过 |
//! |---|---|
//! | ① 锚定「细 + 近乎实心 + 薄 + 低对比 + 跨度 ≥200」的一行（见 `scan::Shot::find_thin_line`） | 五态行的 40px 高底色带、`Primary` 指示条顶替分隔线 |
//! | ② 线色必须是 `Surface → SurfaceVariant` 的插值（覆盖率 30%~105%） | 更靠上的同类细线顶替：`OutlineVariant` 组间线（t 出界）、hover 8% 带（三通道 t 散开） |
//! | ③ 参考色取**该行墨迹众数色**（见 `scan::Shot::line_segments`） | 缺口恰落在跨度中点时，参考色退化成页面底色 → 恒 1 段 |
//! | ④ 在该行**整段墨迹跨度**上数段（不是某次 run 的窗口） | 缺口在窗口外看不见、在窗口内被「跳过去重新起跑」滑过 |
//!
//! ## 锚定行**无条件打印**
//!
//! 不管红绿都往 stderr 打一行 `[probe] divider anchored at y=… span=x..x color=(r,g,b) segments=N`。
//! 之前绿的时候什么都不说，于是「锚错行」是静默的——不看断言在量什么，等于没有断言。
//!
//! ## 深浅两档
//!
//! 浅色图必跑；深色图（`tabs-dark.png`）**存在就跑**（深色的 divider 对比更低，更值得测）。
//! 没有深色图时不算失败，但会在输出里留一行 `dark 档跳过`。

use unlucky_design::color::Role;

use crate::theme::Ui;

use super::scan::{rgb, Shot};

/// 在 `shot` 上验证分隔线是一条直线；`label` 只用于日志（`light` / `dark`）。
pub(super) fn check(shot: &Shot, ui: &Ui, label: &str) {
    let surface = rgb(ui.color(Role::Surface));
    let variant = rgb(ui.color(Role::SurfaceVariant));
    let (y, x0, x1, color) = shot
        .find_thin_line(surface, variant, 200, 120)
        .unwrap_or_else(|| panic!("[{label}] 找不到 Secondary 那一行细线（锚定失败）"));
    let segments = shot.line_segments(y, x0, x1);
    eprintln!(
        "[probe] divider anchored at y={y} span=x{x0}..{x1} color={color:?} segments={segments} \
         ({label}, {shot_width}x{shot_height})",
        shot_width = shot.width,
        shot_height = shot.height,
    );
    assert_eq!(
        segments, 1,
        "[{label}] Secondary 分隔线应是一条直线，实测 {segments} 段（量到 y={y}, x{x0}..{x1}）——\
         分段画会在交界处留缝"
    );
}
