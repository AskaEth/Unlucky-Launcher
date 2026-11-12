//! 右侧助手栏的**布局数学**：三档宽度、磁铁吸附、中栗下限。纯函数 + 常量，不碰 iced 控件树。
//!
//! ## 一屏的横向预算
//!
//! ```text
//! 窗口宽 = 左轨 88 + [中栗] + [拖拽缝 8（仅展开档）] + [右栗]
//! ```
//!
//! - 右栗宽度 = [`rail_width`]：折叠 48 / 展开 280~360（吸附值）/ 拆出 0；
//! - 中栗宽度 = 窗口 − 右栗 − 拖拽缝（[`content_width`]，页面层再扣左轨与页边距）；
//! - **中栗下限** [`min_content_width`] = 行宽上限 424 + 两侧页边距 48 = **472**：
//!   再窄正文列就装不下一行满宽文，所以拖拽被夹在这里（不是「拖到 0 再说」）。
//!
//! 「展开时中栗**收缩而不是溢出**」这句话的落点就是 [`content_width`]：页面布局系统仍以
//! 「一个窗口宽」为输入（它内部扣 rail 与页边距），所以右栗占掉的那一段必须在这里先减掉。
//! 四个对照宽度的实际数值由 `metrics/tests.rs` 的表钉住（报告里贴的是同一张表）。
//!
//! ## 三档预设的取值理由（tower 要求写明，不许默默定）
//!
//! | 档位 | 值 | 依据 |
//! |---|---|---|
//! | 折叠 | **48** | `LayoutMetrics::touch_target`：M3 无障碍下限，图标按钮 40 + 未读点要放得下 |
//! | 展开（锚） | **320** | 令牌 `ai_panel_width`：M69 起这个令牌就叫「AI 面板宽度」，改锚等于改令牌语义 |
//! | 展开 ±1 档 | **280 / 360** | 在 4px 栅格上、离锚正好 40（`Space::X8 + Space::X2`）；280 = 官方 supporting-pane 侧列下限 240 + 两份 20 内边距，仍能排 40 字符/行 |
//!
//! **不取 420**：900 宽窗上右栗上限只有 340（[`max_rail_width`]），420 这个预设永远点不到 = 假档位。
//! **不取 240**：减去两侧 16 内边距只剩 208，一行不到 35 字符，会话气泡会碎成柱状。

use unlucky_design::spacing::{snap_to_grid, LayoutMetrics, Space};

use crate::pages::layout;

/// 折叠态图标条宽 = M3 触控目标 **48**（`LayoutMetrics::touch_target`，不是新字面量）。
pub const COLLAPSED_WIDTH: f32 = LayoutMetrics::DESKTOP.touch_target;

/// 展开态面板宽 = 令牌 `ai_panel_width` = **320**。
pub const DEFAULT_EXPANDED_WIDTH: f32 = LayoutMetrics::DESKTOP.ai_panel_width;

/// 磁铁档位间距 = 一个页级留白（32）+ 一个小留白（8）= **40**。
pub const PRESET_STEP: f32 = Space::X8.dp() + Space::X2.dp();

/// 三个预设档位：**280 / 320 / 360**（理由见模块文档）。
pub const PRESETS: [f32; 3] = [
    DEFAULT_EXPANDED_WIDTH - PRESET_STEP,
    DEFAULT_EXPANDED_WIDTH,
    DEFAULT_EXPANDED_WIDTH + PRESET_STEP,
];

/// 磁铁半径（`Space::X6` = 24）：离预设档位 24px 以内就吸过去，否则按 4px 栅格落地。
///
/// 24 > 档位间距的一半（20），所以 280~360 之间**任何**位置都会被吸到某一档；360 以上到上限
/// 之间不再有档位，那段才走 4px 栅格（用户想要 500 宽的面板也给）。
pub const MAGNET_RADIUS: f32 = Space::X6.dp();

/// 面板与中栗之间的可拖拽缝隙宽（`Space::X2` = 8）。
///
/// 它既是分隔线（1px `OutlineVariant` 居中）也是命中区：不是「看起来细就抓不到」。
pub const RESIZE_GUTTER: f32 = Space::X2.dp();

/// 中栗的最小可用宽度 = 行宽上限 424 + 两侧页边距 24×2 = **472**。
///
/// 依据是布局系统自己的判据（[`layout::measure_cap_px`]）：1180 展开 320 后中栗 **852**，
/// 880（进程最小窗宽）展开 320 后中栗 **552** —— 都还在 472 之上。
pub fn min_content_width() -> f32 {
    layout::measure_cap_px() + 2.0 * Space::X6.dp()
}

/// 右栗能占的最大宽度：中栗不许低于 [`min_content_width`]（拖拽缝也算右栗的开销）。
///
/// 下限取 [`COLLAPSED_WIDTH`]（窗口再窄也不把右栗夹成负数）。
pub fn max_rail_width(window_width: f32) -> f32 {
    (window_width - layout::rail_width() - min_content_width() - RESIZE_GUTTER).max(COLLAPSED_WIDTH)
}

/// 展开宽度的合法化：夹到可用范围。低于 280 就吸到 280（不是变细）。
///
/// 窗口窄到连 280 都不成立时返回 [`COLLAPSED_WIDTH`]——那时「展开」这件事本身不成立，
/// 画一条 280 的栏会把中栗压到 472 以下。
pub fn clamp_width(window_width: f32, requested: f32) -> f32 {
    let ceiling = max_rail_width(window_width);
    if ceiling < PRESETS[0] {
        return COLLAPSED_WIDTH;
    }
    requested.clamp(PRESETS[0], ceiling)
}

/// 拖拽落点 → 吸附后的右栗宽度（磁铁 → 4px 栅格 → 夹紧）。
pub fn magnet_width(window_width: f32, raw: f32) -> f32 {
    let nearest = PRESETS
        .iter()
        .copied()
        .min_by(|left, right| (left - raw).abs().total_cmp(&(right - raw).abs()))
        .unwrap_or(DEFAULT_EXPANDED_WIDTH);

    let settled = if (nearest - raw).abs() <= MAGNET_RADIUS {
        nearest
    } else {
        snap_to_grid(raw)
    };

    clamp_width(window_width, settled)
}

/// 窗口窄到连最小展开档都放不下时，`Expanded` **事实上**就是折叠档。
///
/// 不是「自动帮用户收起」的一时兴起：`clamp_width` 在这种情况下返回的就是 [`COLLAPSED_WIDTH`]，
/// 若仍报 `Expanded`，宿主会画一条 48 宽的面板、还白扣 8px 拖拽缝。
pub fn effective_state(
    window_width: f32,
    state: crate::shell::agent::RailState,
    expanded: f32,
) -> crate::shell::agent::RailState {
    use crate::shell::agent::RailState;
    match state {
        RailState::Expanded if clamp_width(window_width, expanded) <= COLLAPSED_WIDTH => {
            RailState::Collapsed
        }
        other => other,
    }
}

/// 一屏的横向预算（窗口 → 左轨 / 中栗 / 右栗）。纯数据，供单测与日志用。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RailLayout {
    /// 窗口宽度（输入）。
    pub window: f32,
    /// 左侧导航轨宽度（令牌 `nav_rail_width` = 88）。
    pub left: f32,
    /// 右栗面板宽度（0 / 48 / 280~360）。
    pub rail: f32,
    /// 右栗前面的拖拽缝宽度（只有展开档有，其余 0）。
    pub gutter: f32,
    /// 中栗宽度 = 窗口 − 右栗 − 拖拽缝（页面层再扣左轨与页边距）。
    pub content: f32,
}

/// 算一屏的横向预算（档位先经 [`effective_state`] 归一）。
pub fn layout_for(
    window_width: f32,
    state: crate::shell::agent::RailState,
    expanded: f32,
) -> RailLayout {
    use crate::shell::agent::RailState;

    let (rail, gutter) = match effective_state(window_width, state, expanded) {
        RailState::Collapsed => (COLLAPSED_WIDTH, 0.0),
        RailState::Expanded => (clamp_width(window_width, expanded), RESIZE_GUTTER),
        RailState::Detached => (0.0, 0.0),
    };

    RailLayout {
        window: window_width,
        left: layout::rail_width(),
        rail,
        gutter,
        content: (window_width - rail - gutter).max(0.0),
    }
}

/// 右栗面板宽度（`App::view` 与 shell 都用这一个入口，避免两处各算一套）。
pub fn rail_width(window_width: f32, state: crate::shell::agent::RailState, expanded: f32) -> f32 {
    layout_for(window_width, state, expanded).rail
}

/// **交给页面布局系统的宽度** = 窗口 − 右栗 − 拖拽缝。
pub fn content_width(
    window_width: f32,
    state: crate::shell::agent::RailState,
    expanded: f32,
) -> f32 {
    layout_for(window_width, state, expanded).content
}

#[cfg(test)]
#[path = "metrics/tests.rs"]
mod tests;
