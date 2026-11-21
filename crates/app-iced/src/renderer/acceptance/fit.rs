//! 屏幕收缩（M362）：把**请求的窗口尺寸收进显示器可用区**。纯函数、无 IO。
//!
//! ## 为什么要有它（算式，可复核）
//!
//! 窗口尺寸是**逻辑像素**，而「放不放得下」的基准是**显示器物理可用区**：
//!
//! ```text
//! 窗口物理像素 = 逻辑尺寸 × scale              （winit/iced 都按 scale 换算，见下）
//! 放得下        ⇔ 逻辑尺寸 ≤ 物理可用区 / scale（两边同除 scale，等价）
//! ```
//!
//! 三处现场（`WINDOW_SIZE = 1180×760`、`MIN_SIZE = 880×560`，`app/mod.rs`）：
//!
//! | 屏幕 | scale | 桌面逻辑尺寸 | 默认窗物理尺寸 | 溢出 | 最小窗物理尺寸 |
//! |---|---|---|---|---|---|
//! | 1366×768 | 125% | 1092.8×614.4 | 1475×950 | **109×182** | 1100×700 |
//! | 1366×768 | 150% | 910.7×512.0 | 1770×1140 | **404×372** | 1320×**840** |
//! | 1024×600 | 100% | 1024×600 | 1180×760 | **156×160** | 880×560（刚好放得下） |
//!
//! 两处下界结论：
//!
//! 1. **1366×768@150% 连最小窗都放不下**（最小窗物理高 840 > 屏高 768）——固守 `MIN_SIZE`
//!    等于在这类屏上永远看不到完整窗口；`1024×600@125%`（逻辑 819×480）同理。
//! 2. 所以「可用区 < `MIN_SIZE`」时**以可用区为准**，并同步放宽 `min_size`（理由见
//!    「为什么 min 要一起放宽」）。
//!
//! ## 策略（三条，判据表在 `fit_tests.rs`）
//!
//! 1. 放得下 ⇒ **一个像素都不动**（`FitReason::Fits`）。100% DPI / 大屏下开窗尺寸与今天完全相同。
//! 2. 某一维放不下 ⇒ **按边缩**（逐维取 `min(请求, 可用区)`），最小尺寸不变（`Shrunk`）。
//! 3. 可用区小于 `MIN_SIZE` ⇒ 以可用区为准、**放宽最小尺寸**（`BelowMin`）。
//!
//! 为什么按边而不是等比：界面是响应式的（断点 600 / 840 走 `pages::layout`），缩完仍是合法布局；
//! 逐维裁掉能保住最大面积，等比会因为一条边的瓶颈白丢另一条边的像素（1366×768@125% 下等比只剩
//! 954×614，按边是 1093×614）。
//!
//! ## 为什么 min 要一起放宽（不这么做等于空转）
//!
//! winit 的 `set_min_inner_size` 会夹住 `request_inner_size`：`iced_winit` 把 `Action::Resize`
//! 与 `Action::SetMinSize` 都换算成物理 px 交给 winit（`iced_winit-0.14.1/src/lib.rs:1447-1470`）。
//! 可用区小于 `MIN_SIZE` 时只改 size、不改 min，窗口会停在 880×560 —— 日志说「缩到 800×500」，
//! 屏幕上还是 880×560，正是本仓反复踩的「日志说请求、报告当结果」。
//!
//! ## 「可用区」这个名字的诚实边界（M371 之后）
//!
//! 真正的可用区要扣掉任务栏（Windows 的 `MONITORINFO::rcWork`）。iced 0.14 只给
//! [`iced::window::monitor_size`]（= 显示器**全**逻辑尺寸，`iced_winit-0.14.1/src/lib.rs:1665-1677`
//! 用的是 `monitor.size()` 而不是工作区；winit 0.30.13 的 `MonitorHandle` 也没暴露工作区，
//! 见 `winit-0.30.13/src/monitor.rs:112-166`），所以 M371 另开一个落点去读 `rcWork`
//! （[`crate::platform::win32::work_area`]）：Windows 上喂进来的是**真工作区**；macOS / Linux 上
//! 仍是显示器全尺寸（**上界**：保证落在屏内，但任务栏那一侧仍可能被压住几十逻辑 px——那两个平台
//! 没有实现，不许编值）。日志里 `work_area_source=` 就是为了一眼分清这几件事
//! （`win32-rcwork` / `monitor` / `monitor:probe-failed` / `env` / `env:invalid` / `none`），
//! 同行的 `monitor=` 是显示器全尺寸：
//! 两块值一起看才知道任务栏到底扣了多少 px。
//!
//! ## 位置（M372）：第二个纯函数，与尺寸**并列**
//!
//! M362 / M371 只改尺寸，而产品窗口不设 position ⇒ iced 的 `Position::Default` 在 Windows 上是
//! **级联位**（本机实测每次启动 +26 物理 px：26/52/78/156），于是「尺寸已收进可用区」的窗口仍会因
//! **顶边偏移**把底边顶出去：`--scale-factor 2` 下窗口物理 2360×1224 落在 y=260 ⇒ 底边 1484 >
//! `rcWork` 底边 1224（溢出 260 物理 px，其中 48 px 正落在任务栏带里）。
//! 位置因此单独一个纯函数 [`place_in_work_area`]（[`fit_to_work_area`] 一个字没改）：
//!
//! ```text
//! 居中位 = 可用区起点 + (可用区尺寸 − 窗口尺寸) ÷ 2      ← 逐维；越界推回边界
//! ```
//!
//! 为什么居中而不是「越界才推回」：级联位由系统计数器驱动，每次启动都不同，「夹进可用区」会让
//! 位置随上一次启动漂移、取证无法逐字节比较；居中只依赖 (窗口尺寸, 可用区)。反例与取舍见
//! `DevDocs/artifacts/winpos/REPORT.md`。
//!
//! **M386 的搬移**：定位那一整段（`WorkRect` / `place_in_work_area` / `centered` / 五档
//! `reason=` 取值 / `place_line` / `report_place`）已**纯搬移**到 [`super::place`]，本文件只把它
//! `pub use` 回来（`acceptance::fit::<名字>` 这条路不变 ⇒ 调用方一个字没改）。搬的理由是行数：
//! 本文件当时 397 行、余量 3，I7 要加第五档 `no-window` 加不进去。判据表也一起搬到了
//! `place_tests.rs`。

use super::window::parse_window_size;

/// 可用区覆盖（`ULL_WORK_AREA=1093x614`，逻辑 px）。
///
/// 存在的两个理由，都是「不许猜」：
///
/// 1. **取证**：收缩路径只在「屏幕比窗小」时才走，开发机常年 1080p 以上 ⇒ 没有这个旋钮就永远
///    跑不到那段代码。用它可以把宿主显示器换成一个小的可用区，**真开窗**看收缩后的窗口。
/// 2. **兜底**：`window::monitor_size` 返回 `None`（拿不到显示器）时，运维可以显式钉一个值。
///
/// 与 [`super::window::WINDOW_SIZE_ENV`] 同一套语义：只影响这一次进程，且**只在自动尺寸档生效**
/// （见 [`fit_skip_reason`]）。来源进日志的 `work_area_source=env`，不会与真显示器混淆。
pub const WORK_AREA_ENV: &str = "ULL_WORK_AREA";

/// `[window] fit` 这一行里的尺寸来源取值（`size_source=` 的第三个族）。
///
/// 今天 `size_source=` 的取值：`default` / `termux-screen` / `ULL_WINDOW_SIZE`
/// （`:clamped-to-min` / `:invalid`）/ **`screen-fit`**。前两类在 `[window] requested` 行上，
/// 本值只出现在开窗**之后**的 `[window] fit` 行上——因为收缩要靠 iced 拿到显示器，那件事只有
/// 窗口存在之后才能问（见 `app/boot/window_fit` 的模块文档）。
pub const SIZE_SOURCE_SCREEN_FIT: &str = "screen-fit";

/// 收缩判据的结论（进日志 `reason=`，也是判据表的「期望输出」列）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitReason {
    /// 请求尺寸本来就放得下：不动。
    Fits,
    /// 某一维放不下、按边缩到可用区，最小尺寸不变（可用区仍 ≥ `MIN_SIZE`）。
    Shrunk,
    /// 可用区比 `MIN_SIZE` 还小：以可用区为准，并放宽最小尺寸。
    BelowMin,
    /// 没有可用的可用区（拿不到显示器 / 值非法）：不动，理由写进日志。
    Unknown,
}

impl FitReason {
    /// 日志取值。
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Fits => "fits",
            Self::Shrunk => "shrunk",
            Self::BelowMin => "below-min",
            Self::Unknown => "unknown",
        }
    }

    /// 要不要真去动窗口（`Fits` / `Unknown` 不动一个像素 —— M362 的回归口径）。
    #[must_use]
    pub fn adjusts(self) -> bool {
        matches!(self, Self::Shrunk | Self::BelowMin)
    }
}

/// [`fit_to_work_area`] 的产物：收缩后的尺寸 + **应当生效的最小尺寸** + 结论。
///
/// 为什么 `min` 要跟着回来：可用区小于 `MIN_SIZE` 时必须把它一起放宽，否则 winit 会把 resize
/// 夹回去（模块文档「为什么 min 要一起放宽」）。只返回 `size` 的签名会让调用方漏掉这一步。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fit {
    /// 收缩后的窗口尺寸（逻辑 px）。
    pub size: (f32, f32),
    /// 收缩后应当生效的最小尺寸（逻辑 px；正常档等于传进来的 `min`）。
    pub min: (f32, f32),
    /// 结论。
    pub reason: FitReason,
}

/// 把 `size` 收进 `work_area`（全部是**逻辑 px**）。纯函数：无 IO、无环境、可单测。
///
/// `work_area = None` 或值非法（非有限 / ≤0）⇒ 结论 `Unknown`，`size` / `min` 原样返回。
#[must_use]
pub fn fit_to_work_area(size: (f32, f32), min: (f32, f32), work_area: Option<(f32, f32)>) -> Fit {
    let Some(area) = work_area.filter(|(width, height)| usable(*width) && usable(*height)) else {
        return Fit {
            size,
            min,
            reason: FitReason::Unknown,
        };
    };

    if area.0 >= size.0 && area.1 >= size.1 {
        return Fit {
            size,
            min,
            reason: FitReason::Fits,
        };
    }

    let fitted = (size.0.min(area.0), size.1.min(area.1));
    // 逐维判：只要有一维装不下 MIN_SIZE，固守 min 就等于让窗口永远超屏（1366×768@150% 就是）。
    let reason = if area.0 < min.0 || area.1 < min.1 {
        FitReason::BelowMin
    } else {
        FitReason::Shrunk
    };
    Fit {
        size: fitted,
        min: (min.0.min(fitted.0), min.1.min(fitted.1)),
        reason,
    }
}

/// 可用区在**逻辑**坐标下的矩形、居中算式、`reason=` 的五档取值与 `[window] place` 日志行
/// 都在 [`super::place`]（M386 纯搬移：`fit.rs` 当时余量 3，I7 加不进新档）。
pub use super::place::{
    place_in_work_area, place_line, report_place, WorkRect, PLACE_CENTERED, PLACE_NO_WINDOW,
    PLACE_NO_WORK_AREA, PLACE_PROBE_FAILED, PLACE_UNKNOWN_AREA,
};

/// 可用区从哪来：`raw` = `ULL_WORK_AREA` 的原值，`work` = Windows 的**工作区**（`rcWork`），
/// `monitor` = iced 给的显示器逻辑尺寸，`probe_failed` = 本平台**有**工作区探测但它这一次没返回
/// （[`crate::platform::WORK_AREA_IMPLEMENTED`] 为真且 `work` 为空）。
///
/// 返回 `(可用区, 来源)`；来源进日志的 `work_area_source=`：
///
/// | 来源 | 含义 |
/// |---|---|
/// | `env` | 环境覆盖生效（**不是**真显示器，取证/兜底用） |
/// | `env:invalid` | 覆盖写了但解析不了（不静默吞），下面退回 `work`（没有再退 `monitor`） |
/// | `win32-rcwork` | Windows 的**真工作区**（`MONITORINFO::rcWork`，任务栏已扣；M371） |
/// | `monitor:probe-failed` | **本平台有**工作区探测、这一次失败了（布局 / 句柄 bug），退回显示器上界 |
/// | `monitor` | iced 的显示器全逻辑尺寸（macOS / Linux 的唯一来源；也是 Windows 读不到时的上界） |
/// | `none` | 三样都没有 |
///
/// 优先级是**真值 > 上界**：`env` > `win32-rcwork` > `monitor`。`env` 永远最高，
/// 因为它是取证/兜底旋钮（`ULL_WORK_AREA` 的行为在 M371 里必须逐字不变）。
///
/// 为什么 `monitor:probe-failed` 要单独一格：`GetMonitorInfoW` 对写错的 `cbSize` 只返回 FALSE，
/// 失败形态就是「静默回退到显示器尺寸」——与「macOS/Linux 本来就没实现」在日志里必须能分开，
/// 否则真机排障时看不出这条链断在哪。
#[must_use]
pub fn work_area_from(
    raw: Option<&str>,
    work: Option<(f32, f32)>,
    monitor: Option<(f32, f32)>,
    probe_failed: bool,
) -> (Option<(f32, f32)>, &'static str) {
    if let Some(text) = raw {
        return match parse_window_size(text) {
            Some(area) => (Some(area), "env"),
            None => (work.or(monitor), "env:invalid"),
        };
    }
    if let Some(area) = work {
        return (Some(area), "win32-rcwork");
    }
    match monitor {
        Some(area) if probe_failed => (Some(area), "monitor:probe-failed"),
        Some(area) => (Some(area), "monitor"),
        None => (None, "none"),
    }
}

/// [`work_area_from`] 的读环境版本（`app::boot::window_fit` 用它，测试用纯的那个）。
#[must_use]
pub fn work_area(
    work: Option<(f32, f32)>,
    monitor: Option<(f32, f32)>,
    probe_failed: bool,
) -> (Option<(f32, f32)>, &'static str) {
    let raw = std::env::var(WORK_AREA_ENV).ok();
    let resolved = work_area_from(raw.as_deref(), work, monitor, probe_failed);
    if resolved.1 == "env:invalid" {
        eprintln!(
            "[window] {WORK_AREA_ENV}={:?} 解析不了（要 1093x614 这种），按下面那个来源跑",
            raw.unwrap_or_default()
        );
    }
    resolved
}

/// 这一次要不要做收缩（纯判据）：`None` = 做，`Some(理由)` = 不做，理由进日志 `reason=`。
///
/// 两条「不做」的路，各有硬理由：
///
/// - `skipped-capture`（`--screenshot`）：取证图的尺寸必须只由参数决定。让宿主显示器参与，
///   D30 矩阵的逐字节对照就会随机器变（同一份参数在两台机上出两张不同尺寸的图）。
/// - `skipped-source`（`ULL_WINDOW_SIZE` / `termux-screen`）：那两条是**显式**尺寸决定
///   （矩阵的「最小 / 宽屏」两档靠 env，Termux 自己按 X 屏幕算过一遍），被静默改写等于
///   「脚本以为在测宽屏，其实测的是宿主屏」。
///
/// 顺序：取证优先（取证 + 覆盖同时成立时报 `skipped-capture`，那才是这次进程的性质）。
#[must_use]
pub fn fit_skip_reason(size_source: &str, capture: bool) -> Option<&'static str> {
    if capture {
        return Some("skipped-capture");
    }
    (size_source != "default").then_some("skipped-source")
}

/// 见 [`report_fit`]。
///
/// 一行里同时写 `work_area=`（真正喂给判据的那一块）与 `monitor=`（显示器全尺寸）：
/// 两块值的差就是任务栏/停靠栏占掉的像素——「可用区 < 显示器尺寸」这件事因此**不依赖外部工具**
/// 就能在日志里自证（M371 的验收判据 ④）。
#[must_use]
pub fn fit_line(
    requested: (f32, f32),
    fit: &Fit,
    work_area: Option<(f32, f32)>,
    work_area_source: &str,
    monitor: Option<(f32, f32)>,
) -> String {
    format!(
        "[window] fit size_source={SIZE_SOURCE_SCREEN_FIT} requested={} size={} min={} \
         work_area={} work_area_source={work_area_source} monitor={} reason={}",
        dim(requested),
        dim(fit.size),
        dim(fit.min),
        work_area.map_or_else(|| "<none>".to_owned(), dim),
        monitor.map_or_else(|| "<none>".to_owned(), dim),
        fit.reason.name(),
    )
}

/// 见 [`report_skip`]。
#[must_use]
pub fn skip_line(requested: (f32, f32), min: (f32, f32), size_source: &str, why: &str) -> String {
    format!(
        "[window] fit size_source={size_source} requested={} size={} min={} \
         work_area=<none> work_area_source=skipped monitor=<none> reason={why}",
        dim(requested),
        dim(requested),
        dim(min),
    )
}

/// `[window] fit`：收缩怎么定的（**与真窗口同源**：调用方拿同一个 [`Fit`] 去 resize）。
pub fn report_fit(
    requested: (f32, f32),
    fit: &Fit,
    work_area: Option<(f32, f32)>,
    work_area_source: &str,
    monitor: Option<(f32, f32)>,
) {
    eprintln!(
        "{}",
        fit_line(requested, fit, work_area, work_area_source, monitor)
    );
}

/// `[window] fit`：这次没做收缩，以及为什么。
pub fn report_skip(requested: (f32, f32), min: (f32, f32), size_source: &str, why: &str) {
    eprintln!("{}", skip_line(requested, min, size_source, why));
}

/// 可用区数值的合法性：有限且 > 0（`to_logical` 在某些虚拟屏上会给 0）。
///
/// `pub(super)`：`place.rs` 的 [`place_in_work_area`] 用的是**同一份**合法性判据（两处各写一遍
/// 迟早会分叉）。
pub(super) fn usable(value: f32) -> bool {
    value.is_finite() && value > 0.0
}

/// `1180x760` 这种一格写法（与 `[window] requested` 行同款，脚本只用一套解析；`place.rs` 也用它）。
pub(super) fn dim(size: (f32, f32)) -> String {
    format!("{}x{}", size.0, size.1)
}

#[cfg(test)]
#[path = "fit_tests.rs"]
mod tests;
